//! Modul "zufall": "Kopf oder Zahl", "Würfel", "2d6", "Zufallszahl 1 bis 100"
//! - Muenzwurf, Wuerfel und Zufallszahlen aus der Krypto-Schnittstelle.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, zufall};
use dioxus::prelude::*;
use dioxus::web::WebEventExt;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};

const CSS: &str = r#"
.zf-ergebnis { min-height: 110px; display: flex; flex-direction: column; justify-content: center; margin: 4px 0 12px; }
.zf-muenze { width: 96px; height: 96px; border-radius: 50%; display: grid; place-items: center; font-size: 20px; font-weight: 500; background: radial-gradient(circle at 35% 35%, #fde293, #e3a008); color: #5f4100; box-shadow: inset 0 0 0 4px #c48a00; }
.zf-wuerfe { display: flex; flex-wrap: wrap; gap: 10px; font-size: 64px; line-height: 1; }
.zf-wuerfe span { min-width: 64px; text-align: center; }
"#;

const WUERFEL: [&str; 6] = ["⚀", "⚁", "⚂", "⚃", "⚄", "⚅"];

/// Element.animate(keyframes, optionen) - ueber Reflect, weil web-sys es nur instabil anbietet.
fn animieren(el: &web_sys::Element, bilder: &str, optionen: &str) {
    let (Ok(bilder), Ok(optionen)) = (js_sys::JSON::parse(bilder), js_sys::JSON::parse(optionen)) else { return };
    if let Ok(f) = js_sys::Reflect::get(el, &JsValue::from_str("animate")).and_then(|f| f.dyn_into::<js_sys::Function>()) {
        let _ = f.call2(el, &bilder, &optionen);
    }
}

/// Eine Zahl wie JS `Number(text)` (leer = 0, Unsinn = NaN).
fn zahl(text: &str) -> f64 {
    let t = text.trim();
    if t.is_empty() { 0.0 } else { t.parse().unwrap_or(f64::NAN) }
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let f = ctx.frage.to_lowercase();
    let roh = |i: usize| ctx.gruppen.get(i).cloned().flatten();
    if ["kopf", "münz", "muenz", "coin"].iter().any(|w| f.contains(w)) {
        return Some(json!({ "art": "muenze" }));
    }
    // /^\d*d\d+$/
    let wuerfel_form = f.split_once('d').is_some_and(|(a, b)| a.bytes().all(|c| c.is_ascii_digit()) && !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()));
    if wuerfel_form {
        let anzahl = zahl(&roh(1).unwrap_or_default());
        let anzahl = if anzahl.is_nan() || anzahl == 0.0 { 1.0 } else { anzahl.min(20.0) };
        let seiten = zahl(&roh(2).unwrap_or_default()).clamp(2.0, 1000.0);
        return Some(json!({ "art": "wuerfel", "anzahl": anzahl, "seiten": seiten }));
    }
    if f.contains("zufall") || f.contains("random") {
        let von = roh(1).map_or(1.0, |t| zahl(&t));
        let bis = roh(2).map_or(100.0, |t| zahl(&t));
        return Some(json!({ "art": "zahl", "von": von, "bis": bis }));
    }
    Some(json!({ "art": "wuerfel", "anzahl": 1, "seiten": 6 }))
}

/// Das gezogene Ergebnis.
#[derive(Clone, PartialEq)]
enum Wurf {
    Leer,
    Muenze(bool),
    Wuerfe(Vec<u32>),
    Zahl(i64),
}

/// Zeichnet das Modul (steht in `<section class="modul modul-zufall">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let art = daten.get("art").and_then(Value::as_str).unwrap_or("wuerfel").to_string();
    let anzahl = daten.get("anzahl").and_then(Value::as_f64).unwrap_or(1.0).max(1.0) as u32;
    let seiten = daten.get("seiten").and_then(Value::as_f64).unwrap_or(6.0).max(2.0) as u32;
    let von_start = daten.get("von").and_then(Value::as_f64).unwrap_or(1.0);
    let bis_start = daten.get("bis").and_then(Value::as_f64).unwrap_or(100.0);

    let mut von = use_signal(|| von_start.to_string());
    let mut bis = use_signal(|| bis_start.to_string());
    let mut wurf = use_signal(|| Wurf::Leer);
    let mut ziel = use_signal(|| None::<web_sys::Element>);

    let a2 = art.clone();
    let ziehen = use_callback(move |_: ()| {
        let neu = match a2.as_str() {
            "muenze" => Wurf::Muenze(zufall(2) == 0),
            "wuerfel" => Wurf::Wuerfe((0..anzahl).map(|_| zufall(seiten) + 1).collect()),
            _ => {
                let lesen = |t: &str| {
                    let n = zahl(t);
                    if n.is_finite() { n.round() as i64 } else { 0 }
                };
                let (v, b) = (lesen(&von.peek()), lesen(&bis.peek()));
                let (a, b) = if v <= b { (v, b) } else { (b, v) };
                let spanne = (b - a + 1).clamp(1, i64::from(u32::MAX)) as u32;
                Wurf::Zahl(a + i64::from(zufall(spanne)))
            }
        };
        wurf.set(neu);
        // Nach dem Zeichnen das neue Ergebnis kurz drehen lassen.
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(0).await;
            if let Some(kind) = ziel.peek().as_ref().and_then(|z| z.first_element_child()) {
                animieren(&kind, r#"[{"transform":"rotateY(90deg) scale(.8)"},{"transform":"none"}]"#, r#"{"duration":300,"easing":"ease-out"}"#);
            }
        });
    });

    use_effect(move || ziehen.call(()));

    let titel = match art.as_str() {
        "muenze" => "Münzwurf".to_string(),
        "wuerfel" => format!("Würfel ({anzahl}d{seiten})"),
        _ => "Zufallszahl".to_string(),
    };
    let knopf = match art.as_str() {
        "muenze" => "Werfen",
        "wuerfel" => "Würfeln",
        _ => "Erzeugen",
    };

    let ergebnis = match wurf() {
        Wurf::Leer => rsx! {},
        Wurf::Muenze(kopf) => {
            let (klasse, text) = if kopf { ("zf-muenze kopf", "Kopf") } else { ("zf-muenze zahl", "Zahl") };
            rsx! { div { class: "{klasse}", "{text}" } }
        }
        Wurf::Wuerfe(wuerfe) => {
            let summe: u32 = wuerfe.iter().sum();
            rsx! {
                div { class: "zf-wuerfe",
                    for w in wuerfe.iter() {
                        span { if seiten == 6 { "{WUERFEL[(*w as usize - 1) % 6]}" } else { "{w}" } }
                    }
                }
                if anzahl > 1 {
                    div { class: "w-text", "Summe: " b { "{summe}" } }
                }
            }
        }
        Wurf::Zahl(n) => rsx! { div { class: "w-riesig", "{n}" } },
    };

    rsx! {
        ModulCss { name: "zufall".to_string(), css: CSS }
        Kopf { titel }
        if art == "zahl" {
            div { class: "w-zeile",
                label {
                    "von "
                    input { class: "w-eingabe zf-von", value: "{von}", size: "6", oninput: move |e| von.set(e.value()) }
                }
                label {
                    "bis "
                    input { class: "w-eingabe zf-bis", value: "{bis}", size: "6", oninput: move |e| bis.set(e.value()) }
                }
            }
        }
        div { class: "zf-ergebnis", onmounted: move |e| ziel.set(Some(e.data().as_web_event())), {ergebnis} }
        button { r#type: "button", class: "w-knopf haupt", onclick: move |_| ziehen.call(()), "{knopf}" }
    }
}
