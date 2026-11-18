//! Modul "regex": "RegEx Tester", "regex \d+" - ein regulaerer Ausdruck
//! gegen einen Text, mit markierten Treffern und Gruppen.
//! Portierung von public/module/regex.js - mit der RegExp-Engine des Browsers.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss};
use dioxus::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};

const CSS: &str = "
.re-strich { color: var(--gedimmt); font-size: 18px; }
.re-muster { flex: 1; font-family: ui-monospace, Menlo, Consolas, monospace; }
.re-flags { width: 64px; font-family: ui-monospace, Menlo, Consolas, monospace; }
.re-ausgabe { white-space: pre-wrap; background: var(--flaeche2); border-radius: var(--r); padding: 10px 12px; margin-top: 10px; font-family: ui-monospace, Menlo, Consolas, monospace; font-size: 13px; }
.re-ausgabe mark { background: #fbbc0466; color: inherit; border-radius: 2px; }
";

const BEISPIEL: &str = "nrzSearch findet Treffer seit 2024 – Version 1.5, Kontakt: info@example.org";

/// Ein Treffer: Position (UTF-16), ganzer Treffer, Gruppen.
struct Treffer {
    index: usize,
    ganz: String,
    gruppen: Vec<Option<String>>,
}

/// Das Ergebnis der Pruefung: Stuecke der Ausgabe (Text, markiert?), Status, Fehler?
struct Pruefung {
    stuecke: Vec<(String, bool)>,
    status: String,
    fehler: bool,
}

/// new RegExp(muster, flags) - wirft der Browser, kommt seine Meldung zurueck.
fn regexp(muster: &str, flags: &str) -> Result<js_sys::RegExp, String> {
    let bauer = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("RegExp")).map_err(|_| "RegExp fehlt".to_string())?;
    let argumente = js_sys::Array::of2(&JsValue::from_str(muster), &JsValue::from_str(flags));
    let bauer: &js_sys::Function = bauer.unchecked_ref();
    js_sys::Reflect::construct(bauer, &argumente).map(JsCast::unchecked_into).map_err(|e| {
        e.dyn_ref::<js_sys::Error>().map(|e| String::from(e.message())).or_else(|| e.as_string()).unwrap_or_default()
    })
}

/// Wie [...text.matchAll(re)].slice(0, 500): exec in einer Schleife, Leer-Treffer ruecken weiter.
fn alle_treffer(re: &js_sys::RegExp, text: &str, einheiten: &[u16], unicode: bool) -> Vec<Treffer> {
    let mut aus = Vec::new();
    re.set_last_index(0);
    while aus.len() < 500 {
        let Some(m) = re.exec(text) else { break };
        let index = js_sys::Reflect::get(&m, &JsValue::from_str("index")).ok().and_then(|i| i.as_f64()).unwrap_or(0.0) as usize;
        let ganz = m.get(0).as_string().unwrap_or_default();
        let gruppen = (1..m.length()).map(|i| m.get(i).as_string()).collect();
        if ganz.is_empty() {
            // Wie AdvanceStringIndex: ein Zeichen weiter (mit u ein ganzes Ersatzpaar).
            let li = re.last_index() as usize;
            let paar = unicode
                && einheiten.get(li).is_some_and(|c| (0xD800..0xDC00).contains(c))
                && einheiten.get(li + 1).is_some_and(|c| (0xDC00..0xE000).contains(c));
            re.set_last_index((li + if paar { 2 } else { 1 }) as u32);
        }
        aus.push(Treffer { index, ganz, gruppen });
    }
    aus
}

fn pruefen(muster: &str, flags_roh: &str, text: &str) -> Pruefung {
    let mut flags = String::new();
    for c in flags_roh.chars().filter(|c| "gimsuyd".contains(*c)).chain(['g']) {
        if !flags.contains(c) {
            flags.push(c);
        }
    }
    let re = match regexp(muster, &flags) {
        Ok(re) => re,
        Err(meldung) => return Pruefung { stuecke: vec![(text.to_string(), false)], status: format!("✗ {meldung}"), fehler: true },
    };
    let einheiten: Vec<u16> = text.encode_utf16().collect();
    let treffer = if muster.is_empty() { Vec::new() } else { alle_treffer(&re, text, &einheiten, flags.contains('u')) };
    let stueck = |von: usize, bis: usize| String::from_utf16_lossy(&einheiten[von.min(einheiten.len())..bis.clamp(von.min(einheiten.len()), einheiten.len())]);
    let mut stuecke = Vec::new();
    let mut pos = 0;
    for m in treffer.iter().filter(|m| !m.ganz.is_empty()) {
        stuecke.push((stueck(pos, m.index), false));
        stuecke.push((m.ganz.clone(), true));
        pos = m.index + m.ganz.encode_utf16().count();
    }
    stuecke.push((stueck(pos, einheiten.len()), false));
    let gruppen = treffer.iter().find(|m| !m.gruppen.is_empty()).map(|m| {
        m.gruppen.iter().map(|g| g.as_ref().map(|g| serde_json::to_string(g).unwrap_or_default()).unwrap_or_default()).collect::<Vec<_>>().join(", ")
    });
    let status = match gruppen {
        Some(g) => format!("{} Treffer · Gruppen im ersten: {g}", treffer.len()),
        None => format!("{} Treffer", treffer.len()),
    };
    Pruefung { stuecke, status, fehler: false }
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(json!({ "muster": ctx.gruppen.get(1).cloned().flatten().unwrap_or_default() }))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-regex">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let mut muster = use_signal(|| daten["muster"].as_str().unwrap_or_default().to_string());
    let mut flags = use_signal(|| "g".to_string());
    let mut text = use_signal(|| BEISPIEL.to_string());

    let p = pruefen(&muster(), &flags(), &text());
    let status_klasse = if p.fehler { "re-status w-text fehler" } else { "re-status w-text" };

    rsx! {
        ModulCss { name: "regex".to_string(), css: CSS }
        Kopf { titel: "RegEx-Tester".to_string() }
        div { class: "w-zeile",
            span { class: "re-strich", "/" }
            input {
                class: "w-eingabe re-muster",
                spellcheck: "false",
                placeholder: "[A-Z]\\w+",
                value: "{muster}",
                oninput: move |e| muster.set(e.value()),
            }
            span { class: "re-strich", "/" }
            input {
                class: "w-eingabe re-flags",
                size: "4",
                spellcheck: "false",
                aria_label: "Flags",
                value: "{flags}",
                oninput: move |e| flags.set(e.value()),
            }
        }
        textarea {
            class: "w-eingabe w-flaeche re-text",
            rows: "4",
            spellcheck: "false",
            value: "{text}",
            oninput: move |e| text.set(e.value()),
        }
        div { class: "re-ausgabe",
            for (i, (stueck, markiert)) in p.stuecke.into_iter().enumerate() {
                if markiert {
                    mark { key: "{i}", "{stueck}" }
                } else {
                    span { key: "{i}", "{stueck}" }
                }
            }
        }
        div { class: "{status_klasse}", "{p.status}" }
    }
}
