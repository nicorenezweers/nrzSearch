//! Die Ablage fuer Chats und Arbeitsbereiche, ohne Anmeldung - Portierung von
//! public/lib/ablage.js.
//!
//! Jeder Browser bekommt beim ersten Besuch ein zufaelliges Token (im localStorage). Damit holt
//! er seine Chats vom Server - verschluesselt mit genau diesem Token (siehe src/ablage.rs). Ohne
//! das Token kommt niemand heran, auch der Server allein nicht. Ein anderer Browser hat ein
//! anderes Token und damit eigene Chats.
//!
//! Schnittstelle: [`chats`], [`chats_setzen`], [`dateien`], [`dateien_setzen`].

use crate::api::{self, Ergebnis};
use serde_json::{Value, json};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;

const MAX_CHATS: usize = 100;

thread_local! {
    static TOKEN: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Die Chats (einmal geladen).
    static CACHE: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
}

fn speicher() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn token_gueltig(t: &str) -> bool {
    t.len() == 66 && t.starts_with("t_") && t[2..].chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// Das Token dieses Browsers (beim ersten Mal neu gewuerfelt und gemerkt).
pub fn token() -> String {
    if let Some(t) = TOKEN.with(|t| t.borrow().clone()) {
        return t;
    }
    let alt = speicher().and_then(|s| s.get_item("nrz_token").ok().flatten()).filter(|t| token_gueltig(t));
    let t = alt.unwrap_or_else(|| {
        let mut roh = [0u8; 32];
        if let Some(k) = web_sys::window().and_then(|w| w.crypto().ok()) {
            let _ = k.get_random_values_with_u8_array(&mut roh);
        }
        let t: String = format!("t_{}", roh.iter().map(|b| format!("{b:02x}")).collect::<String>());
        // Privater Modus: dann gilt es nur fuer diese Sitzung.
        if let Some(s) = speicher() {
            let _ = s.set_item("nrz_token", &t);
        }
        t
    });
    TOKEN.with(|x| *x.borrow_mut() = Some(t.clone()));
    t
}

async fn anfrage(pfad: &str, daten: &Value) -> Ergebnis<Value> {
    api::post(&format!("/api/ablage/{pfad}"), daten, Some(&token())).await
}

/// Alle Chats dieses Browsers (einmal vom Server geholt, dann aus dem Speicher).
pub async fn chats() -> Ergebnis<Vec<Value>> {
    if let Some(c) = CACHE.with(|c| c.borrow().clone()) {
        return Ok(c);
    }
    let mut liste = anfrage("laden", &json!({ "teil": "chats" })).await?.get("daten").and_then(Value::as_array).cloned().unwrap_or_default();
    if liste.is_empty() {
        liste = altes_uebernehmen().await.unwrap_or_default();
    }
    CACHE.with(|c| *c.borrow_mut() = Some(liste.clone()));
    Ok(liste)
}

/// Die Chats speichern (hoechstens 100, die neuesten zuerst).
pub async fn chats_setzen(mut liste: Vec<Value>) -> Ergebnis<()> {
    liste.truncate(MAX_CHATS);
    CACHE.with(|c| *c.borrow_mut() = Some(liste.clone()));
    anfrage("speichern", &json!({ "teil": "chats", "daten": liste })).await.map(|_| ())
}

/// Die Dateien (mit Inhalt) eines Arbeitsbereichs.
pub async fn dateien(id: &str) -> Ergebnis<Value> {
    Ok(anfrage("laden", &json!({ "teil": format!("dateien-{id}") })).await?.get("daten").cloned().unwrap_or(Value::Null))
}

/// Die Dateien eines Arbeitsbereichs speichern (None oder leer = loeschen).
pub async fn dateien_setzen(id: &str, liste: Option<Vec<Value>>) -> Ergebnis<()> {
    let daten = liste.filter(|l| !l.is_empty()).map_or(Value::Null, Value::from);
    anfrage("speichern", &json!({ "teil": format!("dateien-{id}"), "daten": daten })).await.map(|_| ())
}

/// Chats aus aelteren Versionen (nur im Browser) ziehen einmal auf den Server um.
async fn altes_uebernehmen() -> Ergebnis<Vec<Value>> {
    let alt: Vec<Value> = speicher()
        .and_then(|s| s.get_item("nrz_ki_chats").ok().flatten())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    if alt.is_empty() {
        return Ok(alt);
    }
    for c in &alt {
        let Some(id) = c.get("id").and_then(Value::as_str) else { continue };
        if let Some(daten) = idb_dateien(id).await.filter(|d| d.as_array().is_some_and(|a| !a.is_empty())) {
            anfrage("speichern", &json!({ "teil": format!("dateien-{id}"), "daten": daten })).await?;
        }
    }
    anfrage("speichern", &json!({ "teil": "chats", "daten": alt })).await?;
    if let Some(s) = speicher() {
        let _ = s.remove_item("nrz_ki_chats");
        let _ = s.remove_item("nrz_konto");
    }
    if let Some(f) = web_sys::window().and_then(|w| w.indexed_db().ok().flatten()) {
        let _ = f.delete_database("nrzsearch");
    }
    Ok(alt)
}

/// Auf eine IndexedDB-Anfrage warten: das Ergebnis oder None.
async fn idb_warten(req: &web_sys::IdbRequest) -> Option<JsValue> {
    let promise = js_sys::Promise::new(&mut |ok: js_sys::Function, fehler: js_sys::Function| {
        let r = req.clone();
        let bei_ok = Closure::once_into_js(move |_: web_sys::Event| {
            let _ = ok.call1(&JsValue::NULL, &r.result().unwrap_or(JsValue::UNDEFINED));
        });
        let bei_fehler = Closure::once_into_js(move |_: web_sys::Event| {
            let _ = fehler.call0(&JsValue::NULL);
        });
        req.set_onsuccess(Some(bei_ok.unchecked_ref()));
        req.set_onerror(Some(bei_fehler.unchecked_ref()));
    });
    JsFuture::from(promise).await.ok()
}

/// Die Dateien aelterer Versionen lagen in der IndexedDB.
async fn idb_dateien(id: &str) -> Option<Value> {
    let fabrik = web_sys::window()?.indexed_db().ok()??;
    let oeffnen = fabrik.open_with_u32("nrzsearch", 1).ok()?;
    let o2 = oeffnen.clone();
    let neu = Closure::once_into_js(move |_: web_sys::Event| {
        if let Ok(db) = o2.result() {
            let _ = db.unchecked_into::<web_sys::IdbDatabase>().create_object_store("dateien");
        }
    });
    oeffnen.set_onupgradeneeded(Some(neu.unchecked_ref()));
    let db: web_sys::IdbDatabase = idb_warten(&oeffnen).await?.dyn_into().ok()?;
    let lesen = db.transaction_with_str("dateien").ok()?.object_store("dateien").ok()?.get(&JsValue::from_str(id)).ok()?;
    let wert = idb_warten(&lesen).await?;
    db.close();
    if wert.is_undefined() || wert.is_null() {
        return None;
    }
    let text = js_sys::JSON::stringify(&wert).ok()?.as_string()?;
    serde_json::from_str(&text).ok()
}
