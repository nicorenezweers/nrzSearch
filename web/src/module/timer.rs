//! Modul "timer": "Timer", "Timer 5 Minuten", "10 min timer", "1h 30m timer"
//! - ein Countdown mit Start, Pause, Zuruecksetzen und Alarmton.
//! Einstellungen: "standard" (Sekunden ohne Angabe), "ton" (true/false).

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, jetzt_ms, ton, use_intervall};
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

const CSS: &str = r#"
.t-anzeige { font-size: 56px; text-align: center; padding: 8px 0; outline: none; border-radius: var(--r); font-variant-numeric: tabular-nums; }
.t-anzeige:focus { background: var(--flaeche2); }
.t-balken { height: 4px; background: var(--flaeche3); border-radius: 2px; margin: 4px 0 16px; overflow: hidden; }
.t-balken span { display: block; height: 100%; background: var(--hoch); transition: width .2s linear; }
.modul-timer .abgelaufen .t-anzeige { color: var(--rot); animation: blinken 1s steps(2) infinite; }
@keyframes blinken { 50% { opacity: .3; } }
"#;

/// Sekunden je Einheit.
fn einheit(e: &str) -> Option<f64> {
    Some(match e {
        "h" | "std" | "stunde" | "stunden" => 3600.0,
        "m" | "min" | "minute" | "minuten" => 60.0,
        "s" | "sek" | "sekunde" | "sekunden" => 1.0,
        _ => return None,
    })
}

/// "5 minuten", "1h 30m", "90 s", "5:00", "1:30:00" -> Sekunden
pub fn dauer_lesen(text: &str) -> Option<u64> {
    let t = text.trim().to_lowercase();
    // "5:00" oder "1:30:00"
    let teile: Vec<&str> = t.split(':').collect();
    if (2..=3).contains(&teile.len())
        && !teile[0].is_empty()
        && teile[0].bytes().all(|b| b.is_ascii_digit())
        && teile[1..].iter().all(|p| (1..=2).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Some(teile.iter().fold(0u64, |s, p| s * 60 + p.parse::<u64>().unwrap_or(0)));
    }
    // Wie /(\d+(?:[.,]\d+)?)\s*([a-zä]+)?/g
    let z: Vec<char> = t.chars().collect();
    let mut summe = 0.0f64;
    let mut i = 0;
    while i < z.len() {
        if !z[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut zahl = String::new();
        while i < z.len() && z[i].is_ascii_digit() {
            zahl.push(z[i]);
            i += 1;
        }
        if i + 1 < z.len() && (z[i] == '.' || z[i] == ',') && z[i + 1].is_ascii_digit() {
            zahl.push('.');
            i += 1;
            while i < z.len() && z[i].is_ascii_digit() {
                zahl.push(z[i]);
                i += 1;
            }
        }
        while i < z.len() && z[i].is_whitespace() {
            i += 1;
        }
        let mut wort = String::new();
        while i < z.len() && (z[i].is_ascii_lowercase() || z[i] == 'ä') {
            wort.push(z[i]);
            i += 1;
        }
        let faktor = if wort.is_empty() { 60.0 } else { einheit(&wort).unwrap_or(f64::NAN) };
        summe += zahl.parse::<f64>().unwrap_or(f64::NAN) * faktor;
    }
    (summe.is_finite() && summe > 0.0).then(|| summe.round() as u64)
}

/// Sekunden als "mm:ss" bzw. "h:mm:ss".
fn zeit(s: u64) -> String {
    let h = s / 3600;
    let text = format!("{:02}:{:02}", (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{text}") } else { text }
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    match ctx.gruppe(1) {
        None => Some(json!(ctx.cfg_f64("standard").filter(|s| *s != 0.0).unwrap_or(300.0))),
        Some(angabe) => dauer_lesen(angabe).map(|s| json!(s)),
    }
}

/// Den Text der Anzeige direkt setzen (sie ist bearbeitbar, darum nicht ueber das rsx).
fn anzeige_setzen(el: &Option<web_sys::Element>, rest: u64) {
    if let Some(el) = el {
        el.set_text_content(Some(&zeit(rest)));
    }
}

/// Zeichnet das Modul (steht in `<section class="modul modul-timer">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let start = daten.as_f64().unwrap_or(300.0).max(0.0).round() as u64;
    let mut gesamt = use_signal(|| start);
    let mut rest = use_signal(|| start);
    // Zeitpunkt des Endes, solange er laeuft (0 = steht)
    let mut ende = use_signal(|| 0.0f64);
    let mut anzeige = use_signal(|| None::<web_sys::Element>);
    let mit_ton = ctx.cfg_bool("ton") != Some(false);

    let mut setzen = move |s: u64| {
        gesamt.set(s);
        rest.set(s);
        ende.set(0.0);
        anzeige_setzen(&anzeige.peek(), s);
    };

    use_intervall(200, move || {
        let e = *ende.peek();
        if e == 0.0 {
            return;
        }
        let r = ((e - jetzt_ms()) / 1000.0).ceil().max(0.0) as u64;
        if r == 0 {
            ende.set(0.0);
            if mit_ton {
                ton(880.0, 0.25, 4);
            }
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                doc.set_title("⏰ Timer abgelaufen");
            }
        }
        if r != *rest.peek() {
            rest.set(r);
        }
    });

    // Die Anzeige folgt dem Rest.
    use_effect(move || anzeige_setzen(&anzeige.read(), rest()));

    let (g, r, laeuft) = (gesamt(), rest(), ende() != 0.0);
    let breite = if g > 0 { r as f64 / g as f64 * 100.0 } else { 0.0 };
    let beschriftung = if laeuft { "Pause" } else if r == g { "Start" } else { "Weiter" };
    let klasse = if r == 0 && g > 0 { "abgelaufen" } else { "" };

    rsx! {
        ModulCss { name: "timer".to_string(), css: CSS }
        div { class: "{klasse}",
            Kopf { titel: "Timer".to_string() }
            div {
                class: "t-anzeige",
                contenteditable: "true",
                spellcheck: "false",
                title: "Zum Ändern klicken",
                onmounted: move |e| {
                    let el = e.data().as_web_event();
                    el.set_text_content(Some(&zeit(*rest.peek())));
                    anzeige.set(Some(el));
                },
                // Die Anzeige laesst sich direkt bearbeiten: "10:00", "90s", "2 min".
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        e.prevent_default();
                        if let Some(el) = anzeige.peek().as_ref().and_then(|el| el.dyn_ref::<web_sys::HtmlElement>()) {
                            let _ = el.blur();
                        }
                    }
                },
                onblur: move |_| {
                    let text = anzeige.peek().as_ref().and_then(|el| el.text_content()).unwrap_or_default();
                    let neu = dauer_lesen(&text).unwrap_or(*gesamt.peek());
                    setzen(neu);
                },
            }
            div { class: "t-balken", span { style: "width: {breite}%" } }
            div { class: "w-zeile",
                button {
                    r#type: "button",
                    class: "w-knopf haupt",
                    onclick: move |_| {
                        if *ende.peek() != 0.0 {
                            ende.set(0.0);
                        } else if *rest.peek() > 0 {
                            ende.set(jetzt_ms() + *rest.peek() as f64 * 1000.0);
                            // Den Ton im Klick freischalten (Autoplay-Regeln).
                            ton(0.0, 0.001, 1);
                        }
                    },
                    "{beschriftung}"
                }
                button { r#type: "button", class: "w-knopf", onclick: move |_| setzen(*gesamt.peek()), "Zurücksetzen" }
                for m in [1u64, 5, 10, 15] {
                    button { key: "{m}", r#type: "button", class: "w-knopf klein", onclick: move |_| setzen(m * 60), "{m} min" }
                }
            }
        }
    }
}
