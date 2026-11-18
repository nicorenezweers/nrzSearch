//! Modul "einheiten": "100 m/s in km/h", "5 GB in MB", "30 °C in °F",
//! "Einheiten umrechnen" - Laenge, Masse, Zeit, Geschwindigkeit, Daten,
//! Flaeche, Volumen, Energie, Druck und Temperatur.
//! Portierung von public/module/einheiten.js.

use super::ModulCtx;
use super::finanzrechner::parse_float;
use crate::werkzeug::{Kopf, ModulCss, zahl};
use dioxus::prelude::*;
use serde_json::{Value, json};

const CSS: &str = "
.ei-groesse { margin-bottom: 12px; }
.ei-paar { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.ei-paar > div { display: flex; flex-direction: column; gap: 6px; flex: 1; min-width: 180px; }
.ei-wert, .ei-aus { font-size: 22px; }
.ei-aus { display: block; min-height: 44px; }
.ei-gleich { font-size: 22px; color: var(--gedimmt); }
.ei-formel { margin-top: 10px; }
";

type Einheit = (&'static str, f64, &'static str);

/// Jede Groesse: Einheit -> Faktor zur Grundeinheit (und Name).
const GROESSEN: &[(&str, &[Einheit])] = &[
    ("Länge", &[("m", 1.0, "Meter"), ("km", 1e3, "Kilometer"), ("cm", 0.01, "Zentimeter"), ("mm", 1e-3, "Millimeter"), ("µm", 1e-6, "Mikrometer"), ("nm", 1e-9, "Nanometer"), ("mi", 1609.344, "Meile"), ("meile", 1609.344, "Meile"), ("meilen", 1609.344, "Meilen"), ("yd", 0.9144, "Yard"), ("ft", 0.3048, "Fuß"), ("fuss", 0.3048, "Fuß"), ("in", 0.0254, "Zoll"), ("zoll", 0.0254, "Zoll"), ("sm", 1852.0, "Seemeile"), ("ly", 9.4607e15, "Lichtjahr")]),
    ("Masse", &[("kg", 1.0, "Kilogramm"), ("g", 1e-3, "Gramm"), ("mg", 1e-6, "Milligramm"), ("t", 1e3, "Tonne"), ("lb", 0.45359237, "Pfund (lb)"), ("oz", 0.028349523, "Unze"), ("pfund", 0.5, "Pfund")]),
    ("Zeit", &[("s", 1.0, "Sekunde"), ("ms", 1e-3, "Millisekunde"), ("sek", 1.0, "Sekunde"), ("sekunde", 1.0, "Sekunde"), ("sekunden", 1.0, "Sekunden"), ("min", 60.0, "Minute"), ("minute", 60.0, "Minute"), ("minuten", 60.0, "Minuten"), ("h", 3600.0, "Stunde"), ("std", 3600.0, "Stunde"), ("stunde", 3600.0, "Stunde"), ("stunden", 3600.0, "Stunden"), ("d", 86400.0, "Tag"), ("tag", 86400.0, "Tag"), ("tage", 86400.0, "Tage"), ("woche", 604800.0, "Woche"), ("wochen", 604800.0, "Wochen"), ("jahr", 31557600.0, "Jahr"), ("jahre", 31557600.0, "Jahre")]),
    ("Geschwindigkeit", &[("m/s", 1.0, "Meter pro Sekunde"), ("km/h", 1.0 / 3.6, "Kilometer pro Stunde"), ("kmh", 1.0 / 3.6, "Kilometer pro Stunde"), ("mph", 0.44704, "Meilen pro Stunde"), ("kn", 0.514444, "Knoten"), ("knoten", 0.514444, "Knoten"), ("ft/s", 0.3048, "Fuß pro Sekunde"), ("mach", 343.0, "Mach")]),
    ("Datenmenge", &[("b", 1.0, "Byte"), ("byte", 1.0, "Byte"), ("bit", 0.125, "Bit"), ("kb", 1e3, "Kilobyte"), ("mb", 1e6, "Megabyte"), ("gb", 1e9, "Gigabyte"), ("tb", 1e12, "Terabyte"), ("pb", 1e15, "Petabyte"), ("kib", 1024.0, "Kibibyte"), ("mib", 1048576.0, "Mebibyte"), ("gib", 1073741824.0, "Gibibyte"), ("tib", 1099511627776.0, "Tebibyte"), ("kbit", 125.0, "Kilobit"), ("mbit", 125e3, "Megabit"), ("gbit", 125e6, "Gigabit")]),
    ("Fläche", &[("m²", 1.0, "Quadratmeter"), ("m2", 1.0, "Quadratmeter"), ("km²", 1e6, "Quadratkilometer"), ("km2", 1e6, "Quadratkilometer"), ("cm²", 1e-4, "Quadratzentimeter"), ("ha", 1e4, "Hektar"), ("ar", 100.0, "Ar"), ("acre", 4046.8564, "Acre")]),
    ("Volumen", &[("l", 1.0, "Liter"), ("liter", 1.0, "Liter"), ("ml", 1e-3, "Milliliter"), ("cl", 0.01, "Zentiliter"), ("dl", 0.1, "Deziliter"), ("m³", 1e3, "Kubikmeter"), ("m3", 1e3, "Kubikmeter"), ("gal", 3.785411784, "Gallone (US)"), ("gallone", 3.785411784, "Gallone (US)"), ("pint", 0.473176, "Pint (US)"), ("tasse", 0.25, "Tasse")]),
    ("Energie", &[("j", 1.0, "Joule"), ("kj", 1e3, "Kilojoule"), ("cal", 4.184, "Kalorie"), ("kcal", 4184.0, "Kilokalorie"), ("wh", 3600.0, "Wattstunde"), ("kwh", 3.6e6, "Kilowattstunde"), ("ev", 1.602176634e-19, "Elektronenvolt")]),
    ("Druck", &[("pa", 1.0, "Pascal"), ("hpa", 100.0, "Hektopascal"), ("kpa", 1e3, "Kilopascal"), ("bar", 1e5, "Bar"), ("mbar", 100.0, "Millibar"), ("psi", 6894.757, "psi"), ("atm", 101325.0, "Atmosphäre")]),
    ("Leistung", &[("w", 1.0, "Watt"), ("kw", 1e3, "Kilowatt"), ("mw", 1e6, "Megawatt"), ("ps", 735.49875, "PS"), ("hp", 745.7, "hp")]),
];

/// Temperatur rechnet nicht mit Faktoren: Einheit, Name, Art (c/f/k).
const TEMPERATUR: &[(&str, &str, char)] = &[
    ("°c", "Grad Celsius", 'c'), ("c", "Grad Celsius", 'c'), ("celsius", "Grad Celsius", 'c'),
    ("°f", "Grad Fahrenheit", 'f'), ("f", "Grad Fahrenheit", 'f'), ("fahrenheit", "Grad Fahrenheit", 'f'),
    ("k", "Kelvin", 'k'), ("kelvin", "Kelvin", 'k'),
];
/// Die Temperatur-Einheiten in der Auswahl.
const TEMPERATUR_AUSWAHL: &[&str] = &["°c", "°f", "k"];

fn temperatur(k: &str) -> Option<&'static (&'static str, &'static str, char)> {
    TEMPERATUR.iter().find(|t| t.0 == k)
}

fn einheit(groesse: &str, k: &str) -> Option<&'static Einheit> {
    GROESSEN.iter().find(|g| g.0 == groesse)?.1.iter().find(|e| e.0 == k)
}

/// Einheit -> (Groesse, Schluessel), oder None.
fn finden(e: &str) -> Option<(String, String)> {
    let k = e.to_lowercase();
    if temperatur(&k).is_some() {
        return Some(("Temperatur".into(), k));
    }
    GROESSEN.iter().find(|g| g.1.iter().any(|x| x.0 == k)).map(|g| (g.0.to_string(), k))
}

fn umrechnen(wert: f64, groesse: &str, von: &str, nach: &str) -> f64 {
    if groesse == "Temperatur" {
        let celsius = match temperatur(von).map(|t| t.2) {
            Some('f') => (wert - 32.0) * 5.0 / 9.0,
            Some('k') => wert - 273.15,
            _ => wert,
        };
        return match temperatur(nach).map(|t| t.2) {
            Some('f') => celsius * 9.0 / 5.0 + 32.0,
            Some('k') => celsius + 273.15,
            _ => celsius,
        };
    }
    let f = |k: &str| einheit(groesse, k).map_or(f64::NAN, |e| e.1);
    (wert * f(von)) / f(nach)
}

fn einheiten(groesse: &str) -> Vec<&'static str> {
    if groesse == "Temperatur" {
        return TEMPERATUR_AUSWAHL.to_vec();
    }
    GROESSEN.iter().find(|g| g.0 == groesse).map(|g| g.1.iter().map(|e| e.0).collect()).unwrap_or_default()
}

fn name(groesse: &str, k: &str) -> &'static str {
    if groesse == "Temperatur" {
        return temperatur(k).map_or("", |t| t.1);
    }
    einheit(groesse, k).map_or("", |e| e.2)
}

/// Die Auswahl einer Groesse: je Name nur die erste Einheit.
fn optionen(groesse: &str) -> Vec<(&'static str, &'static str)> {
    let mut aus: Vec<(&str, &str)> = Vec::new();
    for k in einheiten(groesse) {
        let n = name(groesse, k);
        if !aus.iter().any(|(_, m)| *m == n) {
            aus.push((k, n));
        }
    }
    aus
}

/// Die Einheit, die in der Auswahl fuer `k` steht ("fuss" -> "ft", "celsius" -> "°c").
fn in_auswahl(groesse: &str, k: &str) -> String {
    let n = name(groesse, k);
    optionen(groesse).into_iter().find(|(_, m)| *m == n).map_or(k, |(x, _)| x).to_string()
}

/// Wie Number.prototype.toExponential(4) - mit Komma.
fn exponential(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let roh = format!("{n:.4e}");
    let (mantisse, exp) = roh.split_once('e').unwrap_or((&roh, "0"));
    let exp = if exp.starts_with('-') { exp.to_string() } else { format!("+{exp}") };
    format!("{mantisse}e{exp}").replacen('.', ",", 1)
}

fn ergebnis_text(n: f64) -> String {
    let a = n.abs();
    if (1e-4..1e12).contains(&a) {
        let roh = zahl(n, if a < 1.0 { 6 } else if a < 100.0 { 4 } else { 2 });
        // Wie .replace(/,?0+$/, ''): Nullen am Ende weg, dann ein Komma davor.
        let ohne = roh.trim_end_matches('0');
        if ohne.len() == roh.len() {
            return roh;
        }
        return ohne.strip_suffix(',').unwrap_or(ohne).to_string();
    }
    exponential(n)
}

/// Wie String(zahl) in JS, aber mit Komma.
fn wert_text(w: f64) -> String {
    if w.is_nan() {
        return "NaN".into();
    }
    js_sys::Number::from(w).to_string_with_radix(10).map(String::from).unwrap_or_else(|_| w.to_string()).replacen('.', ",", 1)
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let Some(e2) = ctx.gruppe(2) else {
        return Some(json!({ "wert": 1, "von": { "groesse": "Länge", "k": "km" }, "nach": { "groesse": "Länge", "k": "mi" } }));
    };
    let von = finden(e2)?;
    let nach = finden(ctx.gruppe(3).unwrap_or_default())?;
    if von.0 != nach.0 {
        return None;
    }
    // "1.5" und "1,5" sind beide anderthalb, "1.000,5" ist tausend.
    let roh = ctx.gruppe(1).unwrap_or_default();
    let wert = if roh.contains(',') { parse_float(&roh.replace('.', "").replacen(',', ".", 1)) } else { parse_float(roh) };
    Some(json!({ "wert": wert, "von": { "groesse": von.0, "k": von.1 }, "nach": { "groesse": nach.0, "k": nach.1 } }))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-einheiten">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let start_groesse = daten["von"]["groesse"].as_str().unwrap_or("Länge").to_string();
    let mut groesse = use_signal(|| start_groesse.clone());
    let mut von = use_signal(|| in_auswahl(&start_groesse, daten["von"]["k"].as_str().unwrap_or_default()));
    let mut nach = use_signal(|| in_auswahl(&start_groesse, daten["nach"]["k"].as_str().unwrap_or_default()));
    let mut eingabe = use_signal(|| wert_text(daten["wert"].as_f64().unwrap_or(f64::NAN)));

    let g = groesse();
    let (v, n) = (von(), nach());
    let w = {
        let roh: String = eingabe().chars().filter(|c| !c.is_whitespace()).collect();
        let z = parse_float(&roh.replacen(',', ".", 1));
        if z.is_nan() { 0.0 } else { z }
    };
    let e = umrechnen(w, &g, &v, &n);
    let formel = format!("{} {} = {} {}", ergebnis_text(w), name(&g, &v), ergebnis_text(e), name(&g, &n));
    let auswahl = optionen(&g);

    rsx! {
        ModulCss { name: "einheiten".to_string(), css: CSS }
        Kopf { titel: "Einheitenrechner".to_string() }
        select {
            class: "w-eingabe ei-groesse",
            onchange: move |ev| {
                let neu = ev.value();
                let o = optionen(&neu);
                von.set(o.first().map(|x| x.0.to_string()).unwrap_or_default());
                nach.set(o.get(1).or(o.first()).map(|x| x.0.to_string()).unwrap_or_default());
                groesse.set(neu);
            },
            for name in GROESSEN.iter().map(|x| x.0).chain(["Temperatur"]) {
                option { key: "{name}", value: "{name}", selected: name == g, "{name}" }
            }
        }
        div { class: "ei-paar",
            div {
                input {
                    class: "w-eingabe ei-wert",
                    inputmode: "decimal",
                    value: "{eingabe}",
                    oninput: move |ev| eingabe.set(ev.value()),
                }
                select {
                    key: "von-{g}",
                    class: "w-eingabe ei-von",
                    oninput: move |ev| von.set(ev.value()),
                    for (k, nm) in auswahl.iter() {
                        option { key: "{k}", value: "{k}", selected: *k == v, "{nm}" }
                    }
                }
            }
            span { class: "ei-gleich", "=" }
            div {
                output { class: "w-eingabe ei-aus", "{ergebnis_text(e)}" }
                select {
                    key: "nach-{g}",
                    class: "w-eingabe ei-nach",
                    oninput: move |ev| nach.set(ev.value()),
                    for (k, nm) in auswahl.iter() {
                        option { key: "{k}", value: "{k}", selected: *k == n, "{nm}" }
                    }
                }
            }
        }
        div { class: "w-text ei-formel", "{formel}" }
    }
}
