//! Modul "stoppuhr": eine Stoppuhr mit Rundenzeiten.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, jetzt_ms, use_intervall};
use dioxus::prelude::*;
use serde_json::Value;

const CSS: &str = r#"
.s-anzeige { font-size: 56px; text-align: center; padding: 8px 0 16px; font-variant-numeric: tabular-nums; }
.s-runden { margin: 12px 0 0; padding-left: 28px; max-height: 220px; overflow: auto; color: var(--gedimmt); font-variant-numeric: tabular-nums; }
.s-runden li { padding: 4px 0; border-bottom: 1px solid var(--kante); }
.s-runden li span { display: inline-block; min-width: 120px; color: var(--schrift); }
.s-runden li span + span { color: var(--gedimmt); }
"#;

/// Millisekunden als "mm:ss,hh" bzw. "h:mm:ss,hh".
fn zeit(ms: f64) -> String {
    let ms = ms.max(0.0).floor() as u64;
    let s = ms / 1000;
    let h = s / 3600;
    let text = format!("{:02}:{:02},{:02}", (s / 60) % 60, s % 60, (ms / 10) % 100);
    if h > 0 { format!("{h}:{text}") } else { text }
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

/// Zeichnet das Modul (steht in `<section class="modul modul-stoppuhr">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = (&ctx, &daten);
    // Startzeit, solange sie laeuft (0 = steht)
    let mut start = use_signal(|| 0.0f64);
    // die gestoppte Zeit davor
    let mut vorher = use_signal(|| 0.0f64);
    let mut runden = use_signal(Vec::<f64>::new);
    let mut takt = use_signal(|| 0u64);

    let jetzt = move || *vorher.peek() + if *start.peek() != 0.0 { jetzt_ms() - *start.peek() } else { 0.0 };

    use_intervall(31, move || {
        if *start.peek() != 0.0 {
            takt += 1;
        }
    });

    let _ = takt();
    let (s, v) = (start(), vorher());
    let gerade = v + if s != 0.0 { jetzt_ms() - s } else { 0.0 };
    let beschriftung = if s != 0.0 { "Stopp" } else if gerade != 0.0 { "Weiter" } else { "Start" };
    let liste = runden();

    rsx! {
        ModulCss { name: "stoppuhr".to_string(), css: CSS }
        Kopf { titel: "Stoppuhr".to_string() }
        div { class: "s-anzeige", "{zeit(gerade)}" }
        div { class: "w-zeile",
            button {
                r#type: "button",
                class: "w-knopf haupt",
                onclick: move |_| {
                    let s = *start.peek();
                    if s != 0.0 {
                        vorher += jetzt_ms() - s;
                        start.set(0.0);
                    } else {
                        start.set(jetzt_ms());
                    }
                },
                "{beschriftung}"
            }
            button {
                r#type: "button",
                class: "w-knopf",
                onclick: move |_| {
                    let gesamt = jetzt();
                    if gesamt != 0.0 {
                        runden.write().insert(0, gesamt);
                    }
                },
                "Runde"
            }
            button {
                r#type: "button",
                class: "w-knopf",
                onclick: move |_| {
                    start.set(0.0);
                    vorher.set(0.0);
                    runden.write().clear();
                },
                "Zurücksetzen"
            }
        }
        ol { class: "s-runden", reversed: true,
            for (i, r) in liste.iter().enumerate() {
                li { key: "{liste.len() - i}",
                    span { "{zeit(r - liste.get(i + 1).copied().unwrap_or(0.0))}" }
                    span { "{zeit(*r)}" }
                }
            }
        }
    }
}
