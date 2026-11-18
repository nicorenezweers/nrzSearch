//! Modul "finanzrechner": "15 % von 240", "Prozentrechner", "Zinsrechner",
//! "Zinseszins", "Kreditrechner" - drei Rechner in einem, mit Reitern.
//! Portierung von public/module/finanzrechner.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, zahl};
use dioxus::prelude::*;
use serde_json::{Value, json};

const CSS: &str = "
.fr-reiter { display: flex; gap: 4px; margin-bottom: 14px; border-bottom: 1px solid var(--kante); }
.fr-reiter button { font: inherit; background: none; border: 0; border-bottom: 3px solid transparent; padding: 8px 14px; color: var(--gedimmt); cursor: pointer; }
.fr-reiter button.aktiv { color: var(--hoch); border-bottom-color: var(--hoch); }
.fr-felder { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 12px; margin-bottom: 14px; }
.fr-felder label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--gedimmt); }
";

/// Die Rechner: id, Name, Felder (Schluessel, Beschriftung, Startwert).
const RECHNER: &[(&str, &str, &[(&str, &str, &str)])] = &[
    ("prozent", "Prozent", &[("p", "Prozent", "15"), ("g", "von", "240")]),
    ("zins", "Zinseszins", &[("k", "Startkapital (€)", "10000"), ("z", "Zins p. a. (%)", "5"), ("j", "Jahre", "10"), ("r", "Sparrate / Monat (€)", "100")]),
    ("kredit", "Kredit", &[("s", "Kreditsumme (€)", "20000"), ("z", "Sollzins p. a. (%)", "6,5"), ("j", "Laufzeit (Jahre)", "5")]),
];

fn felder(art: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    RECHNER.iter().find(|r| r.0 == art).map_or(RECHNER[0].2, |r| r.2)
}

fn startwerte(art: &str) -> Vec<String> {
    felder(art).iter().map(|f| f.2.to_string()).collect()
}

/// Wie parseFloat in JS: die laengste Zahl am Anfang, sonst NaN.
pub(super) fn parse_float(s: &str) -> f64 {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if s[i..].starts_with("Infinity") {
        return if b.first() == Some(&b'-') { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let ziffern_ab = |mut j: usize| {
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        j
    };
    let vor = ziffern_ab(i);
    let mut ende = vor;
    let mut ziffern = vor > i;
    if ende < b.len() && b[ende] == b'.' {
        let nach = ziffern_ab(ende + 1);
        if nach > ende + 1 || ziffern {
            ziffern |= nach > ende + 1;
            ende = nach;
        }
    }
    if !ziffern {
        return f64::NAN;
    }
    if ende < b.len() && (b[ende] == b'e' || b[ende] == b'E') {
        let mut j = ende + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let k = ziffern_ab(j);
        if k > j {
            ende = k;
        }
    }
    s[..ende].parse().unwrap_or(f64::NAN)
}

/// Ein Feld als Zahl: Leerraum weg, erstes Komma als Punkt, ungueltig = 0.
fn n(wert: &str) -> f64 {
    let w: String = wert.chars().filter(|c| !c.is_whitespace()).collect();
    let z = parse_float(&w.replacen(',', ".", 1));
    if z.is_nan() { 0.0 } else { z }
}

/// Math.round wie in JS (halbe nach oben).
fn runden(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn euro(v: f64) -> String {
    format!("{} €", zahl(v, 2))
}

/// Das Ergebnis: (grosse Zeile, Erklaerung).
fn rechne(art: &str, werte: &[String]) -> (String, String) {
    let w = |i: usize| n(werte.get(i).map_or("", String::as_str));
    match art {
        "zins" => {
            let k = w(0);
            let z = w(1) / 100.0 / 12.0;
            let monate = runden(w(2) * 12.0);
            let r = w(3);
            let mut summe = k;
            let mut i = 0.0;
            while i < monate {
                summe = summe * (1.0 + z) + r;
                i += 1.0;
            }
            let eingezahlt = k + r * monate;
            (euro(summe), format!("eingezahlt {} · Zinsen {} (monatliche Verzinsung)", euro(eingezahlt), euro(summe - eingezahlt)))
        }
        "kredit" => {
            let s = w(0);
            let z = w(1) / 100.0 / 12.0;
            let m = runden(w(2) * 12.0).max(1.0);
            let rate = if z != 0.0 { (s * z) / (1.0 - (1.0 + z).powf(-m)) } else { s / m };
            (format!("{} / Monat", euro(rate)), format!("{m} Raten · gesamt {} · Zinsen {}", euro(rate * m), euro(rate * m - s)))
        }
        _ => {
            let p = w(0);
            let g = w(1);
            let x = (g * p) / 100.0;
            (zahl(x, 2), format!("{} % von {} · plus: {} · minus: {}", zahl(p, 2), zahl(g, 2), zahl(g + x, 2), zahl(g - x, 2)))
        }
    }
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    if let Some(g) = ctx.gruppe(2) {
        return Some(json!({ "art": "prozent", "p": ctx.gruppe(1).unwrap_or_default(), "g": g }));
    }
    let wort = ctx.gruppe(1).unwrap_or_default().to_lowercase();
    let art = if wort.contains("kredit") || wort.contains("tilgung") {
        "kredit"
    } else if wort.contains("zins") {
        "zins"
    } else {
        "prozent"
    };
    Some(json!({ "art": art }))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-finanzrechner">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let mut art = use_signal(|| daten["art"].as_str().filter(|a| RECHNER.iter().any(|r| r.0 == *a)).unwrap_or("prozent").to_string());
    let mut werte = use_signal(|| {
        let a = art.peek().clone();
        let mut w = startwerte(&a);
        // Nur beim ersten Zeigen: die Zahlen aus der Frage ("15 % von 240").
        for (i, (k, _, _)) in felder(&a).iter().enumerate() {
            if let Some(v) = daten[*k].as_str().filter(|v| !v.is_empty()) {
                w[i] = v.to_string();
            }
        }
        w
    });

    let a = art();
    let (gross, text) = rechne(&a, &werte());

    rsx! {
        ModulCss { name: "finanzrechner".to_string(), css: CSS }
        Kopf { titel: "Finanzrechner".to_string() }
        div { class: "fr-reiter",
            for (id, name, _) in RECHNER.iter() {
                button {
                    key: "{id}",
                    r#type: "button",
                    class: if *id == a { "aktiv" } else { "" },
                    onclick: move |_| {
                        art.set(id.to_string());
                        werte.set(startwerte(id));
                    },
                    "{name}"
                }
            }
        }
        div { class: "fr-felder",
            for (i, (k, beschriftung, _)) in felder(&a).iter().enumerate() {
                label { key: "{a}-{k}",
                    "{beschriftung}"
                    input {
                        class: "w-eingabe",
                        "data-f": "{k}",
                        value: "{werte.read().get(i).cloned().unwrap_or_default()}",
                        oninput: move |e| {
                            if let Some(w) = werte.write().get_mut(i) {
                                *w = e.value();
                            }
                        },
                    }
                }
            }
        }
        div { class: "fr-ergebnis",
            div { class: "w-gross", "{gross}" }
            div { class: "w-text", "{text}" }
        }
    }
}
