//! Modul "bpm": "BPM", "Tap Tempo", "Metronom" - tippen misst das Tempo,
//! das Metronom schlaegt ein Tempo an.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, ton, use_tasten};
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};

const CSS: &str = r#"
.bp-tap { width: 100%; height: 120px; font: inherit; font-size: 22px; border: 2px dashed var(--kante); border-radius: var(--r); background: var(--flaeche2); color: var(--schrift); cursor: pointer; user-select: none; }
.bp-tap small { font-size: 12px; color: var(--gedimmt); }
.bp-wert { text-align: center; margin: 12px 0; color: var(--gedimmt); }
"#;

/// Element.animate(keyframes, optionen) - ueber Reflect, weil web-sys es nur instabil anbietet.
fn animieren(el: &web_sys::Element, bilder: &str, optionen: &str) {
    let (Ok(bilder), Ok(optionen)) = (js_sys::JSON::parse(bilder), js_sys::JSON::parse(optionen)) else { return };
    if let Ok(f) = js_sys::Reflect::get(el, &JsValue::from_str("animate")).and_then(|f| f.dyn_into::<js_sys::Function>()) {
        let _ = f.call2(el, &bilder, &optionen);
    }
}

/// Millisekunden seit dem Laden der Seite (performance.now()).
fn jetzt() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or_else(js_sys::Date::now, |p| p.now())
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

/// Zeichnet das Modul (steht in `<section class="modul modul-bpm">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = (&ctx, &daten);
    let mut schlaege = use_signal(Vec::<f64>::new);
    let mut wert = use_signal(|| None::<i64>);
    let mut tempo = use_signal(|| "120".to_string());
    // Laeuft das Metronom? Der Zaehler beendet alte Schleifen beim Stoppen.
    let mut metronom = use_signal(|| 0u32);
    let mut laeuft = use_signal(|| false);
    let mut knopf = use_signal(|| None::<web_sys::Element>);

    let mut tippen = move || {
        let t = jetzt();
        let mut s = schlaege.peek().clone();
        if s.last().is_some_and(|l| t - l > 2500.0) {
            s.clear();
        }
        s.push(t);
        if s.len() > 12 {
            s.remove(0);
        }
        schlaege.set(s.clone());
        if let Some(el) = knopf.peek().as_ref() {
            animieren(el, r#"[{"transform":"scale(.94)"},{"transform":"none"}]"#, r#"{"duration":120}"#);
        }
        if s.len() < 2 {
            return;
        }
        let bpm = 60000.0 / ((s[s.len() - 1] - s[0]) / (s.len() - 1) as f64);
        let b = bpm.round() as i64;
        wert.set(Some(b));
        tempo.set(b.to_string());
    };

    use_tasten(move |e| {
        if e.code() == "Space" {
            e.prevent_default();
            tippen();
        }
    });

    let anzeige = wert().map_or_else(|| "–".to_string(), |b| b.to_string());

    rsx! {
        ModulCss { name: "bpm".to_string(), css: CSS }
        Kopf { titel: "BPM-Messung & Metronom".to_string() }
        button {
            r#type: "button",
            class: "bp-tap",
            onmounted: move |e| knopf.set(Some(e.data().as_web_event())),
            onclick: move |_| tippen(),
            "Tippen"
            br {}
            small { "oder Leertaste" }
        }
        div { class: "bp-wert", span { class: "w-riesig", "{anzeige}" } " BPM" }
        div { class: "w-zeile",
            button {
                r#type: "button",
                class: "w-knopf",
                onclick: move |_| {
                    schlaege.write().clear();
                    wert.set(None);
                },
                "Zurücksetzen"
            }
            label {
                "Metronom "
                input {
                    r#type: "number",
                    class: "w-eingabe bp-tempo",
                    value: "{tempo}",
                    min: "20",
                    max: "300",
                    style: "width:80px",
                    oninput: move |e| tempo.set(e.value()),
                }
                " BPM"
            }
            button {
                r#type: "button",
                class: "w-knopf",
                onclick: move |_| {
                    if *laeuft.peek() {
                        laeuft.set(false);
                        metronom += 1;
                        return;
                    }
                    laeuft.set(true);
                    metronom += 1;
                    let nummer = *metronom.peek();
                    let bpm = tempo.peek().trim().parse::<f64>().ok().filter(|b| *b != 0.0 && b.is_finite()).unwrap_or(120.0);
                    let abstand = 60000.0 / bpm.clamp(20.0, 300.0);
                    // Die Schleife endet mit der Komponente (spawn gehoert zu ihr) oder beim Stoppen;
                    // gewartet wird bis zum naechsten geplanten Schlag (laeuft nicht weg wie setInterval).
                    spawn(async move {
                        let mut takt = 0u32;
                        let mut naechster = jetzt();
                        loop {
                            if *metronom.peek() != nummer {
                                break;
                            }
                            ton(if takt % 4 != 0 { 880.0 } else { 1320.0 }, 0.05, 1);
                            takt += 1;
                            naechster += abstand;
                            let warten = (naechster - jetzt()).max(0.0) as u32;
                            gloo_timers::future::TimeoutFuture::new(warten).await;
                        }
                    });
                },
                if laeuft() { "Stopp" } else { "Start" }
            }
        }
    }
}
