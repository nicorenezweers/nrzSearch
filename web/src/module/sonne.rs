//! Modul "sonne": "Sonnenaufgang", "Sonnenuntergang Hamburg" - Auf- und Untergang, Tageslaenge
//! und der Stand der Sonne als Bogen. Einstellung: "ort" (ohne Angabe).
//! Portierung von public/module/sonne.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss};
use dioxus::prelude::*;
use serde_json::Value;
use std::f64::consts::PI;
use wasm_bindgen::JsValue;

const CSS: &str = r#"
.so-zeiten { display: flex; gap: 32px; flex-wrap: wrap; margin-bottom: 8px; }
.so-zeiten div { display: flex; flex-direction: column; }
.so-bogen { width: 100%; max-width: 420px; display: block; }
.so-bahn { fill: none; stroke: var(--gedimmt); stroke-dasharray: 4 4; }
.so-horizont { stroke: var(--kante); }
.so-sonne { fill: #fbbc04; }
.so-bogen text { font-size: 10px; fill: var(--gedimmt); }
"#;

const RAD: f64 = PI / 180.0;
const TAG_MS: f64 = 864e5;

/// Auf- und Untergang in Minuten nach Mitternacht UTC (oder Polartag/-nacht).
enum Zeiten {
    Polar { tag: bool },
    Normal { auf: f64, unter: f64, mittag: f64 },
}

/// Tage seit 1970-01-01 fuer ein Datum (Monat 1-12; Tag 0 = letzter Tag des Vormonats).
fn tage_seit_1970(j: i64, m: i64, t: i64) -> i64 {
    let j = if m <= 2 { j - 1 } else { j };
    let ara = j.div_euclid(400);
    let jda = j - ara * 400;
    let mp = (m + 9) % 12;
    let tdj = (153 * mp + 2) / 5 + t - 1;
    let tda = jda * 365 + jda / 4 - jda / 100 + tdj;
    ara * 146097 + tda - 719468
}

/// Sonnenstand nach NOAA fuer den Tag (UTC) von `jetzt`.
fn sonnenzeiten(jetzt: &js_sys::Date, lat: f64, lon: f64) -> Zeiten {
    let jahr = i64::from(jetzt.get_utc_full_year());
    let heute = tage_seit_1970(jahr, i64::from(jetzt.get_utc_month()) + 1, i64::from(jetzt.get_utc_date()));
    let tag = (heute - tage_seit_1970(jahr, 1, 0)) as f64;
    let g = (2.0 * PI / 365.0) * (tag - 1.0);
    let eqzeit = 229.18 * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos() - 0.040849 * (2.0 * g).sin());
    let dekl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos() + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();
    let cos_h = (90.833 * RAD).cos() / ((lat * RAD).cos() * dekl.cos()) - (lat * RAD).tan() * dekl.tan();
    if cos_h > 1.0 {
        return Zeiten::Polar { tag: false };
    }
    if cos_h < -1.0 {
        return Zeiten::Polar { tag: true };
    }
    let ha = cos_h.acos() / RAD;
    Zeiten::Normal { auf: 720.0 - 4.0 * (lon + ha) - eqzeit, unter: 720.0 - 4.0 * (lon - ha) - eqzeit, mittag: 720.0 - 4.0 * lon - eqzeit }
}

/// Ein Intl.DateTimeFormat "de-DE" mit Zeitzone, nur Stunde und Minute.
fn uhr_format(zone: &str) -> js_sys::Intl::DateTimeFormat {
    let o = js_sys::Object::new();
    for (k, v) in [("hour", "2-digit"), ("minute", "2-digit")] {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v.into());
    }
    if !zone.is_empty() {
        let _ = js_sys::Reflect::set(&o, &"timeZone".into(), &zone.into());
    }
    js_sys::Intl::DateTimeFormat::new(&js_sys::Array::of1(&"de-DE".into()), &o)
}

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let q = ctx.gruppe(1).or_else(|| ctx.cfg_str("ort").map(str::trim).filter(|o| !o.is_empty())).unwrap_or("Berlin").to_string();
    let ort = crate::api::get_oder_nichts("/api/geo", &[("q", q.as_str())]).await?.get("ort").cloned()?;
    ort.get("lat").filter(|l| !l.is_null())?;
    Some(ort)
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let s = |k: &str| daten.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let f = |k: &str| daten.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN);
    let heute = js_sys::Date::new_0();
    let jetzt_ms = heute.get_time();
    let utc_mitternacht = (jetzt_ms / TAG_MS).floor() * TAG_MS;
    let fmt = uhr_format(&s("zeitzone"));
    let um = |min: f64| {
        let d = js_sys::Date::new(&JsValue::from_f64(utc_mitternacht + min * 6e4));
        fmt.format().call1(&JsValue::NULL, &d).ok().and_then(|v| v.as_string()).unwrap_or_default()
    };
    let ortname = [s("name"), s("land")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(", ");
    let titel = format!("Sonne in {ortname}");

    let inhalt = match sonnenzeiten(&heute, f("lat"), f("lon")) {
        Zeiten::Polar { tag } => rsx! {
            div { class: "w-gross",
                if tag { "Polartag – die Sonne geht nicht unter" } else { "Polarnacht – die Sonne geht nicht auf" }
            }
        },
        Zeiten::Normal { auf, unter, mittag } => {
            let laenge = unter - auf;
            let jetzt = (jetzt_ms - utc_mitternacht) / 6e4;
            let anteil = ((jetzt - auf) / laenge).clamp(0.0, 1.0);
            let x = 20.0 + anteil * 260.0;
            let y = 110.0 - (anteil * PI).sin() * 90.0;
            let ist_tag = jetzt > auf && jetzt < unter;
            let (t_auf, t_unter, t_mittag) = (um(auf), um(unter), um(mittag));
            let dauer = format!("{} h {} min", (laenge / 60.0).floor(), (laenge % 60.0).round());
            rsx! {
                div { class: "so-zeiten",
                    div { span { class: "w-text", "Sonnenaufgang" } span { class: "w-gross", "{t_auf}" } }
                    div { span { class: "w-text", "Sonnenuntergang" } span { class: "w-gross", "{t_unter}" } }
                    div { span { class: "w-text", "Tageslänge" } span { class: "w-gross", "{dauer}" } }
                }
                svg { class: "so-bogen", view_box: "0 0 300 130", "aria-hidden": "true",
                    path { d: "M20 110 Q150 -70 280 110", class: "so-bahn" }
                    line { x1: "0", y1: "110", x2: "300", y2: "110", class: "so-horizont" }
                    if ist_tag {
                        circle { cx: "{x}", cy: "{y}", r: "9", class: "so-sonne" }
                    }
                    text { x: "20", y: "126", "{t_auf}" }
                    text { x: "150", y: "126", text_anchor: "middle", "{t_mittag}" }
                    text { x: "280", y: "126", text_anchor: "end", "{t_unter}" }
                }
            }
        }
    };
    rsx! {
        ModulCss { name: "sonne".to_string(), css: CSS }
        Kopf { titel }
        {inhalt}
    }
}
