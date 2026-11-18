//! Modul "wetter" - Portierung von public/module/wetter.js.
//!
//! "Wetter", "Wetter Berlin", "Wetter in Tokio" - das Wetter wie bei Google: jetzt, der Verlauf
//! der naechsten 24 Stunden (Temperatur, Niederschlag, Wind) und sieben Tage. Daten: Open-Meteo
//! ueber den Server. Einstellung: "ort" (ohne Angabe).

use super::ModulCtx;
use crate::bib::wetter::{WetterIcon, code_aus, grad, tag_aus, wetter_text, wochentag};
use dioxus::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;

/// Der Aufbau der Ergebnisseite, in dem das Wetter-Modul steht - dann zeigt die Info-Karte
/// kein eigenes kleines Wetter.
pub static WETTER_LAUF: GlobalSignal<Option<u64>> = Signal::global(|| None);

const B: f64 = 620.0;
const H: f64 = 110.0;

/// Prueft die Frage genauer und liefert den Ort fuer [`Zeigen`] - oder None.
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let q = ctx.gruppe(1).or(ctx.cfg_str("ort")).unwrap_or("Berlin").trim().to_string();
    let ort = crate::api::get_oder_nichts("/api/geo", &[("q", &q)]).await?.get("ort").cloned()?;
    if ort.get("lat").is_none_or(Value::is_null) {
        return None;
    }
    *WETTER_LAUF.write() = Some(ctx.lauf);
    Some(ort)
}

/// Zeichnet das Modul (steht in `<section class="modul modul-wetter">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let ort = daten.clone();
    let wetter = use_resource(move || {
        let ort = ort.clone();
        async move {
            let lat = zahl_text(&ort["lat"]);
            let lon = zahl_text(&ort["lon"]);
            crate::api::get_oder_nichts("/api/wetter", &[("lat", &lat), ("lon", &lon)]).await?.get("wetter").cloned().filter(|w| !w.is_null())
        }
    });
    let inhalt = match &*wetter.read() {
        None => rsx! {
            div { class: "we-laden",
                div { class: "skelett-zeile titel" }
                div { class: "skelett-rahmen", style: "height:200px;aspect-ratio:auto" }
            }
        },
        Some(None) => return rsx! {},
        Some(Some(w)) => rsx! { WetterKarte { wetter: w.clone(), ort: daten.clone() } },
    };
    rsx! {
        crate::werkzeug::ModulCss { name: "wetter".to_string(), css: WETTER_CSS }
        {inhalt}
    }
}

/// Eine Zahl aus JSON als Text fuer die Adresse.
fn zahl_text(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

fn f(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Die Stelle des Zeigers waagerecht im Element `selektor` (0..1) - aus einem Zeiger-Ereignis.
fn zeiger_anteil(e: &PointerEvent, selektor: &str) -> Option<f64> {
    let roh = e.data().downcast::<web_sys::PointerEvent>()?.clone();
    let el = roh.target()?.dyn_into::<web_sys::Element>().ok()?.closest(selektor).ok()??;
    let r = el.get_bounding_client_rect();
    (r.width() > 0.0).then(|| ((f64::from(roh.client_x()) - r.left()) / r.width()).clamp(0.0, 1.0))
}

/// Die ganze Wetterkarte (jetzt, 24-h-Verlauf, 7 Tage) - auch fuer andere Teile nutzbar.
#[component]
pub fn WetterKarte(wetter: Value, ort: Value) -> Element {
    let mut art = use_signal(|| "temp");
    let mut stunde = use_signal(|| None::<usize>);
    let stunden: Vec<Value> = wetter["stunden"].as_array().cloned().unwrap_or_default();
    let tage: Vec<Value> = wetter["tage"].as_array().cloned().unwrap_or_default();
    let jetzt = &wetter["aktuell"];

    // Was oben steht: die gewaehlte Stunde oder jetzt (mit dem Niederschlag der ersten Stunde).
    let (s, regen) = match stunde().and_then(|i| stunden.get(i)) {
        Some(s) => (s.clone(), f(&s["regen"])),
        None => (jetzt.clone(), stunden.first().and_then(|s| f(&s["regen"]))),
    };
    let zeit = s["zeit"].as_str().unwrap_or("");
    let zeit_text = format!("{}, {}", wochentag(zeit, true), zeit.get(11..16).unwrap_or(""));
    let temp = f(&s["temperatur"]).map_or("–".into(), |t| (t.round() as i64).to_string());
    let code = code_aus(&s["code"]);
    let fuss = ["name", "region", "land"].iter().filter_map(|k| ort[*k].as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ");
    let link = wetter["link"].as_str().unwrap_or("https://open-meteo.com/").to_string();
    let anzahl = stunden.len();

    rsx! {
        div { class: "we-kopf",
            div { class: "we-jetzt",
                WetterIcon { code, tag: tag_aus(&s["tag"]), groesse: 64 }
                span { class: "we-temp", "{temp}" }
                span { class: "we-einheit", "°C" }
                div { class: "we-details",
                    if let Some(r) = regen {
                        "Niederschlag: {r} %"
                    } else {
                        "Gefühlt: {grad(f(&s[\"gefuehlt\"]))}"
                    }
                    br {}
                    if let Some(h) = f(&s["feuchte"]) {
                        "Luftfeuchte: {h} %"
                        br {}
                    }
                    "Wind: {f(&s[\"wind\"]).unwrap_or(0.0).round()} km/h"
                }
            }
            div { class: "we-ort",
                div { class: "we-titel", "Wetter" }
                div { class: "we-zeit", "{zeit_text}" }
                div { class: "we-zustand", "{wetter_text(code)}" }
            }
        }
        div { class: "we-reiter",
            for (id, name) in [("temp", "Temperatur"), ("regen", "Niederschlag"), ("wind", "Wind")] {
                button {
                    r#type: "button",
                    class: if art() == id { "aktiv" },
                    onclick: move |_| art.set(id),
                    "{name}"
                }
            }
        }
        svg {
            class: "we-verlauf",
            view_box: "0 0 {B} {H + 22.0}",
            preserve_aspect_ratio: "none",
            // Ueber den Verlauf fahren zeigt die Stunde oben - wie bei Google.
            onpointermove: move |e| {
                if anzahl > 0 {
                    if let Some(a) = zeiger_anteil(&e, ".we-verlauf") {
                        stunde.set(Some((a * (anzahl - 1) as f64).round() as usize));
                    }
                }
            },
            onpointerleave: move |_| stunde.set(None),
            Verlauf { stunden: stunden.clone(), art: art() }
        }
        div { class: "we-tage",
            for (i, t) in tage.iter().enumerate() {
                div {
                    key: "{i}",
                    class: if i == 0 { "we-tag aktiv" } else { "we-tag" },
                    title: "{wetter_text(code_aus(&t[\"code\"]))}",
                    span { "{wochentag(t[\"zeit\"].as_str().unwrap_or(\"\"), false)}" }
                    WetterIcon { code: code_aus(&t["code"]), tag: true, groesse: 40 }
                    span {
                        b { "{grad(f(&t[\"max\"]))}" }
                        " "
                        i { "{grad(f(&t[\"min\"]))}" }
                    }
                }
            }
        }
        div { class: "we-fuss",
            "{fuss} · "
            a { href: "{link}", target: "_blank", rel: "noreferrer", "Open-Meteo" }
        }
    }
}

/// Der Verlauf als SVG: Temperatur als Flaeche, Niederschlag als Balken, Wind als Zahlen -
/// jede dritte Stunde beschriftet.
#[component]
fn Verlauf(stunden: Vec<Value>, art: &'static str) -> Element {
    let feld = if art == "temp" { "temperatur" } else { art };
    let werte: Vec<f64> = stunden.iter().map(|s| f(&s[feld]).unwrap_or(0.0)).collect();
    let n = werte.len();
    if n == 0 {
        return rsx! {};
    }
    let x = move |i: usize| if n > 1 { 10.0 + (i as f64 / (n - 1) as f64) * (B - 20.0) } else { B / 2.0 };
    let zeiten: Vec<(f64, String)> = stunden
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 3 == 0)
        .map(|(i, s)| (x(i), s["zeit"].as_str().and_then(|z| z.get(11..16)).unwrap_or("").to_string()))
        .collect();
    let zeit_texte = rsx! {
        for (zx, z) in zeiten {
            text { x: "{zx}", y: "{H + 16.0}", text_anchor: "middle", "{z}" }
        }
    };
    match art {
        "regen" => {
            let breite = (B - 20.0) / n as f64 - 4.0;
            rsx! {
                for (i, w) in werte.iter().enumerate() {
                    rect {
                        x: "{x(i) - breite / 2.0}",
                        y: "{H - (w / 100.0) * (H - 22.0)}",
                        width: "{breite}",
                        height: "{((w / 100.0) * (H - 22.0)).max(1.0)}",
                        rx: "2",
                        fill: "#4fa3ff",
                    }
                    if i % 3 == 0 {
                        text { x: "{x(i)}", y: "{H - (w / 100.0) * (H - 22.0) - 5.0}", text_anchor: "middle", "{w} %" }
                    }
                }
                {zeit_texte}
            }
        }
        "wind" => rsx! {
            for (i, w) in werte.iter().enumerate().filter(|(i, _)| i % 3 == 0) {
                text { x: "{x(i)}", y: "{H / 2.0}", text_anchor: "middle", "{w.round()} km/h" }
                path {
                    d: "M{x(i) - 6.0} {H / 2.0 + 14.0}h12m-4-4l4 4-4 4",
                    stroke: "#9aa0a6",
                    stroke_width: "1.5",
                    fill: "none",
                }
            }
            {zeit_texte}
        },
        _ => {
            let min = werte.iter().copied().fold(f64::INFINITY, f64::min) - 1.0;
            let spanne = werte.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 1.0 - min;
            let y = move |w: f64| H - 6.0 - ((w - min) / spanne) * (H - 34.0);
            let punkte = werte.iter().enumerate().map(|(i, w)| format!("{},{}", x(i), y(*w))).collect::<Vec<_>>().join(" ");
            let flaeche = format!("{},{H} {punkte} {},{H}", x(0), x(n - 1));
            rsx! {
                defs {
                    linearGradient { id: "we-grad", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0", stop_color: "#fbbc04", stop_opacity: ".35" }
                        stop { offset: "1", stop_color: "#fbbc04", stop_opacity: "0" }
                    }
                }
                polygon { points: "{flaeche}", fill: "url(#we-grad)" }
                polyline { points: "{punkte}", fill: "none", stroke: "#fbbc04", stroke_width: "2" }
                for (i, w) in werte.iter().enumerate().filter(|(i, _)| i % 3 == 0) {
                    text { x: "{x(i)}", y: "{y(*w) - 8.0}", text_anchor: "middle", "{w.round()}" }
                }
                {zeit_texte}
            }
        }
    }
}

/// Das CSS des Moduls (auch fuer die Wetterkarte an anderer Stelle).
pub const WETTER_CSS: &str = r#"
.we-kopf { display: flex; justify-content: space-between; gap: 16px; flex-wrap: wrap; }
.we-jetzt { display: flex; align-items: flex-start; gap: 6px; }
.we-temp { font-size: 64px; line-height: 1; margin-left: 4px; }
.we-einheit { font-size: 16px; margin-top: 6px; }
.we-details { margin-left: 18px; font-size: 13px; line-height: 1.6; color: var(--gedimmt); }
.we-ort { text-align: right; }
.we-titel { font-size: 22px; }
.we-zeit, .we-zustand { color: var(--gedimmt); font-size: 14px; }
.we-reiter { display: flex; gap: 4px; margin: 14px 0 4px; border-bottom: 1px solid var(--kante); }
.we-reiter button { font: 14px Arial; background: none; border: 0; border-bottom: 3px solid transparent; color: var(--gedimmt); padding: 6px 10px; cursor: pointer; }
.we-reiter button.aktiv { color: var(--schrift); border-bottom-color: #fbbc04; }
.we-verlauf { width: 100%; height: 132px; display: block; overflow: visible; cursor: crosshair; touch-action: pan-y; }
.we-verlauf text { font-size: 11px; fill: var(--gedimmt); }
.we-tage { display: grid; grid-template-columns: repeat(7, 1fr); gap: 4px; margin-top: 8px; }
.we-tag { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 6px 0; border-radius: var(--r); font-size: 13px; }
.we-tag.aktiv { background: var(--flaeche2); }
.we-tag b { font-weight: 400; color: var(--schrift); }
.we-tag i { font-style: normal; color: var(--gedimmt); }
.we-fuss { margin-top: 10px; font-size: 12px; color: var(--gedimmt); }
"#;
