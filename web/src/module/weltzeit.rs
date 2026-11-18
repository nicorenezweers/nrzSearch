//! Modul "weltzeit": "Uhrzeit Tokio", "Zeit in New York", "Uhrzeit" - die Ortszeit und der
//! Unterschied zur eigenen Zeit. Einstellung: "ort" (ohne Angabe), z. B. "Berlin".
//! Portierung von public/module/weltzeit.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, use_intervall};
use dioxus::prelude::*;
use js_sys::Intl::DateTimeFormat;
use serde_json::{Value, json};
use wasm_bindgen::JsValue;

const CSS: &str = r#"
.wz-uhr { font-size: 56px; font-variant-numeric: tabular-nums; line-height: 1.2; }
.wz-tag { font-size: 18px; margin-bottom: 8px; }
"#;

/// Ein Intl.DateTimeFormat mit Sprache, Zeitzone (leer = eigene) und Optionen.
fn format(sprache: &str, zone: &str, optionen: &[(&str, &str)]) -> DateTimeFormat {
    let o = js_sys::Object::new();
    for (k, v) in optionen {
        let _ = js_sys::Reflect::set(&o, &(*k).into(), &(*v).into());
    }
    if !zone.is_empty() {
        let _ = js_sys::Reflect::set(&o, &"timeZone".into(), &zone.into());
    }
    let sprachen = if sprache.is_empty() { js_sys::Array::new() } else { js_sys::Array::of1(&sprache.into()) };
    DateTimeFormat::new(&sprachen, &o)
}

fn text(f: &DateTimeFormat, d: &js_sys::Date) -> String {
    f.format().call1(&JsValue::NULL, d).ok().and_then(|v| v.as_string()).unwrap_or_default()
}

/// Die Zeitzone des Browsers.
fn eigene_zone() -> String {
    let opt = format("", "", &[]).resolved_options();
    js_sys::Reflect::get(&opt, &"timeZone".into()).ok().and_then(|v| v.as_string()).unwrap_or_default()
}

/// Tage seit 1970-01-01 (Monat 1-12).
fn tage_seit_1970(j: i64, m: i64, t: i64) -> i64 {
    let j = if m <= 2 { j - 1 } else { j };
    let ara = j.div_euclid(400);
    let jda = j - ara * 400;
    let mp = (m + 9) % 12;
    let tdj = (153 * mp + 2) / 5 + t - 1;
    let tda = jda * 365 + jda / 4 - jda / 100 + tdj;
    ara * 146097 + tda - 719468
}

/// Der Versatz der Zone gegen UTC in Stunden: die Ortszeit als "Wanduhr" gegen jetzt.
fn versatz(zone: &str, jetzt: &js_sys::Date) -> f64 {
    let f = format(
        "en-US",
        zone,
        &[("hourCycle", "h23"), ("year", "numeric"), ("month", "numeric"), ("day", "numeric"), ("hour", "numeric"), ("minute", "numeric")],
    );
    let teil = |art: &str| -> i64 {
        f.format_to_parts(jetzt)
            .iter()
            .find(|p| js_sys::Reflect::get(p, &"type".into()).ok().and_then(|t| t.as_string()).as_deref() == Some(art))
            .and_then(|p| js_sys::Reflect::get(&p, &"value".into()).ok())
            .and_then(|v| v.as_string())
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    let tage = tage_seit_1970(teil("year"), teil("month"), teil("day"));
    let wand = (tage * 1440 + teil("hour") * 60 + teil("minute")) as f64 * 6e4;
    (wand - jetzt.get_time()) / 36e5
}

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let name = ctx.gruppe(1).or_else(|| ctx.cfg_str("ort").map(str::trim).filter(|o| !o.is_empty()));
    let Some(name) = name else {
        return Some(json!({ "name": "Ortszeit", "zeitzone": eigene_zone() }));
    };
    let ort = crate::api::get_oder_nichts("/api/geo", &[("q", name)]).await?.get("ort").cloned()?;
    ort.get("zeitzone").and_then(Value::as_str).filter(|z| !z.is_empty())?;
    Some(ort)
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let s = |k: &str| daten.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let zone = s("zeitzone");
    // Die Formate und der Unterschied werden einmal bestimmt, die Uhr laeuft jede Sekunde.
    let (uhr, tag, diff_text) = use_hook(|| {
        let uhr = format("de-DE", &zone, &[("hour", "2-digit"), ("minute", "2-digit"), ("second", "2-digit")]);
        let tag = format("de-DE", &zone, &[("weekday", "long"), ("day", "numeric"), ("month", "long"), ("year", "numeric")]);
        let jetzt = js_sys::Date::new_0();
        let diff = ((versatz(&zone, &jetzt) + jetzt.get_timezone_offset() / 60.0) * 2.0).round() / 2.0;
        let diff_text = if diff == 0.0 {
            "gleiche Zeit wie bei dir".to_string()
        } else {
            format!("{} Std. {} deiner Zeit", diff.abs().to_string().replace('.', ","), if diff > 0.0 { "vor" } else { "hinter" })
        };
        (uhr, tag, diff_text)
    });
    let mut jetzt = use_signal(js_sys::Date::now);
    use_intervall(1000, move || jetzt.set(js_sys::Date::now()));
    let d = js_sys::Date::new(&JsValue::from_f64(jetzt()));
    let ortname = [s("name"), s("land")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", ");
    rsx! {
        ModulCss { name: "weltzeit".to_string(), css: CSS }
        Kopf { titel: format!("Ortszeit in {ortname}") }
        div { class: "wz-uhr", "{text(&uhr, &d)}" }
        div { class: "wz-tag", "{text(&tag, &d)}" }
        div { class: "w-text", "{zone} · {diff_text}" }
    }
}
