//! Modul "passwort": "Passwort-Generator", "sicheres Passwort 32" - ein
//! kryptografisch sicheres Passwort mit einstellbarer Laenge und Zeichen.
//! Einstellung: "laenge" (Vorgabe).

use super::ModulCtx;
use crate::werkzeug::{Kopf, KopierKnopf, ModulCss, zufall};
use dioxus::prelude::*;
use serde_json::{Value, json};

const CSS: &str = r#"
.pw-feld { display: flex; gap: 8px; align-items: center; margin-bottom: 12px; }
.pw-wert { flex: 1; font-family: ui-monospace, Menlo, Consolas, monospace; font-size: 20px; padding: 10px 14px; background: var(--flaeche2); border-radius: var(--r); word-break: break-all; }
.pw-optionen label { display: inline-flex; align-items: center; gap: 6px; color: var(--gedimmt); }
.pw-staerke { height: 4px; background: var(--flaeche3); border-radius: 2px; margin-top: 12px; overflow: hidden; }
.pw-staerke span { display: block; height: 100%; transition: width .2s; }
"#;

/// Die Zeichenarten: (Name, Beschriftung, Zeichen).
const ZEICHEN: [(&str, &str, &str); 4] = [
    ("gross", "A–Z", "ABCDEFGHJKLMNPQRSTUVWXYZ"),
    ("klein", "a–z", "abcdefghijkmnopqrstuvwxyz"),
    ("ziffern", "0–9", "23456789"),
    ("sonder", "!@#", "!@#$%&*?-_=+.:;"),
];

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let n = ctx.gruppe(1).and_then(|g| g.parse::<f64>().ok()).filter(|n| *n != 0.0);
    Some(json!(n.or(ctx.cfg_f64("laenge").filter(|n| *n != 0.0)).unwrap_or(20.0)))
}

/// Ein neues Passwort der Laenge `n` aus den gewaehlten Arten.
fn erzeugen(n: usize, arten: [bool; 4]) -> (String, f64) {
    let mut gewaehlt: Vec<Vec<char>> = ZEICHEN.iter().zip(arten).filter(|(_, an)| *an).map(|((_, _, z), _)| z.chars().collect()).collect();
    if gewaehlt.is_empty() {
        gewaehlt.push(ZEICHEN[1].2.chars().collect());
    }
    // Aus jeder gewaehlten Art mindestens ein Zeichen, dann gemischt.
    let alle: Vec<char> = gewaehlt.concat();
    let mut zeichen: Vec<char> = gewaehlt.iter().map(|a| a[zufall(a.len() as u32) as usize]).collect();
    while zeichen.len() < n {
        zeichen.push(alle[zufall(alle.len() as u32) as usize]);
    }
    for i in (1..zeichen.len()).rev() {
        let j = zufall(i as u32 + 1) as usize;
        zeichen.swap(i, j);
    }
    zeichen.truncate(n);
    let bits = n as f64 * (alle.len() as f64).log2();
    (zeichen.into_iter().collect(), bits)
}

/// Zeichnet das Modul (steht in `<section class="modul modul-passwort">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let start = daten.as_f64().unwrap_or(20.0).clamp(6.0, 64.0).round() as usize;
    let mut laenge = use_signal(|| start);
    let mut arten = use_signal(|| [true; 4]);
    let mut ergebnis = use_signal(|| erzeugen(start, [true; 4]));

    let mut neu = move || ergebnis.set(erzeugen(*laenge.peek(), *arten.peek()));

    let (wert, bits) = ergebnis();
    let breite = (bits / 1.28).min(100.0);
    let farbe = if bits < 50.0 { "var(--rot)" } else if bits < 80.0 { "#fbbc04" } else { "var(--gruen)" };
    let bit_text = format!("{} Bit", bits.round());
    let n = laenge();
    let an = arten();

    rsx! {
        ModulCss { name: "passwort".to_string(), css: CSS }
        Kopf { titel: "Passwort-Generator".to_string() }
        div { class: "pw-feld",
            output { class: "pw-wert", "{wert}" }
            KopierKnopf { text: wert.clone(), class: "w-knopf".to_string() }
            button { r#type: "button", class: "w-knopf haupt", onclick: move |_| neu(), "Neu" }
        }
        div { class: "w-zeile pw-optionen",
            label {
                "Länge "
                input {
                    r#type: "range",
                    min: "6",
                    max: "64",
                    value: "{n}",
                    class: "pw-laenge",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<usize>() {
                            laenge.set(v);
                        }
                        neu();
                    },
                }
                " "
                b { class: "pw-zahl", "{n}" }
            }
            for (i, (art, name, _)) in ZEICHEN.iter().enumerate() {
                label { key: "{art}",
                    input {
                        r#type: "checkbox",
                        checked: an[i],
                        "data-art": "{art}",
                        oninput: move |e| {
                            arten.write()[i] = e.checked();
                            neu();
                        },
                    }
                    " {name}"
                }
            }
        }
        div { class: "pw-staerke",
            span { style: "width: {breite}%; background: {farbe}", title: "{bit_text}" }
        }
    }
}
