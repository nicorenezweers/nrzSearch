//! Der KI-Modus: Gespraeche und Arbeitsbereiche mit Dieter - Portierung des KI-Teils von
//! public/app.js (kiSeite, kiNeu, kiOeffnen, kiFrage, kiKlick, arbeitsbereichStarten …).
//!
//! Jede Frage sucht neu in der Instanz, Dieter liest die ersten Seiten und antwortet mit
//! Quellen. Mit Dateien wird das Gespraech ein Arbeitsbereich: Dieter kennt alle Dateien (PDF,
//! Word, Excel, Bilder …) und hilft damit - zusammenfassen, E-Mails schreiben, auswerten. Die
//! Gespraeche stehen links in der Liste - verschluesselt auf dem Server, nur fuer diesen Browser
//! (Token, siehe bib/ablage.rs).
//!
//! Feste Schnittstelle fuer den Kern: [`KiSeite`], [`arbeitsbereich_starten`], [`weiterfragen`].

use crate::api::Abbruch;
use crate::bib::{ablage, dateien, ki};
use crate::bib::ki::{KiAntwort, Quelle, QuellenZeile};
use crate::i18n::t;
use crate::state::{self, ANHAENGE, Datei, EINGABE, EINSTELLUNGEN, ZUSTAND};
use crate::werkzeug::{KopierKnopf, datum_text, jetzt_ms, zufall};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

const ZUSAMMENFASSEN: &str = "Was steht in den Dateien? Fasse sie kurz zusammen.";

/// Eine Uebersicht mit KI, die hier weitergeht: (Frage, { text, quellen }).
static ANFANG: GlobalSignal<Option<(String, Option<Value>)>> = Signal::global(|| None);
/// Dateien aus der Suchleiste fuer einen neuen Arbeitsbereich.
static START: GlobalSignal<Option<Vec<Datei>>> = Signal::global(|| None);

/// Eine Nachricht im Gespraech ("ich" oder "ki").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Nachricht {
    pub rolle: String,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub quellen: Vec<Quelle>,
}

/// Ein Gespraech (ohne Dateiinhalte - die liegen getrennt in der Ablage).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chat {
    pub id: String,
    pub titel: String,
    pub zeit: f64,
    pub nachrichten: Vec<Nachricht>,
    /// Nur die Angaben: { name, typ, groesse, seiten }.
    pub dateien: Vec<Value>,
}

/// Eine Frage mit der Antwort darunter, wie sie gerade auf der Seite steht.
#[derive(Clone, Debug, Default, PartialEq)]
struct Blase {
    frage: String,
    text: String,
    quellen: Vec<Quelle>,
    status: Option<String>,
    fehler: Option<String>,
    fertig: bool,
}

/// Der Zustand der Seite - alles Signale, darum `Copy` (auch in async-Bloecken).
#[derive(Clone, Copy, PartialEq)]
struct Ki {
    liste: Signal<Vec<Chat>>,
    chat: Signal<Option<Chat>>,
    dateien: Signal<Vec<Datei>>,
    blasen: Signal<Vec<Blase>>,
    lauf: Signal<Option<Abbruch>>,
    /// Zaehlt jedes neue/geoeffnete Gespraech - alte Auftraege hoeren dann auf.
    runde: Signal<u64>,
    bereit: Signal<Option<bool>>,
    /// Die zuletzt gespeicherten Dateien (Name, Groesse) - nur Aenderungen werden gespeichert.
    gespeichert: Signal<Vec<(String, u64)>>,
    feld: Signal<Option<web_sys::HtmlTextAreaElement>>,
    datei_feld: Signal<Option<web_sys::HtmlInputElement>>,
}

fn dateien_von(v: &Value) -> Vec<Datei> {
    v.as_array().into_iter().flatten().filter_map(|d| serde_json::from_value(d.clone()).ok()).collect()
}

fn datei_meta(d: &Datei) -> Value {
    json!({ "name": d.name, "typ": d.typ, "groesse": d.groesse, "seiten": d.seiten })
}

fn datei_sauber(d: &Datei) -> Value {
    json!({ "name": d.name, "typ": d.typ, "groesse": d.groesse, "text": d.text, "daten": d.daten, "seiten": d.seiten })
}

fn dateien_text(n: usize) -> String {
    format!("{n} Datei{}", if n == 1 { "" } else { "en" })
}

fn neue_id() -> String {
    const ZEICHEN: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = jetzt_ms() as u64;
    let mut id = Vec::new();
    while n > 0 {
        id.push(ZEICHEN[(n % 36) as usize]);
        n /= 36;
    }
    id.reverse();
    id.extend((0..4).map(|_| ZEICHEN[zufall(36) as usize]));
    String::from_utf8(id).unwrap_or_default()
}

/// Ein Element in den sichtbaren Bereich holen (nach dem naechsten Zeichnen).
async fn scrollen(selektor: &str, ende: bool, sanft: bool) {
    gloo_timers::future::TimeoutFuture::new(0).await;
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let Ok(alle) = doc.query_selector_all(selektor) else { return };
    let Some(el) = alle.item(alle.length().saturating_sub(1)).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) else { return };
    let o = web_sys::ScrollIntoViewOptions::new();
    o.set_block(if ende { web_sys::ScrollLogicalPosition::End } else { web_sys::ScrollLogicalPosition::Start });
    if sanft {
        o.set_behavior(web_sys::ScrollBehavior::Smooth);
    }
    el.scroll_into_view_with_scroll_into_view_options(&o);
}

impl Ki {
    fn stoppen(mut self) {
        if let Some(a) = self.lauf.write().take() {
            a.abbrechen();
        }
    }

    fn blase(mut self, i: usize, f: impl FnOnce(&mut Blase)) {
        if let Some(b) = self.blasen.write().get_mut(i) {
            f(b);
        }
    }

    /// Ein Gespraech speichern (ganz nach oben in der Liste).
    async fn speichern(mut self, mut chat: Chat) {
        chat.zeit = jetzt_ms();
        let offen = self.chat.peek().as_ref().is_some_and(|c| c.id == chat.id);
        if offen {
            chat.dateien = self.dateien.peek().iter().filter(|d| d.fehler.is_none() && !d.lesen).map(datei_meta).collect();
            self.chat.set(Some(chat.clone()));
        }
        let mut liste = vec![chat.clone()];
        liste.extend(self.liste.peek().iter().filter(|c| c.id != chat.id).cloned());
        self.liste.set(liste.clone());
        let werte = liste.iter().filter_map(|c| serde_json::to_value(c).ok()).collect();
        if let Err(f) = ablage::chats_setzen(werte).await {
            web_sys::console::warn_1(&format!("Speichern: {f}").into());
        }
    }

    /// Die Dateien des offenen Gespraechs speichern (sobald alle gelesen sind).
    async fn dateien_speichern(mut self) {
        let Some(chat) = self.chat.peek().clone() else { return };
        if self.dateien.peek().iter().any(|d| d.lesen) {
            return;
        }
        let gute: Vec<Datei> = self.dateien.peek().iter().filter(|d| d.fehler.is_none()).cloned().collect();
        self.gespeichert.set(gute.iter().map(|d| (d.name.clone(), d.groesse)).collect());
        if let Err(f) = ablage::dateien_setzen(&chat.id, Some(gute.iter().map(datei_sauber).collect())).await {
            web_sys::console::warn_1(&format!("Dateien: {f}").into());
        }
        self.speichern(chat).await;
    }

    /// Ein neues Gespraech - mit der ersten Frage oder leer mit Begruessung.
    fn neu(mut self, frage: String, anfang: Option<Value>, dateien: Vec<Datei>) {
        self.stoppen();
        *self.runde.write() += 1;
        self.chat.set(None);
        self.gespeichert.set(dateien.iter().map(|d| (d.name.clone(), d.groesse)).collect());
        self.dateien.set(dateien);
        self.blasen.set(Vec::new());
        if frage.is_empty() {
            if let Some(f) = self.feld.peek().as_ref() {
                let _ = f.focus();
            }
        } else {
            spawn(self.fragen(frage, anfang));
        }
    }

    /// Ein gespeichertes Gespraech oeffnen.
    async fn oeffnen(mut self, chat: Chat) {
        self.stoppen();
        *self.runde.write() += 1;
        let runde = *self.runde.peek();
        self.chat.set(Some(chat.clone()));
        {
            let mut z = ZUSTAND.write();
            z.q = chat.titel.clone();
        }
        *EINGABE.write() = chat.titel.clone();
        state::url_setzen(&ZUSTAND.peek());
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(&format!("{} - nrzSearch", chat.titel));
        }
        let geladen = if chat.dateien.is_empty() { Vec::new() } else { ablage::dateien(&chat.id).await.map(|v| dateien_von(&v)).unwrap_or_default() };
        if *self.runde.peek() != runde {
            return;
        }
        self.gespeichert.set(geladen.iter().map(|d| (d.name.clone(), d.groesse)).collect());
        self.dateien.set(geladen);
        let blasen = chat
            .nachrichten
            .chunks(2)
            .map(|p| Blase {
                frage: p[0].text.clone(),
                text: p.get(1).map(|a| a.text.clone()).unwrap_or_default(),
                quellen: p.get(1).map(|a| a.quellen.clone()).unwrap_or_default(),
                fertig: true,
                ..Default::default()
            })
            .collect();
        self.blasen.set(blasen);
        scrollen(".ki-gespraech > *", true, false).await;
    }

    /// Eine Frage an Dieter (oder eine fertige Antwort aus der Uebersicht uebernehmen).
    async fn fragen(mut self, frage: String, fertig: Option<Value>) {
        // Das erste Wort macht das Gespraech - es steht ab jetzt in der Liste.
        let neu = self.chat.peek().is_none();
        if neu {
            self.chat.set(Some(Chat { id: neue_id(), titel: frage.clone(), zeit: jetzt_ms(), ..Default::default() }));
        }
        let runde = *self.runde.peek();
        let ab = Abbruch::neu();
        self.lauf.set(Some(ab.clone()));
        let i = {
            let mut b = self.blasen.write();
            b.push(Blase { frage: frage.clone(), status: Some("Suche läuft …".into()), ..Default::default() });
            b.len() - 1
        };
        scrollen(".ki-gespraech .ki-ich", false, true).await;
        // Erst alle Dateien fertig lesen - Dieter soll alles kennen.
        if self.dateien.peek().iter().any(|d| d.lesen) {
            self.blase(i, |b| b.status = Some("Dateien werden gelesen …".into()));
            dateien::warten_bis_gelesen(self.dateien).await;
        }
        if *self.runde.peek() != runde || ab.abgebrochen() {
            return;
        }
        if neu && !self.dateien.peek().is_empty() {
            self.dateien_speichern().await;
        }
        let gute: Vec<Datei> = self.dateien.peek().iter().filter(|d| d.fehler.is_none()).cloned().collect();
        let anhang = dateien::fuer_dieter(&gute);
        let anzahl = |k: &str| anhang.get(k).and_then(Value::as_array).map_or(0, Vec::len);
        let mit_dateien = anzahl("dateien") + anzahl("bilder");
        if mit_dateien > 0 {
            self.blase(i, |b| b.status = Some(format!("Dieter liest {} …", dateien_text(mit_dateien))));
        }
        let mut text = String::new();
        let mut quellen: Vec<Quelle> = Vec::new();
        let mut fehler = String::new();
        if let Some(f) = &fertig {
            text = f.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
            quellen = Quelle::liste(f.get("quellen").unwrap_or(&Value::Null));
        } else {
            let verlauf: Vec<Value> = self.chat.peek().as_ref().map(|c| c.nachrichten.iter().map(|n| json!({ "rolle": n.rolle, "text": n.text })).collect()).unwrap_or_default();
            let anfrage = json!({
                "frage": frage,
                "verlauf": verlauf,
                "sprache": EINSTELLUNGEN.peek().sprache,
                "dateien": anhang["dateien"],
                "bilder": anhang["bilder"],
            });
            let ergebnis = ki::fragen(&anfrage, &ab, |name, d| {
                if *self.runde.peek() != runde {
                    return;
                }
                match name {
                    // Dieter sucht im Hintergrund selbst - die Suchen stehen kurz im Status.
                    "suche" => {
                        let fragen: Vec<String> = d.get("fragen").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
                        if !fragen.is_empty() {
                            self.blase(i, |b| b.status = Some(format!("Dieter sucht: {} …", fragen.join(" · "))));
                        } else if mit_dateien == 0 {
                            let q = d.get("q").and_then(Value::as_str).unwrap_or_default().to_string();
                            self.blase(i, |b| b.status = Some(format!("Suche nach „{q}“ …")));
                        }
                    }
                    "quellen" => {
                        let neue = Quelle::liste(d.get("quellen").unwrap_or(&Value::Null));
                        if !neue.is_empty() {
                            quellen = neue;
                            let n = quellen.len();
                            let qs = quellen.clone();
                            self.blase(i, |b| {
                                b.status = Some(format!("{n} Quellen gelesen - Dieter schreibt …"));
                                b.quellen = qs;
                            });
                        }
                    }
                    "text" => {
                        text.push_str(d.get("t").and_then(Value::as_str).unwrap_or_default());
                        let t = text.clone();
                        self.blase(i, |b| b.text = t);
                    }
                    "fehler" => fehler = d.get("fehler").and_then(Value::as_str).unwrap_or_default().to_string(),
                    _ => {}
                }
            })
            .await;
            if ab.abgebrochen() {
                return;
            }
            if let Err(f) = ergebnis {
                if fehler.is_empty() {
                    fehler = f.text;
                }
            }
        }
        if self.lauf.peek().as_ref() == Some(&ab) {
            self.lauf.set(None);
        }
        if *self.runde.peek() != runde {
            return;
        }
        if text.is_empty() {
            let f = if fehler.is_empty() { "Keine Antwort.".to_string() } else { fehler };
            self.blase(i, |b| {
                b.status = None;
                b.fehler = Some(f);
            });
            return;
        }
        let (t2, q2) = (text.clone(), quellen.clone());
        self.blase(i, |b| {
            b.status = None;
            b.text = t2;
            b.quellen = q2;
            b.fertig = true;
        });
        // Gespeichert wird nur, was zum Wiederlesen noetig ist.
        let Some(mut chat) = self.chat.peek().clone() else { return };
        let quellen = quellen.into_iter().map(|q| Quelle { nr: q.nr, titel: q.titel, url: q.url, host: q.host }).collect();
        chat.nachrichten.push(Nachricht { rolle: "ich".into(), text: frage, quellen: Vec::new() });
        chat.nachrichten.push(Nachricht { rolle: "ki".into(), text, quellen });
        self.speichern(chat).await;
    }

    /// Die Seite fuer die aktuelle Frage aufbauen: weiter aus der Uebersicht, mit Dateien aus
    /// der Suchleiste, ein gespeichertes Gespraech zu dieser Frage (z. B. nach dem Neuladen) -
    /// oder neu.
    async fn starten(mut self) {
        self.stoppen();
        let liste: Vec<Chat> = ablage::chats().await.unwrap_or_default().into_iter().filter_map(|c| serde_json::from_value(c).ok()).collect();
        self.liste.set(liste.clone());
        let bereit = ki::bereit().await;
        self.bereit.set(Some(bereit));
        if !bereit || ZUSTAND.peek().kategorie != "ki" {
            return;
        }
        let q = ZUSTAND.peek().q.clone();
        let anfang = ANFANG.write().take().filter(|(f, _)| *f == q);
        let weiter = anfang.is_some();
        let anfang = anfang.and_then(|(_, a)| a);
        let start = START.write().take();
        let alt = if !weiter && start.is_none() { liste.iter().find(|c| c.titel == q).cloned() } else { None };
        match alt {
            Some(c) => self.oeffnen(c).await,
            None => self.neu(q, anfang, start.unwrap_or_default()),
        }
    }

    /// Absenden aus dem Eingabefeld.
    fn absenden(self, eingabe: &mut Signal<String>) {
        let mut frage = eingabe.peek().trim().to_string();
        if frage.is_empty() && !self.dateien.peek().is_empty() && self.chat.peek().as_ref().is_none_or(|c| c.nachrichten.is_empty()) {
            frage = ZUSAMMENFASSEN.into();
        }
        if frage.is_empty() || self.lauf.peek().is_some() {
            return;
        }
        eingabe.set(String::new());
        if let Some(f) = self.feld.peek().as_ref() {
            let _ = f.style().set_property("height", "auto");
        }
        spawn(self.fragen(frage, None));
    }

    /// Eine Unterhaltung loeschen (nach Rueckfrage).
    fn loeschen(mut self, id: String) {
        let ok = web_sys::window().and_then(|w| w.confirm_with_message("Diese Unterhaltung löschen?").ok()).unwrap_or(false);
        if !ok {
            return;
        }
        let liste: Vec<Chat> = self.liste.peek().iter().filter(|c| c.id != id).cloned().collect();
        self.liste.set(liste.clone());
        let id2 = id.clone();
        spawn(async move {
            let _ = ablage::chats_setzen(liste.iter().filter_map(|c| serde_json::to_value(c).ok()).collect()).await;
            let _ = ablage::dateien_setzen(&id2, None).await;
        });
        if self.chat.peek().as_ref().is_some_and(|c| c.id == id) {
            self.neu(String::new(), None, Vec::new());
        }
    }
}

/// Die Seite des KI-Modus (Reiter "ki"); die Frage steht in `ZUSTAND.q`.
#[component]
pub fn KiSeite() -> Element {
    let ki = Ki {
        liste: use_signal(Vec::new),
        chat: use_signal(|| None),
        dateien: use_signal(Vec::new),
        blasen: use_signal(Vec::new),
        lauf: use_signal(|| None),
        runde: use_signal(|| 0),
        bereit: use_signal(|| None),
        gespeichert: use_signal(Vec::new),
        feld: use_signal(|| None),
        datei_feld: use_signal(|| None),
    };
    let mut eingabe = use_signal(String::new);
    let mut ziehen = use_signal(|| false);
    let mut zone: Signal<Option<web_sys::Element>> = use_signal(|| None);

    // Jede neue Suche im Reiter "ki" (auch dieselbe Frage noch einmal) baut die Seite neu auf.
    let lauf = ZUSTAND.read().lauf;
    use_effect(use_reactive((&lauf,), move |_| {
        spawn(ki.starten());
    }));
    // Dateien geaendert (hinzu, entfernt, fertig gelesen): speichern.
    use_effect(move || {
        let d = ki.dateien.read();
        if d.iter().any(|x| x.lesen) || ki.chat.peek().is_none() {
            return;
        }
        let jetzt: Vec<(String, u64)> = d.iter().filter(|x| x.fehler.is_none()).map(|x| (x.name.clone(), x.groesse)).collect();
        if jetzt != *ki.gespeichert.peek() {
            spawn(ki.dateien_speichern());
        }
    });
    use_drop(move || ki.stoppen());

    let liste = ki.liste.read().clone();
    let aktiv = ki.chat.read().as_ref().map(|c| c.id.clone());
    let dateien_jetzt = ki.dateien.read().clone();
    let blasen = ki.blasen.read().clone();
    let bereit = *ki.bereit.read();
    let hallo = blasen.is_empty() && ki.chat.read().is_none();
    let frag = t("frag");
    let (neuer_chat, hallo_titel, hallo_text) = (t("neuer_chat"), t("hallo"), t("hallo_text"));
    let seite_klasse = if ziehen() { "ki-seite ziehen" } else { "ki-seite" };

    let hinzu = move |dateien: Vec<web_sys::File>| {
        if !dateien.is_empty() {
            spawn(dateien::hinzufuegen_zu(ki.dateien, dateien));
        }
    };

    rsx! {
        main { id: "inhalt", class: "ki-modus",
            div { class: "ki-layout",
                aside { class: "ki-leiste", "aria-label": "Unterhaltungen",
                    button { r#type: "button", class: "ki-neu", onclick: move |_| ki.neu(String::new(), None, Vec::new()),
                        "＋ {neuer_chat}"
                    }
                    div { class: "ki-liste", id: "ki-liste",
                        if liste.is_empty() {
                            p { class: "ki-leer", "Noch keine Unterhaltungen." }
                        }
                        for c in liste {
                            {
                                let id = c.id.clone();
                                let id_weg = c.id.clone();
                                let wann = datum_text(&String::from(js_sys::Date::new(&c.zeit.into()).to_iso_string()));
                                let n = c.dateien.len();
                                let klasse = if aktiv.as_deref() == Some(c.id.as_str()) { "ki-eintrag aktiv" } else { "ki-eintrag" };
                                let chat = c.clone();
                                rsx! {
                                    div { key: "{id}", class: "{klasse}",
                                        button {
                                            r#type: "button",
                                            "data-ki-chat": "{id}",
                                            onclick: move |_| {
                                                if ki.chat.peek().as_ref().map(|x| &x.id) != Some(&chat.id) {
                                                    spawn(ki.oeffnen(chat.clone()));
                                                }
                                            },
                                            span {
                                                if n > 0 {
                                                    i { class: "ki-mappe", title: "Arbeitsbereich mit Dateien" }
                                                }
                                                "{c.titel}"
                                            }
                                            small {
                                                "{wann}"
                                                if n > 0 { " · {dateien_text(n)}" }
                                            }
                                        }
                                        button {
                                            r#type: "button",
                                            class: "ki-weg",
                                            "aria-label": "Unterhaltung löschen",
                                            title: "Löschen",
                                            onclick: move |_| ki.loeschen(id_weg.clone()),
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    class: "{seite_klasse}",
                    onmounted: move |e| zone.set(e.data().downcast::<web_sys::Element>().cloned()),
                    // Dateien: Bueroklammer, "＋ Dateien" oder einfach auf das Gespraech ziehen.
                    ondragover: move |e| {
                        if bereit == Some(true) && dateien::zieht_dateien(&e) {
                            e.prevent_default();
                            ziehen.set(true);
                        }
                    },
                    ondragleave: move |e| {
                        let drin = e
                            .data()
                            .downcast::<web_sys::DragEvent>()
                            .and_then(|d| d.related_target())
                            .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
                            .is_some_and(|n| zone.peek().as_ref().is_some_and(|z| z.contains(Some(&n))));
                        if !drin {
                            ziehen.set(false);
                        }
                    },
                    ondrop: move |e| {
                        let liste = dateien::ablage_dateien(&e);
                        if liste.is_empty() || bereit != Some(true) {
                            return;
                        }
                        e.prevent_default();
                        ziehen.set(false);
                        hinzu(liste);
                    },
                    if bereit == Some(false) {
                        // Noch kein OpenRouter-Schluessel: das richtet der Admin ein (/admin).
                        div { class: "ki-hallo",
                            span { class: "ki-stern" }
                            h2 { "Dieter ist hier noch nicht eingerichtet." }
                        }
                    } else {
                        if !dateien_jetzt.is_empty() {
                            div { class: "ki-arbeit", id: "ki-arbeit",
                                div { class: "ki-arbeit-kopf",
                                    i { class: "ki-mappe" }
                                    b { "Arbeitsbereich" }
                                    span { class: "ki-arbeit-zahl", "{dateien_text(dateien_jetzt.len())}" }
                                    button {
                                        r#type: "button",
                                        class: "ki-arbeit-plus",
                                        onclick: move |_| {
                                            if let Some(f) = ki.datei_feld.peek().as_ref() {
                                                f.click();
                                            }
                                        },
                                        "＋ Dateien"
                                    }
                                }
                                dateien::ChipListe { liste: ki.dateien, class: "ki-arbeit-dateien" }
                            }
                        }
                        div { class: "ki-gespraech", id: "ki-gespraech",
                            if hallo {
                                div { class: "ki-hallo",
                                    span { class: "ki-stern" }
                                    h2 { "{hallo_titel}" }
                                    p { "{hallo_text}" }
                                }
                            }
                            for (n, b) in blasen.into_iter().enumerate() {
                                BlaseAnsicht { key: "{n}", blase: b, dateien: dateien_jetzt.clone() }
                            }
                        }
                        if bereit == Some(true) {
                            form {
                                class: "ki-eingabe",
                                id: "ki-eingabe",
                                onsubmit: move |e| {
                                    e.prevent_default();
                                    ki.absenden(&mut eingabe);
                                },
                                button {
                                    r#type: "button",
                                    class: "anhang-knopf",
                                    "aria-label": "Dateien anhängen",
                                    title: "Dateien anhängen - oder einfach hierher ziehen",
                                    onclick: move |_| {
                                        if let Some(f) = ki.datei_feld.peek().as_ref() {
                                            f.click();
                                        }
                                    },
                                    svg { view_box: "0 0 24 24", "aria-hidden": "true", path { fill: "currentColor", d: dateien::KLAMMER } }
                                }
                                textarea {
                                    rows: "1",
                                    placeholder: "{frag}",
                                    "aria-label": "{frag}",
                                    value: "{eingabe}",
                                    onmounted: move |e| {
                                        let mut feld = ki.feld;
                                        feld.set(e.data().downcast::<web_sys::Element>().and_then(|el| el.clone().dyn_into().ok()));
                                    },
                                    oninput: move |e| {
                                        eingabe.set(e.value());
                                        if let Some(f) = ki.feld.peek().as_ref() {
                                            let _ = f.style().set_property("height", "auto");
                                            let _ = f.style().set_property("height", &format!("{}px", f.scroll_height()));
                                        }
                                    },
                                    onkeydown: move |e| {
                                        if e.key() == Key::Enter && !e.modifiers().shift() {
                                            e.prevent_default();
                                            ki.absenden(&mut eingabe);
                                        }
                                    },
                                }
                                button { r#type: "submit", "aria-label": "Senden", "➤" }
                            }
                        }
                    }
                    input {
                        r#type: "file",
                        multiple: true,
                        hidden: true,
                        onmounted: move |e| {
                            let mut f = ki.datei_feld;
                            f.set(e.data().downcast::<web_sys::Element>().and_then(|el| el.clone().dyn_into().ok()));
                        },
                        onchange: move |e| hinzu(dateien::feld_dateien(&e)),
                    }
                }
            }
        }
    }
}

/// Eine Frage mit Dieters Antwort.
#[component]
fn BlaseAnsicht(blase: Blase, dateien: Vec<Datei>) -> Element {
    let Blase { frage, text, quellen, status, fehler, fertig } = blase;
    rsx! {
        div { class: "ki-ich", "{frage}" }
        div { class: "ki-antwort",
            div { class: "ki-wer", span { class: "ki-stern" } "Dieter" }
            if let Some(s) = status {
                div { class: "ki-status", "{s}" }
            }
            div { class: "ki-text",
                if let Some(f) = fehler {
                    p { class: "ki-fehler", "{f}" }
                } else {
                    KiAntwort { text: text.clone(), quellen: quellen.clone(), dateien }
                }
            }
            if fertig {
                div { class: "ki-aktionen",
                    if !quellen.is_empty() {
                        QuellenZeile { quellen: quellen.clone() }
                    }
                    KopierKnopf { text: text.clone(), beschriftung: t("kopieren"), class: "ki-antwort-kopieren" }
                }
            }
        }
    }
}

/// Dateien aus der Suchleiste (`ANHAENGE`) + Text: einen neuen Arbeitsbereich im KI-Modus oeffnen.
/// Erst werden alle Dateien fertig gelesen; ist keine lesbar, wird normal gesucht.
pub fn arbeitsbereich_starten(text: String) {
    dioxus::core::spawn_forever(async move {
        dateien::warten_bis_gelesen(ANHAENGE.signal()).await;
        let gute: Vec<Datei> = ANHAENGE.peek().iter().filter(|d| d.fehler.is_none()).cloned().collect();
        ANHAENGE.write().clear();
        if gute.is_empty() {
            state::suchen(&text);
            return;
        }
        *START.write() = Some(gute);
        let q = if text.trim().is_empty() { ZUSAMMENFASSEN.to_string() } else { text.trim().to_string() };
        {
            let mut z = ZUSTAND.write();
            z.q = q.clone();
            z.kategorie = "ki".into();
            z.seite = 1;
            z.lauf += 1;
        }
        *EINGABE.write() = q;
        state::url_setzen(&ZUSTAND.peek());
    });
}

/// "Im KI-Modus weiterfragen" aus der Uebersicht mit KI: neues Gespraech, das mit `anfang`
/// (Frage, Antwort, Quellen der Uebersicht: `{ text, quellen }`) beginnt.
pub fn weiterfragen(frage: String, anfang: Option<Value>) {
    *ANFANG.write() = Some((frage.trim().to_string(), anfang.filter(|a| !a.is_null())));
    state::kategorie_setzen("ki");
    state::url_setzen(&ZUSTAND.peek());
}
