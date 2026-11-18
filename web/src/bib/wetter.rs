//! Wetter-Helfer (Icons, Texte, Wochentage) - Portierung von public/lib/wetter.js.
//!
//! Zeichen und Texte nach dem WMO-Code von Open-Meteo, gemeinsam fuer die Info-Karte und das
//! Wetter-Modul. Die Icons sind SVG in Rust (farbig wie bei Google: gelbe Sonne, graue Wolke,
//! blaue Tropfen).

use dioxus::prelude::*;
use serde_json::Value;

/// Text und Bildart zu einem WMO-Code.
fn art(code: i64) -> (&'static str, &'static str) {
    match code {
        0 => ("Klar", "sonne"),
        1 => ("Überwiegend klar", "sonne"),
        2 => ("Teilweise bewölkt", "halb"),
        3 => ("Bewölkt", "wolke"),
        45 => ("Nebel", "nebel"),
        48 => ("Reifnebel", "nebel"),
        51 => ("Leichter Nieselregen", "niesel"),
        53 => ("Nieselregen", "niesel"),
        55 => ("Starker Nieselregen", "niesel"),
        56 | 57 => ("Gefrierender Niesel", "niesel"),
        61 => ("Leichter Regen", "regen"),
        63 => ("Regen", "regen"),
        65 => ("Starker Regen", "regen"),
        66 | 67 => ("Gefrierender Regen", "regen"),
        71 => ("Leichter Schneefall", "schnee"),
        73 => ("Schneefall", "schnee"),
        75 => ("Starker Schneefall", "schnee"),
        77 => ("Schneegriesel", "schnee"),
        80 | 81 => ("Regenschauer", "schauer"),
        82 => ("Heftige Schauer", "schauer"),
        85 | 86 => ("Schneeschauer", "schnee"),
        95 => ("Gewitter", "gewitter"),
        96 | 99 => ("Gewitter mit Hagel", "gewitter"),
        _ => ("", "wolke"),
    }
}

/// Der Text zum WMO-Code ("Leichter Regen"), unbekannt: "".
pub fn wetter_text(code: i64) -> &'static str {
    art(code).0
}

/// Der Code aus einem JSON-Wert (fehlt er: -1).
pub fn code_aus(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).unwrap_or(-1)
}

/// Tag oder Nacht aus einem JSON-Wert (`is_day` = 1/0; fehlt er: Tag).
pub fn tag_aus(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        _ => true,
    }
}

/// Eine Temperatur als "12°" (fehlt sie: "–").
pub fn grad(t: Option<f64>) -> String {
    match t {
        Some(t) if t.is_finite() => format!("{}°", t.round() as i64),
        _ => "–".into(),
    }
}

/// Der Wochentag zu "2026-11-13": "Fr" bzw. lang "Freitag".
pub fn wochentag(tag: &str, lang: bool) -> String {
    const KURZ: [&str; 7] = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"];
    const LANG: [&str; 7] = ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"];
    let d = js_sys::Date::new(&format!("{}T12:00:00", tag.get(..10).unwrap_or(tag)).into());
    if d.get_time().is_nan() {
        return String::new();
    }
    let i = d.get_day() as usize % 7;
    (if lang { LANG[i] } else { KURZ[i] }).to_string()
}

/// Ein Wetterzeichen als SVG (`class="wetter-icon"`).
#[component]
pub fn WetterIcon(code: i64, #[props(default = true)] tag: bool, #[props(default = 48)] groesse: u32) -> Element {
    let bild = match art(code).1 {
        "sonne" => rsx! { Himmel { tag } },
        "halb" => rsx! {
            g { transform: "translate(-6 -6) scale(.8)", Himmel { tag } }
            Wolke { y: 4.0 }
        },
        "nebel" => rsx! {
            Wolke { y: -8.0 }
            g { stroke: "#9aa0a6", stroke_width: "3", stroke_linecap: "round", path { d: "M10 38h28M14 44h20" } }
        },
        "niesel" => rsx! { Wolke { y: -6.0 } Tropfen { n: 2 } },
        "regen" => rsx! { Wolke { y: -6.0, farbe: "#9aa0a6" } Tropfen { n: 3 } },
        "schauer" => rsx! {
            g { transform: "translate(-6 -8) scale(.75)", Himmel { tag } }
            Wolke { y: -4.0 }
            Tropfen { n: 3 }
        },
        "schnee" => rsx! {
            Wolke { y: -6.0 }
            g { fill: "#e8eaed",
                circle { cx: "16", cy: "43", r: "2.2" }
                circle { cx: "24", cy: "45", r: "2.2" }
                circle { cx: "32", cy: "43", r: "2.2" }
            }
        },
        "gewitter" => rsx! {
            Wolke { y: -8.0, farbe: "#9aa0a6" }
            path { d: "M26 34l-6 9h5l-3 7 9-11h-5l3-5z", fill: "#fbbc04" }
        },
        // "wolke"
        _ => rsx! { Wolke { y: -6.0, farbe: "#9aa0a6" } Wolke { y: 2.0 } },
    };
    rsx! {
        svg {
            class: "wetter-icon",
            width: "{groesse}",
            height: "{groesse}",
            view_box: "0 0 48 52",
            "aria-hidden": "true",
            {bild}
        }
    }
}

/// Sonne am Tag, Mond in der Nacht.
#[component]
fn Himmel(tag: bool) -> Element {
    if tag {
        rsx! {
            circle { cx: "24", cy: "24", r: "9", fill: "#fbbc04" }
            g { stroke: "#fbbc04", stroke_width: "3", stroke_linecap: "round",
                path { d: "M24 5v5M24 38v5M5 24h5M38 24h5M10.6 10.6l3.5 3.5M33.9 33.9l3.5 3.5M37.4 10.6l-3.5 3.5M14.1 33.9l-3.5 3.5" }
            }
        }
    } else {
        rsx! { path { d: "M30 8a16 16 0 1 0 10 28A13 13 0 0 1 30 8z", fill: "#c4c7c5" } }
    }
}

#[component]
fn Wolke(y: f64, #[props(default = "#dadce0")] farbe: &'static str) -> Element {
    rsx! {
        path {
            transform: "translate(0 {y})",
            d: "M14 38h22a8 8 0 0 0 1-15.9A11 11 0 0 0 16 20a9 9 0 0 0-2 18z",
            fill: "{farbe}",
        }
    }
}

#[component]
fn Tropfen(n: usize) -> Element {
    const PUNKTE: [(i32, i32); 3] = [(16, 40), (24, 42), (32, 40)];
    rsx! {
        g { stroke: "#4fa3ff", stroke_width: "3", stroke_linecap: "round",
            for (x, y) in PUNKTE.iter().take(n) {
                path { d: "M{x} {y}l-2 5" }
            }
        }
    }
}
