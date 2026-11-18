//! Modul "json": "JSON Formatter", oder einfach JSON in das Suchfeld
//! kopieren - prueft, formatiert und verkleinert JSON.
//! Portierung von public/module/json.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, KopierKnopf, ModulCss};
use dioxus::prelude::*;
use serde_json::{Value, json};

const CSS: &str = ".js-aus { max-height: 360px; overflow: auto; background: var(--flaeche2); border-radius: var(--r); padding: 12px; margin: 12px 0 0; font-size: 13px; }
.js-aus:empty { display: none; }";

/// Ist der Schluessel ein Array-Index? (JS setzt solche Schluessel in Objekten nach vorn.)
fn index_schluessel(k: &str) -> Option<u32> {
    if k == "0" {
        return Some(0);
    }
    if k.starts_with('0') || k.is_empty() || !k.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    k.parse::<u32>().ok().filter(|&n| n != u32::MAX)
}

/// Eine Zahl so, wie JavaScript sie schreibt (1.0 -> "1", 1e21 -> "1e+21").
fn js_zahl(n: &serde_json::Number) -> String {
    let f = n.as_f64().unwrap_or(f64::NAN);
    if !f.is_finite() {
        return "null".into();
    }
    js_sys::Number::from(f).to_string_with_radix(10).map(String::from).unwrap_or_else(|_| n.to_string())
}

/// Wie JSON.stringify(wert, null, einzug): einzug 0 = kompakt.
fn schreiben(wert: &Value, einzug: usize, tiefe: usize, aus: &mut String) {
    let umbruch = |aus: &mut String, t: usize| {
        if einzug > 0 {
            aus.push('\n');
            aus.push_str(&" ".repeat(einzug * t));
        }
    };
    match wert {
        Value::Null => aus.push_str("null"),
        Value::Bool(b) => aus.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => aus.push_str(&js_zahl(n)),
        Value::String(s) => aus.push_str(&serde_json::to_string(s).unwrap_or_default()),
        Value::Array(liste) => {
            if liste.is_empty() {
                aus.push_str("[]");
                return;
            }
            aus.push('[');
            for (i, w) in liste.iter().enumerate() {
                if i > 0 {
                    aus.push(',');
                }
                umbruch(aus, tiefe + 1);
                schreiben(w, einzug, tiefe + 1, aus);
            }
            umbruch(aus, tiefe);
            aus.push(']');
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                aus.push_str("{}");
                return;
            }
            // Reihenfolge wie in JS: erst Array-Index-Schluessel aufsteigend, dann der Rest.
            let mut indizes: Vec<(u32, &String, &Value)> = obj.iter().filter_map(|(k, v)| index_schluessel(k).map(|n| (n, k, v))).collect();
            indizes.sort_by_key(|(n, _, _)| *n);
            let rest = obj.iter().filter(|(k, _)| index_schluessel(k).is_none());
            let paare = indizes.into_iter().map(|(_, k, v)| (k, v)).chain(rest);
            aus.push('{');
            for (i, (k, v)) in paare.enumerate() {
                if i > 0 {
                    aus.push(',');
                }
                umbruch(aus, tiefe + 1);
                aus.push_str(&serde_json::to_string(k).unwrap_or_default());
                aus.push(':');
                if einzug > 0 {
                    aus.push(' ');
                }
                schreiben(v, einzug, tiefe + 1, aus);
            }
            umbruch(aus, tiefe);
            aus.push('}');
        }
    }
}

/// Prueft und formatiert: None = leere Eingabe, sonst (Ausgabe, Status, Klasse des Status).
fn auswerten(roh: &str, einzug: usize) -> Option<(String, String, &'static str)> {
    if roh.trim().is_empty() {
        return None;
    }
    Some(match serde_json::from_str::<Value>(roh) {
        Ok(wert) => {
            let mut aus = String::new();
            schreiben(&wert, einzug, 0, &mut aus);
            (aus, "✓ gültiges JSON".into(), "js-status w-text gut")
        }
        Err(fehler) => (String::new(), format!("✗ {fehler}"), "js-status w-text fehler"),
    })
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(json!({ "text": ctx.gruppen.get(1).cloned().flatten().unwrap_or_default() }))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-json">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let start = auswerten(daten["text"].as_str().unwrap_or_default(), 2);
    let mut eingabe = use_signal(|| daten["text"].as_str().unwrap_or_default().to_string());
    let mut ausgabe = use_signal(|| start.as_ref().map(|(a, _, _)| a.clone()).unwrap_or_default());
    let mut status = use_signal(|| start.as_ref().map(|(_, s, _)| s.clone()).unwrap_or_default());
    let mut status_klasse = use_signal(|| start.as_ref().map_or("js-status w-text", |(_, _, k)| k).to_string());

    let mut machen = move |einzug: usize| match auswerten(&eingabe.peek(), einzug) {
        None => {
            ausgabe.set(String::new());
            status.set(String::new());
        }
        Some((a, s, k)) => {
            ausgabe.set(a);
            status.set(s);
            status_klasse.set(k.to_string());
        }
    };

    rsx! {
        ModulCss { name: "json".to_string(), css: CSS }
        Kopf { titel: "JSON-Formatter".to_string() }
        textarea {
            class: "w-eingabe w-flaeche js-ein",
            rows: "6",
            spellcheck: "false",
            placeholder: "{{\"name\": \"nrzSearch\"}}",
            value: "{eingabe}",
            oninput: move |e| {
                eingabe.set(e.value());
                machen(2);
            },
        }
        div { class: "w-zeile",
            button { r#type: "button", class: "w-knopf haupt", onclick: move |_| machen(2), "Formatieren" }
            button { r#type: "button", class: "w-knopf", onclick: move |_| machen(0), "Verkleinern" }
            KopierKnopf { text: ausgabe(), class: "w-knopf".to_string() }
            span { class: "{status_klasse}", "{status}" }
        }
        if !ausgabe().is_empty() {
            pre { class: "js-aus", "{ausgabe}" }
        }
    }
}
