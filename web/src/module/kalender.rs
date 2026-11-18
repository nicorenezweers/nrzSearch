//! Modul "kalender": "Kalender", "Kalender Dezember 2026" - ein Monatsblatt zum Blaettern (mit
//! Wochen, Feiertagen, heute). "Kalenderwoche", "KW", "Feiertage", "Feiertage 2027", "naechster
//! Feiertag" - die ISO-Woche und die gesetzlichen Feiertage. Einstellung: "land" ("DE" -
//! bundesweite Feiertage). Portierung von public/module/kalender.js.
//!
//! Tage werden als Tagesnummer (Tage seit 1970-01-01, ortsunabhaengig) gerechnet; nur "heute"
//! und die Texte kommen aus js_sys::Date / Intl.DateTimeFormat.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss};
use dioxus::prelude::*;
use js_sys::Intl::DateTimeFormat;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wasm_bindgen::JsValue;

const CSS: &str = r#"
.modul-kalender .kal-kopf { display: flex; align-items: center; gap: 6px; margin-bottom: 10px; }
.modul-kalender .kal-monat { flex: 1; order: -1; font: 400 22px "Google Sans", "Segoe UI", Arial; color: var(--schrift); }
.modul-kalender .kal-pfeil, .modul-kalender .kal-heute { height: 34px; border: 1px solid var(--kante); background: none; color: var(--schrift); cursor: pointer; }
.modul-kalender .kal-pfeil { width: 34px; border-radius: 50%; font-size: 20px; line-height: 1; }
.modul-kalender .kal-heute { padding: 0 14px; border-radius: 17px; font: 14px Arial; }
.modul-kalender .kal-pfeil:hover, .modul-kalender .kal-heute:hover { background: var(--flaeche2); }
.modul-kalender .kal-blatt { width: 100%; border-collapse: collapse; table-layout: fixed; }
.modul-kalender .kal-blatt th { padding: 6px 0; font-size: 12px; font-weight: 500; color: var(--gedimmt); }
.modul-kalender .kal-blatt td { padding: 2px; text-align: center; }
.modul-kalender .kal-kw { width: 34px; font-size: 11px; color: var(--gedimmt); }
.modul-kalender .kal-blatt button { position: relative; width: 100%; max-width: 46px; aspect-ratio: 1; border: 0; border-radius: 50%; background: none; color: var(--schrift); font: 15px Arial; cursor: pointer; }
.modul-kalender .kal-blatt button:hover { background: var(--flaeche2); }
.modul-kalender .kal-blatt .fremd { color: var(--flaeche3); }
.modul-kalender .kal-blatt .wochenende:not(.fremd) { color: var(--gedimmt); }
.modul-kalender .kal-blatt .feiertag:not(.fremd) { color: var(--rot); }
.modul-kalender .kal-blatt .feiertag::after { content: ""; position: absolute; left: 50%; bottom: 5px; width: 4px; height: 4px; margin-left: -2px; border-radius: 50%; background: var(--rot); }
.modul-kalender .kal-blatt .heute { background: var(--hoch); color: var(--auf-hoch) !important; font-weight: 700; }
.modul-kalender .kal-blatt .gewaehlt:not(.heute) { box-shadow: inset 0 0 0 2px var(--hoch); }
.modul-kalender .kal-info { min-height: 20px; margin-top: 10px; font-size: 14px; color: var(--snippet); }
.modul-kalender .kal-info b { color: var(--schrift); font-weight: 500; }
.modul-kalender .kal-frei { color: var(--rot); }
.modul-kalender .kal-feiertage { display: flex; flex-wrap: wrap; gap: 6px 16px; margin: 10px 0 0; padding: 10px 0 0; border-top: 1px solid var(--kante); list-style: none; font-size: 13px; color: var(--snippet); }
.modul-kalender .kal-feiertage span { margin-right: 6px; color: var(--rot); font-weight: 600; }
"#;

const MONATE: [&str; 12] = ["januar", "februar", "märz", "april", "mai", "juni", "juli", "august", "september", "oktober", "november", "dezember"];
const MONATE_EN: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
const FUELL: [&str; 19] = ["heute", "aktuell", "online", "dieser", "diesen", "dieses", "monat", "jahr", "für", "fuer", "von", "im", "zeigen", "anzeigen", "deutschland", "mit", "feiertagen", "kw", "kalenderwochen"];
const WOCHENTAGE: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

// ---- Tagesrechnung -----------------------------------------------------------------------

/// Tagesnummer (Tage seit 1970-01-01) fuer Jahr, Monat (0-11, darf ueberlaufen) und Tag (darf
/// ueberlaufen) - wie `new Date(j, m, t)`.
fn tag_nr(j: i32, m: i32, t: i32) -> i64 {
    let j = i64::from(j) + i64::from(m).div_euclid(12);
    let m = i64::from(m).rem_euclid(12) + 1;
    let j = if m <= 2 { j - 1 } else { j };
    let ara = j.div_euclid(400);
    let jda = j - ara * 400;
    let mp = (m + 9) % 12;
    let tdj = (153 * mp + 2) / 5;
    let tda = jda * 365 + jda / 4 - jda / 100 + tdj;
    ara * 146097 + tda - 719468 + i64::from(t) - 1
}

/// Jahr, Monat (0-11) und Tag zu einer Tagesnummer.
fn datum(n: i64) -> (i32, i32, i32) {
    let z = n + 719468;
    let ara = z.div_euclid(146097);
    let tda = z - ara * 146097;
    let jda = (tda - tda / 1460 + tda / 36524 - tda / 146096) / 365;
    let tdj = tda - (365 * jda + jda / 4 - jda / 100);
    let mp = (5 * tdj + 2) / 153;
    let t = tdj - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let j = jda + ara * 400 + i64::from(m <= 2);
    (j as i32, (m - 1) as i32, t as i32)
}

/// Wochentag: 0 = Sonntag … 6 = Samstag (wie getDay).
fn wochentag(n: i64) -> i64 {
    (n + 4).rem_euclid(7)
}

/// Heute (Ortszeit des Browsers) als Tagesnummer.
fn heute() -> i64 {
    let d = js_sys::Date::new_0();
    tag_nr(d.get_full_year() as i32, d.get_month() as i32, d.get_date() as i32)
}

/// Die ISO-Kalenderwoche.
fn kw(n: i64) -> i64 {
    let w = wochentag(n);
    let t = n + 4 - if w == 0 { 7 } else { w };
    let (j, _, _) = datum(t);
    (t - tag_nr(j, 0, 1)) / 7 + 1
}

/// Ostersonntag (Gauss).
fn ostern(j: i32) -> i64 {
    let (a, b, c) = (j % 19, j / 100, j % 100);
    let (d, e) = (b / 4, b % 4);
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let (i, k) = (c / 4, c % 4);
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    tag_nr(j, (h + l - 7 * m + 114) / 31 - 1, (h + l - 7 * m + 114) % 31 + 1)
}

/// Die bundesweiten Feiertage in Deutschland.
fn feiertage(j: i32) -> Vec<(&'static str, i64)> {
    let o = ostern(j);
    vec![
        ("Neujahr", tag_nr(j, 0, 1)),
        ("Karfreitag", o - 2),
        ("Ostermontag", o + 1),
        ("Tag der Arbeit", tag_nr(j, 4, 1)),
        ("Christi Himmelfahrt", o + 39),
        ("Pfingstmontag", o + 50),
        ("Tag der Deutschen Einheit", tag_nr(j, 9, 3)),
        ("1. Weihnachtstag", tag_nr(j, 11, 25)),
        ("2. Weihnachtstag", tag_nr(j, 11, 26)),
    ]
}

// ---- Texte ---------------------------------------------------------------------------------

fn format(optionen: &[(&str, &str)]) -> DateTimeFormat {
    let o = js_sys::Object::new();
    for (k, v) in optionen {
        let _ = js_sys::Reflect::set(&o, &(*k).into(), &(*v).into());
    }
    DateTimeFormat::new(&js_sys::Array::of1(&"de-DE".into()), &o)
}

/// Eine Tagesnummer mit einem Format als Text.
fn text(f: &DateTimeFormat, n: i64) -> String {
    let (j, m, t) = datum(n);
    let d = js_sys::Date::new_with_year_month_day(j.max(0) as u32, m, t);
    f.format().call1(&JsValue::NULL, &d).ok().and_then(|v| v.as_string()).unwrap_or_default()
}

/// "Montag, 13. November 2026"
fn datum_lang(n: i64) -> String {
    text(&format(&[("weekday", "long"), ("day", "numeric"), ("month", "long"), ("year", "numeric")]), n)
}

fn in_tagen(tage: i64) -> String {
    if tage == 0 { "heute".into() } else { format!("in {tage} Tag{}", if tage == 1 { "" } else { "en" }) }
}

// ---- erkennen ------------------------------------------------------------------------------

/// Aus "kalender dezember 2026" Monat und Jahr (fehlt etwas: das heutige).
fn monat_aus(rest: &str) -> Option<Value> {
    let klein = rest.to_lowercase().replacen("maerz", "märz", 1);
    let (hj, hm, _) = datum(heute());
    let mut monat: Option<i32> = None;
    let mut jahr: Option<i32> = None;
    for w in klein.split(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | '/')).filter(|w| !w.is_empty()) {
        let ziffern = w.chars().all(|c| c.is_ascii_digit());
        if ziffern && w.len() == 4 {
            jahr = w.parse().ok();
        } else if ziffern && w.len() <= 2 && (1..=12).contains(&w.parse::<i32>().unwrap_or(0)) {
            monat = w.parse::<i32>().ok().map(|m| m - 1);
        } else {
            let i = [MONATE, MONATE_EN].iter().find_map(|l| l.iter().position(|m| w.chars().count() >= 3 && m.starts_with(w)));
            match i {
                Some(i) => monat = Some(i as i32),
                // "kalender app iphone" ist eine Suche, kein Blatt.
                None if !FUELL.contains(&w) => return None,
                None => {}
            }
        }
    }
    let monat = monat.unwrap_or(if jahr.is_some_and(|j| j != 0) { 0 } else { hm });
    Some(json!({ "art": "monat", "monat": monat, "jahr": jahr.unwrap_or(hj) }))
}

/// Die erste Folge von vier Ziffern in der Frage als Jahr (wie `/\d{4}/`), 0 zaehlt nicht.
fn jahr_in(frage: &str) -> Option<i64> {
    let b = frage.as_bytes();
    (0..b.len().saturating_sub(3))
        .find(|&i| b[i..i + 4].iter().all(u8::is_ascii_digit))
        .and_then(|i| frage[i..i + 4].parse().ok())
        .filter(|&j| j != 0)
}

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    // "kalender …" (auch "calendar", "kalnder"): das Monatsblatt.
    let muster = r"^(?:kalender|kalendar|kalnder|kalener|calendar|calender)(?:\s+(.*))?$";
    if let Some(g) = crate::werkzeug::re_treffer(muster, "iu", ctx.frage.trim()) {
        return monat_aus(g.get(1).cloned().flatten().as_deref().unwrap_or(""));
    }
    Some(json!({ "feiertag": ctx.frage.to_lowercase().contains("feiertag"), "jahr": jahr_in(&ctx.frage) }))
}

// ---- Zeigen --------------------------------------------------------------------------------

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let inhalt = if daten.get("art").and_then(Value::as_str) == Some("monat") {
        let monat = daten.get("monat").and_then(Value::as_i64).unwrap_or(0) as i32;
        let jahr = daten.get("jahr").and_then(Value::as_i64).unwrap_or(1970) as i32;
        rsx! { Blatt { monat, jahr } }
    } else {
        let feiertag = daten.get("feiertag").and_then(Value::as_bool).unwrap_or(false);
        let jahr = daten.get("jahr").and_then(Value::as_i64).map(|j| j as i32);
        if feiertag { rsx! { Feiertage { jahr } } } else { rsx! { Woche {} } }
    };
    rsx! {
        ModulCss { name: "kalender".to_string(), css: CSS }
        {inhalt}
    }
}

/// Die aktuelle Kalenderwoche.
#[component]
fn Woche() -> Element {
    let h = heute();
    let montag = h - (wochentag(h) + 6) % 7;
    let kurz = text(&format(&[("day", "numeric"), ("month", "long")]), montag);
    rsx! {
        Kopf { titel: "Kalenderwoche".to_string() }
        div { class: "w-riesig", "KW {kw(h)}" }
        div { class: "w-text", "{kurz} – {datum_lang(montag + 6)}" }
    }
}

/// Die gesetzlichen Feiertage eines Jahres (ohne Jahr: dazu der naechste).
#[component]
fn Feiertage(jahr: Option<i32>) -> Element {
    let h = heute();
    let j = jahr.unwrap_or(datum(h).0);
    let mut liste = feiertage(j);
    if jahr.is_none() {
        liste.extend(feiertage(j + 1));
    }
    let naechster = liste.iter().find(|(_, d)| *d >= h).filter(|_| jahr.is_none()).map(|(name, d)| format!("Nächster: {name} – {}", in_tagen(d - h)));
    let fmt = format(&[("weekday", "long"), ("day", "numeric"), ("month", "long"), ("year", "numeric")]);
    rsx! {
        Kopf { titel: format!("Gesetzliche Feiertage {j} (bundesweit)") }
        if let Some(n) = naechster {
            div { class: "w-gross", "{n}" }
        }
        table { class: "w-tabelle",
            tbody {
                for (name, d) in feiertage(j) {
                    tr { key: "{name}", class: if d < h { "vorbei" } else { "" },
                        td { "{name}" }
                        td { "{text(&fmt, d)}" }
                    }
                }
            }
        }
    }
}

/// Das Monatsblatt: Montag zuerst, links die Kalenderwoche. Pfeile blaettern, ein Klick auf einen
/// Tag zeigt ihn.
#[component]
fn Blatt(monat: i32, jahr: i32) -> Element {
    let mut z_monat = use_signal(|| monat);
    let mut z_jahr = use_signal(|| jahr);
    let mut gewaehlt = use_signal(|| None::<i64>);
    let (monat, jahr, gew) = (z_monat(), z_jahr(), gewaehlt());

    let h = heute();
    let erster = tag_nr(jahr, monat, 1);
    let start = erster - (wochentag(erster) + 6) % 7;
    let mut frei: BTreeMap<i64, &'static str> = BTreeMap::new();
    for j in jahr - 1..=jahr + 1 {
        for (name, d) in feiertage(j) {
            frei.insert(d, name);
        }
    }
    let mut zeilen: Vec<(i64, Vec<i64>)> = Vec::new();
    for w in 0..6 {
        let montag = start + w * 7;
        if w > 3 && datum(montag).1 != monat {
            break;
        }
        zeilen.push((montag, (0..7).map(|i| montag + i).collect()));
    }
    let im_monat: Vec<(i32, &'static str)> = frei
        .iter()
        .filter_map(|(&d, &name)| {
            let (j, m, t) = datum(d);
            (j == jahr && m == monat).then_some((t, name))
        })
        .collect();
    let titel = text(&format(&[("month", "long"), ("year", "numeric")]), erster);

    // Zu einem Tag: Wochentag, KW, Tag im Jahr, Abstand zu heute, Feiertag.
    let info = gew.map(|d| {
        let tage = d - h;
        let tag_im_jahr = d - tag_nr(datum(d).0, 0, 1) + 1;
        let abstand = match tage {
            0 => "heute".to_string(),
            t if t > 0 => format!("in {t} Tag{}", if t == 1 { "" } else { "en" }),
            t => format!("vor {} Tag{}", -t, if t == -1 { "" } else { "en" }),
        };
        (datum_lang(d), format!(" · KW {} · {tag_im_jahr}. Tag des Jahres · {abstand}", kw(d)), frei.get(&d).copied())
    });

    let mut blaettern = move |schritt: i32| {
        if schritt == 0 {
            let (j, m, _) = datum(heute());
            z_monat.set(m);
            z_jahr.set(j);
            gewaehlt.set(None);
        } else {
            let (j, m, _) = datum(tag_nr(z_jahr(), z_monat() + schritt, 1));
            z_monat.set(m);
            z_jahr.set(j);
        }
    };

    rsx! {
        div { class: "kal-kopf",
            button { r#type: "button", class: "kal-pfeil", "aria-label": "Vorheriger Monat", onclick: move |_| blaettern(-1), "‹" }
            span { class: "kal-monat", "{titel}" }
            button { r#type: "button", class: "kal-pfeil", "aria-label": "Nächster Monat", onclick: move |_| blaettern(1), "›" }
            button { r#type: "button", class: "kal-heute", onclick: move |_| blaettern(0), "Heute" }
        }
        table { class: "kal-blatt",
            thead {
                tr {
                    th { class: "kal-kw", "KW" }
                    for t in WOCHENTAGE {
                        th { key: "{t}", "{t}" }
                    }
                }
            }
            tbody {
                for (montag, tage) in zeilen {
                    tr { key: "{montag}",
                        td { class: "kal-kw", "{kw(montag)}" }
                        for (i, d) in tage.into_iter().enumerate() {
                            {
                                let (dj, dm, dt) = datum(d);
                                let feiertag = frei.get(&d).copied();
                                let klassen = [
                                    (dm != monat).then_some("fremd"),
                                    (i >= 5).then_some("wochenende"),
                                    feiertag.map(|_| "feiertag"),
                                    (d == h).then_some("heute"),
                                    (gew == Some(d)).then_some("gewaehlt"),
                                ]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" ");
                                rsx! {
                                    td { key: "{d}",
                                        button {
                                            r#type: "button",
                                            class: "{klassen}",
                                            title: feiertag,
                                            onclick: move |_| {
                                                gewaehlt.set(Some(d));
                                                if dm != z_monat() {
                                                    z_monat.set(dm);
                                                    z_jahr.set(dj);
                                                }
                                            },
                                            "{dt}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "kal-info",
            if let Some((lang, rest, feiertag)) = info {
                b { "{lang}" }
                "{rest}"
                if let Some(f) = feiertag {
                    " · "
                    span { class: "kal-frei", "{f}" }
                }
            }
        }
        if !im_monat.is_empty() {
            ul { class: "kal-feiertage",
                for (t, name) in im_monat {
                    li { key: "{t}-{name}", span { "{t}." } "{name}" }
                }
            }
        }
    }
}
