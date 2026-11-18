//! Diagramme aus Dieters JSON (Balken, Torte, Linie, Benchmark) als SVG/CSS - Portierung von
//! public/lib/diagramm.js. Die KI liefert nur die Zahlen als JSON (```diagramm {…}```),
//! gezeichnet wird hier:
//!   balken  - Kategorien und Wahlergebnisse, mit "vorher" (z. B. letzte Wahl)
//!   torte   - Anteile an einem Ganzen
//!   linie   - Entwicklung ueber die Zeit, eine oder mehrere Reihen
//!   gruppen - Benchmarks: je Test ein Balken pro Seite ("reihen")
//! Auch Chart.js-JSON ({labels, datasets}) und englische Felder werden verstanden.
//!
//! Feste Schnittstelle (auch fuer /admin): [`Diagramm`] `{ spec }` - optional mit `quellen`
//! (fuer "Quelle: [1]") und `dateien`. Dazu [`json_lesen`] (tolerant), [`zahl_aus`],
//! [`tabellen_diagramm`] (aus einer Markdown-Zahlentabelle) und [`teile`].

use crate::bib::ki::{Quelle, Zeile, Zitat};
use crate::state::Datei;
use crate::werkzeug::zahl;
use dioxus::prelude::*;
use serde_json::{Map, Value, json};

/// Parteifarben (fuer dunklen Hintergrund leicht aufgehellt, wo noetig).
const PARTEIEN: &[(&[&str], &str)] = &[
    (&["\u{1}csu"], "#0080c8"),
    (&["\u{1}cdu", "union"], "#5f6368"),
    (&["\u{1}afd"], "#009ee0"),
    (&["\u{1}spd"], "#e3000f"),
    (&["grüne", "gruene", "bündnis 90"], "#46962b"),
    (&["\u{1}fdp"], "#ffed00"),
    (&["linke"], "#be3075"),
    (&["\u{1}bsw", "wagenknecht"], "#a0326e"),
    (&["freie wähler", "\u{1}fw"], "#f7a800"),
    (&["\u{1}ssw"], "#003c8f"),
    (&["volt"], "#502379"),
    (&["tierschutz"], "#00586b"),
    (&["sonstige", "andere", "other"], "#9aa0a6"),
];
const FARBEN: [&str; 10] = ["#8ab4f8", "#f28b82", "#fdd663", "#81c995", "#ff8bcb", "#c58af9", "#78d9ec", "#fcad70", "#a8dab5", "#e6c9a8"];

// ---- Hilfen ------------------------------------------------------------------------------

/// Ersetzen mit einem Browser-RegExp (wie `text.replace(/muster/flags, ersatz)` in JS).
pub fn ersetzen(text: &str, muster: &str, flags: &str, ersatz: &str) -> String {
    let re = js_sys::RegExp::new(muster, flags);
    String::from(js_sys::JsString::from(text).replace_by_pattern(&re, ersatz))
}

/// Ein Wert als Text wie `String(x)` in JS (null -> "").
pub fn wert_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{f:.0}"),
            Some(f) => format!("{f}"),
            None => n.to_string(),
        },
        andere => andere.to_string(),
    }
}

/// Ist ein Wert "wahr" (wie in JS)?
fn wahr(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// Der erste Schluessel, der nicht null ist (`a ?? b ?? c`).
fn erstes<'a>(o: &'a Map<String, Value>, schluessel: &[&str]) -> Option<&'a Value> {
    schluessel.iter().filter_map(|k| o.get(*k)).find(|v| !v.is_null())
}

/// Der erste Schluessel, der "wahr" ist (`a || b || c`).
fn erstes_wahr<'a>(o: &'a Map<String, Value>, schluessel: &[&str]) -> Option<&'a Value> {
    schluessel.iter().filter_map(|k| o.get(*k)).find(|v| wahr(v))
}

/// `+x` in JS: Zahl, Text als Zahl ("" -> 0), null -> 0, sonst NaN.
fn js_zahl(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) if s.trim().is_empty() => 0.0,
        Value::String(s) => s.trim().parse().unwrap_or(f64::NAN),
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => f64::NAN,
    }
}

/// Der Anfang eines Textes als Zahl (wie `parseFloat`).
fn parse_float(s: &str) -> f64 {
    let mut ende = 0;
    let mut punkt = false;
    let mut ziffern = false;
    for (i, c) in s.char_indices() {
        match c {
            '-' | '+' if i == 0 => {}
            '0'..='9' => ziffern = true,
            '.' if !punkt => punkt = true,
            _ => break,
        }
        ende = i + c.len_utf8();
    }
    if !ziffern {
        return f64::NAN;
    }
    s[..ende].trim_end_matches('.').parse().unwrap_or(f64::NAN)
}

/// "37,1 %", "1.234,5", "4.4 TFLOPS" -> Zahl (sonst NaN).
pub fn zahl_aus(wert: &Value) -> f64 {
    if let Value::Number(n) = wert {
        return n.as_f64().unwrap_or(f64::NAN);
    }
    let text = ersetzen(&wert_text(wert), r"\[\d+\]", "g", "");
    let zeichen: Vec<char> = text.chars().collect();
    let Some(start) = zeichen.iter().position(char::is_ascii_digit) else { return f64::NAN };
    let minus = start > 0 && zeichen[start - 1] == '-';
    let mut m = String::new();
    if minus {
        m.push('-');
    }
    m.extend(zeichen[start..].iter().take_while(|c| c.is_ascii_digit() || **c == '.' || **c == ','));
    if m.contains(',') {
        // Tausenderpunkte weg ("1.234,5"), dann das Komma als Punkt.
        let z: Vec<char> = m.chars().collect();
        let mut ohne = String::new();
        for (i, c) in z.iter().enumerate() {
            let tausender = *c == '.' && z.len() >= i + 4 && z[i + 1..i + 4].iter().all(char::is_ascii_digit) && z.get(i + 4).is_none_or(|n| !n.is_ascii_digit());
            if !tausender {
                ohne.push(*c);
            }
        }
        m = ohne.replacen(',', ".", 1);
    }
    parse_float(&m)
}

/// JSON grosszuegig lesen - Modelle schreiben oft fast-JSON: Kommentare, Komma vor } oder ],
/// Schluessel ohne Anfuehrungszeichen, einfache Anfuehrungszeichen, NaN. Sonst None.
pub fn json_lesen(text: &str) -> Option<Value> {
    let wert = serde_json::from_str::<Value>(text).ok().or_else(|| {
        let roh = ersetzen(text.trim(), r"^[^{\[]*", "", "");
        let roh = ersetzen(&roh, r"[^}\]]*$", "", "");
        let mut r = ersetzen(&roh, r"\/\*[\s\S]*?\*\/", "g", "");
        r = ersetzen(&r, r#"(^|[^:"'\\])\/\/[^\n]*"#, "g", "$1");
        r = ersetzen(&r, r#"'([^'"\\\n]*)'"#, "g", "\"$1\"");
        r = ersetzen(&r, r"([{,]\s*)([A-Za-zÄÖÜäöüß_][\wÄÖÜäöüß-]*)\s*:", "g", "$1\"$2\":");
        r = ersetzen(&r, r"\b(NaN|undefined|Infinity)\b", "g", "null");
        r = ersetzen(&r, r",\s*([}\]])", "g", "$1");
        serde_json::from_str::<Value>(&r).ok()
    })?;
    wahr(&wert).then_some(wert)
}

// ---- Die eigene Form ---------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Eintrag {
    pub name: String,
    pub wert: f64,
    pub vorher: Option<f64>,
    pub werte: Option<Vec<f64>>,
    pub texte: Vec<String>,
    pub farbe: String,
    pub neutral: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reihe {
    pub name: String,
    pub farbe: String,
    pub punkte: Vec<(String, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Koerper {
    Balken(Vec<Eintrag>),
    Torte(Vec<Eintrag>),
    Linie(Vec<Reihe>),
    Gruppen { reihen: Vec<String>, daten: Vec<Eintrag>, zeile: bool, besser: String },
}

/// Ein fertig gelesenes Diagramm.
#[derive(Clone, Debug, PartialEq)]
pub struct Bild {
    pub titel: String,
    pub einheit: String,
    pub quellen: Vec<usize>,
    pub koerper: Koerper,
}

/// Was aus einem JSON gezeichnet wird: Diagramme und Tabellen.
#[derive(Clone, Debug, PartialEq)]
pub enum Teil {
    Bild(Bild),
    Tabelle { kopf: Vec<String>, zeilen: Vec<Vec<String>> },
}

fn eintrag_aus(e: &Value) -> Option<Eintrag> {
    if let Some(a) = e.as_array() {
        return Some(Eintrag { name: wert_text(a.first().unwrap_or(&Value::Null)), wert: zahl_aus(a.get(1).unwrap_or(&Value::Null)), ..Default::default() });
    }
    let o = e.as_object()?;
    Some(Eintrag {
        name: erstes(o, &["name", "label", "x", "partei"]).map(wert_text).unwrap_or_default(),
        wert: zahl_aus(erstes(o, &["wert", "value", "y", "anteil", "prozent"]).unwrap_or(&Value::Null)),
        vorher: erstes(o, &["vorher", "previous", "before"]).map(zahl_aus),
        werte: erstes(o, &["werte", "values"]).and_then(Value::as_array).map(|w| w.iter().map(zahl_aus).collect()),
        texte: o.get("texte").and_then(Value::as_array).map(|t| t.iter().map(wert_text).collect()).unwrap_or_default(),
        farbe: o.get("farbe").and_then(Value::as_str).unwrap_or_default().to_string(),
        neutral: o.get("neutral").is_some_and(wahr),
    })
}

/// Eine Reihe einer Linie, wie das Modell sie schreibt: { name, punkte: [[x, y], …] }.
fn reihe_aus(r: &Value) -> Option<Reihe> {
    let o = r.as_object()?;
    let punkte = o
        .get("punkte")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .filter_map(|p| {
            let y = js_zahl(p.get(1)?);
            y.is_finite().then(|| (wert_text(p.first().unwrap_or(&Value::Null)), y))
        })
        .collect();
    Some(Reihe { name: o.get("name").map(wert_text).unwrap_or_default(), farbe: o.get("farbe").and_then(Value::as_str).unwrap_or_default().to_string(), punkte })
}

/// Ein Diagramm aus einem Objekt - oder None, wenn die Daten nicht passen.
fn ein_diagramm(roh: &Value) -> Option<Bild> {
    let d = roh.as_object()?;
    let titel = erstes(d, &["titel", "title", "name"]).map(wert_text).unwrap_or_default();
    let einheit = erstes(d, &["einheit", "unit"]).map(wert_text).unwrap_or_default();
    let quellen: Vec<usize> = erstes(d, &["quellen", "sources"])
        .and_then(Value::as_array)
        .map(|q| q.iter().map(js_zahl).filter(|n| n.is_finite() && *n >= 1.0 && n.fract() == 0.0).map(|n| n as usize).collect())
        .unwrap_or_default();
    let roh_typ = wert_text(erstes_wahr(d, &["typ", "type"]).unwrap_or(&Value::Null)).to_lowercase();
    let hat = |w: &[&str]| w.iter().any(|x| roh_typ.contains(x));
    let mut typ = if hat(&["line", "lini", "verlauf", "entwick"]) {
        "linie"
    } else if hat(&["pie", "donut", "doughnut", "tort", "kreis", "anteil"]) {
        "torte"
    } else if hat(&["grou", "grup", "vergleich", "multi"]) {
        "gruppen"
    } else {
        "balken"
    };

    let mut daten: Option<Vec<Eintrag>> = None;
    let mut linien: Option<Vec<Reihe>> = None;
    let mut namen: Option<Vec<String>> = None;
    let labels = erstes_wahr(d, &["labels", "kategorien", "x"]).and_then(Value::as_array);
    let sets = erstes_wahr(d, &["datasets", "series"]).and_then(Value::as_array).filter(|s| !s.is_empty());
    let leer = Vec::new();
    let reihe = |s: &Value| -> Vec<Value> { s.as_object().and_then(|o| erstes_wahr(o, &["data", "werte", "values"])).and_then(Value::as_array).cloned().unwrap_or_default() };
    let set_name = |s: &Value| s.as_object().and_then(|o| erstes_wahr(o, &["label", "name"])).map(wert_text).unwrap_or_default();
    if let (Some(labels), Some(sets)) = (labels, sets) {
        let werte: Vec<Vec<Value>> = sets.iter().map(reihe).collect();
        let an = |r: &Vec<Value>, i: usize| zahl_aus(r.get(i).unwrap_or(&Value::Null));
        if typ == "linie" {
            linien = Some(
                sets.iter()
                    .zip(&werte)
                    .map(|(s, r)| Reihe { name: set_name(s), farbe: String::new(), punkte: labels.iter().enumerate().map(|(i, l)| (wert_text(l), an(r, i))).filter(|p| p.1.is_finite()).collect() })
                    .collect(),
            );
        } else if sets.len() > 1 {
            typ = "gruppen";
            namen = Some(sets.iter().map(set_name).collect());
            daten = Some(labels.iter().enumerate().map(|(i, l)| Eintrag { name: wert_text(l), wert: f64::NAN, werte: Some(werte.iter().map(|r| an(r, i)).collect()), ..Default::default() }).collect());
        } else {
            daten = Some(labels.iter().enumerate().map(|(i, l)| Eintrag { name: wert_text(l), wert: an(&werte[0], i), ..Default::default() }).collect());
        }
    } else if let (Some(labels), Some(w)) = (labels, erstes_wahr(d, &["values", "werte"]).and_then(Value::as_array)) {
        daten = Some(labels.iter().enumerate().map(|(i, l)| Eintrag { name: wert_text(l), wert: zahl_aus(w.get(i).unwrap_or(&Value::Null)), ..Default::default() }).collect());
    }
    if daten.is_none() {
        daten = erstes(d, &["daten"]).or_else(|| d.get("data")).and_then(Value::as_array).map(|a| a.iter().filter_map(eintrag_aus).collect());
    }
    let roh_reihen = d.get("reihen").filter(|r| wahr(r)).and_then(Value::as_array).unwrap_or(&leer);
    if typ == "linie" && linien.is_none() {
        linien = if !roh_reihen.is_empty() {
            Some(roh_reihen.iter().filter_map(reihe_aus).collect())
        } else {
            daten.as_ref().map(|ds| vec![Reihe { name: titel.clone(), farbe: String::new(), punkte: ds.iter().filter(|e| e.wert.is_finite()).map(|e| (e.name.clone(), e.wert)).collect() }])
        };
    }
    let namen = namen.unwrap_or_else(|| roh_reihen.iter().map(wert_text).collect());

    let koerper = if typ == "linie" {
        let reihen: Vec<Reihe> = linien.unwrap_or_default().into_iter().filter(|r| r.punkte.len() > 1).take(6).collect();
        if reihen.is_empty() {
            return None;
        }
        Koerper::Linie(reihen)
    } else if typ == "gruppen" && daten.is_some() {
        let reihen: Vec<String> = namen.into_iter().take(4).collect();
        let ds: Vec<Eintrag> = daten.unwrap_or_default().into_iter().filter(|e| e.werte.is_some()).take(12).collect();
        if reihen.len() < 2 || ds.is_empty() {
            return None;
        }
        Koerper::Gruppen { reihen, daten: ds, zeile: d.get("skala").and_then(Value::as_str) == Some("zeile"), besser: d.get("besser").map(wert_text).unwrap_or_default() }
    } else {
        let ds = daten.filter(|d| !d.is_empty())?;
        if typ == "torte" {
            Koerper::Torte(ds.into_iter().filter(|e| e.wert > 0.0).take(12).collect())
        } else {
            Koerper::Balken(ds.into_iter().filter(|e| e.wert.is_finite()).take(16).collect())
        }
    };
    Some(Bild { titel, einheit, quellen, koerper })
}

/// Eine Tabelle als JSON: { kopf: [...], daten: [[...], ...] } (auch "zeilen").
fn tabelle_aus_json(d: &Map<String, Value>) -> Option<Teil> {
    let kopf = erstes_wahr(d, &["kopf", "header", "spalten"])?.as_array()?;
    let zeilen = erstes_wahr(d, &["daten", "zeilen", "rows"])?.as_array()?;
    Some(Teil::Tabelle {
        kopf: kopf.iter().map(wert_text).collect(),
        zeilen: zeilen.iter().filter_map(Value::as_array).map(|z| z.iter().map(|c| if c.is_null() { "–".into() } else { wert_text(c) }).collect()).collect(),
    })
}

/// Was die KI als JSON liefert - ein Diagramm, eine Liste davon oder ein Objekt mit mehreren
/// Diagrammen und Tabellen (so antworten manche Modelle). Passt nichts: leer.
pub fn teile(json: &Value) -> Vec<Teil> {
    match json {
        Value::Array(a) => a.iter().flat_map(teile).collect(),
        Value::Object(x) => {
            let w = |k: &str| x.get(k).is_some_and(wahr);
            if w("kopf") || w("header") || (w("zeilen") && !w("typ")) {
                tabelle_aus_json(x).into_iter().collect()
            } else if w("typ") || w("type") || w("datasets") || (w("labels") && (w("values") || w("werte"))) {
                ein_diagramm(json).map(Teil::Bild).into_iter().collect()
            } else {
                x.values().flat_map(teile).collect()
            }
        }
        _ => Vec::new(),
    }
}

/// Ist das eine Trennzeile einer Markdown-Tabelle (|---|:--|)?
pub fn trennzeile(z: &str) -> bool {
    let innen = z.strip_prefix('|').unwrap_or(z);
    let innen = innen.strip_suffix('|').unwrap_or(innen);
    !innen.is_empty() && innen.chars().all(|c| c.is_whitespace() || matches!(c, ':' | '|' | '-'))
}

/// Die Zellen einer Markdown-Tabellenzeile.
pub fn zellen(z: &str) -> Vec<String> {
    let innen = z.strip_prefix('|').unwrap_or(z);
    let innen = innen.strip_suffix('|').unwrap_or(innen);
    innen.split('|').map(|c| c.trim().to_string()).collect()
}

/// Liefert die KI kein Diagramm, aber eine Tabelle mit Zahlen: selbst eins zeichnen. Eine
/// Zahlenspalte -> Balken (z. B. Wahlergebnis), mehrere Spalten -> je Merkmal ein Balken pro
/// Spalte (z. B. ein Vergleich). Ergebnis: die Spezifikation fuer [`Diagramm`].
pub fn tabellen_diagramm(zeilen: &[String]) -> Option<Value> {
    let alle: Vec<Vec<String>> = zeilen.iter().filter(|z| !trennzeile(z)).map(|z| zellen(z)).collect();
    let (kopf, reihen) = alle.split_first()?;
    if kopf.len() < 2 || reihen.len() < 2 {
        return None;
    }
    let zelle = |r: &Vec<String>, i: usize| r.get(i).cloned().unwrap_or_default();
    let zahl_in = |r: &Vec<String>, i: usize| zahl_aus(&Value::String(zelle(r, i)));
    let sauber = |c: &str| ersetzen(c, r"\[\d+\]", "g", "").replace("**", "").trim().to_string();
    let noetig = (reihen.len() as f64 * 0.6).max(2.0);
    let spalten: Vec<usize> = (1..kopf.len()).filter(|&i| reihen.iter().filter(|r| zahl_in(r, i).is_finite()).count() as f64 >= noetig).collect();
    let spec = match spalten.as_slice() {
        [] => return None,
        [i] => {
            let i = *i;
            let daten: Vec<Value> = reihen.iter().filter(|r| zahl_in(r, i).is_finite()).map(|r| json!({ "name": sauber(&zelle(r, 0)), "wert": zahl_in(r, i) })).collect();
            let einheit = if reihen.iter().any(|r| zelle(r, i).contains('%')) { "%" } else { "" };
            json!({ "typ": "balken", "titel": sauber(&kopf[i]), "einheit": einheit, "daten": daten })
        }
        _ => {
            // Mehrere Seiten: nur Zeilen, in denen alle eine Zahl haben (keine Jahreszahlen).
            let jahr = |w: f64| w.fract() == 0.0 && w > 1900.0 && w < 2100.0;
            let daten: Vec<Value> = reihen
                .iter()
                .filter_map(|r| {
                    let werte: Vec<f64> = spalten.iter().map(|&i| zahl_in(r, i)).collect();
                    let verschieden = werte.iter().any(|w| *w != werte[0]);
                    (werte.iter().all(|w| w.is_finite()) && !werte.iter().all(|w| jahr(*w)) && verschieden)
                        .then(|| json!({ "name": sauber(&zelle(r, 0)), "werte": werte, "texte": spalten.iter().map(|&i| sauber(&zelle(r, i))).collect::<Vec<_>>() }))
                })
                .take(10)
                .collect();
            if daten.len() < 2 {
                return None;
            }
            json!({ "typ": "gruppen", "titel": "Auf einen Blick", "skala": "zeile", "reihen": spalten.iter().map(|&i| sauber(&kopf[i])).collect::<Vec<_>>(), "daten": daten })
        }
    };
    ein_diagramm(&spec).map(|_| spec)
}

// ---- Zeichnen ----------------------------------------------------------------------------

fn farbe(eigene: &str, name: &str, i: usize) -> String {
    let hex = eigene.len() >= 4 && eigene.len() <= 9 && eigene.starts_with('#') && eigene[1..].chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        return eigene.to_string();
    }
    let n = name.to_lowercase();
    // "\u{1}wort" = ganzes Wort (\b in JS), sonst Teilstueck.
    let passt = |m: &str| match m.strip_prefix('\u{1}') {
        Some(w) => n.match_indices(w).any(|(p, _)| {
            let vor = n[..p].chars().next_back().is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
            let nach = n[p + w.len()..].chars().next().is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
            vor && nach
        }),
        None => n.contains(m),
    };
    PARTEIEN.iter().find(|(m, _)| m.iter().any(|x| passt(x))).map_or_else(|| FARBEN[i % FARBEN.len()].to_string(), |(_, f)| (*f).to_string())
}

/// Wie viele Nachkommastellen? So viele, wie die Daten haben (hoechstens 2).
fn stellen(werte: impl IntoIterator<Item = f64>) -> usize {
    werte.into_iter().map(|w| format!("{w}").split_once('.').map_or(0, |(_, r)| r.len())).max().unwrap_or(0).min(2)
}

fn zahl_mit(w: f64, s: usize, einheit: &str) -> String {
    match einheit {
        "" => zahl(w, s),
        "%" => format!("{} %", zahl(w, s)),
        e => format!("{} {e}", zahl(w, s)),
    }
}

/// Ein Diagramm aus einer Spezifikation (Format wie in lib/diagramm.js `diagrammHtml(json)`);
/// `spec` darf auch JSON-Text sein. Mit `quellen` werden "Quelle: [1]" und Tabellenzellen mit
/// Zitaten als Chips gezeichnet.
#[component]
pub fn Diagramm(spec: Value, #[props(default)] quellen: Vec<Quelle>, #[props(default)] dateien: Vec<Datei>) -> Element {
    let spec = match &spec {
        Value::String(s) => json_lesen(s).unwrap_or(Value::Null),
        _ => spec,
    };
    rsx! {
        for (i, teil) in teile(&spec).into_iter().enumerate() {
            match teil {
                Teil::Bild(bild) => rsx! { BildAnsicht { key: "{i}", bild, quellen: quellen.clone() } },
                Teil::Tabelle { kopf, zeilen } => rsx! {
                    div { key: "{i}", class: "ki-tabelle",
                        table {
                            thead { tr { for k in kopf { th { Zeile { text: k, quellen: quellen.clone(), dateien: dateien.clone() } } } } }
                            tbody {
                                for z in zeilen {
                                    tr { for c in z { td { Zeile { text: c, quellen: quellen.clone(), dateien: dateien.clone() } } } }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn BildAnsicht(bild: Bild, quellen: Vec<Quelle>) -> Element {
    let Bild { titel, einheit, quellen: nummern, koerper } = bild;
    let mit_quelle = nummern.iter().any(|n| *n >= 1 && *n <= quellen.len());
    let inhalt = match koerper {
        Koerper::Balken(d) => balken(&d, &einheit),
        Koerper::Torte(d) => torte(&d, &einheit),
        Koerper::Linie(r) => linie(&r, &einheit, &titel),
        Koerper::Gruppen { reihen, daten, zeile, besser } => gruppen(&reihen, &daten, zeile, &besser, &einheit),
    };
    rsx! {
        figure { class: "ki-diagramm",
            if !titel.is_empty() {
                figcaption { "{titel}" }
            }
            {inhalt}
            if mit_quelle {
                div { class: "dg-quelle", "Quelle:" Zitat { nummern, quellen } }
            }
        }
    }
}

fn balken(daten: &[Eintrag], einheit: &str) -> Element {
    let s = stellen(daten.iter().flat_map(|e| [e.wert, e.vorher.unwrap_or(0.0)]));
    let max = daten.iter().flat_map(|e| [e.wert, e.vorher.unwrap_or(0.0)]).fold(0.0f64, f64::max);
    let max = if max == 0.0 { 1.0 } else { max };
    let quer = daten.len() > 8 || daten.iter().any(|e| e.name.chars().count() > 14);
    let mit_vorher = daten.iter().any(|e| e.vorher.is_some_and(f64::is_finite));
    let klasse = if quer { "dg-balken quer" } else { "dg-balken hoch" };
    rsx! {
        div { class: "{klasse}", style: "--n:{daten.len()}",
            for (i, e) in daten.iter().enumerate() {
                {
                    let vorher = e.vorher.filter(|v| mit_vorher && v.is_finite());
                    let delta = vorher.map(|v| e.wert - v);
                    let wert = zahl_mit(e.wert, s, einheit);
                    let titel = match vorher {
                        Some(v) => format!("{}: {wert} (vorher {})", e.name, zahl_mit(v, s, einheit)),
                        None => format!("{}: {wert}", e.name),
                    };
                    let bar = format!("--a:{}%;--f:{}", e.wert / max * 100.0, farbe(&e.farbe, &e.name, i));
                    let alt = format!("--a:{}%", e.vorher.filter(|v| v.is_finite()).unwrap_or(0.0) / max * 100.0);
                    rsx! {
                        div { key: "{i}", class: "dg-eintrag", title: "{titel}",
                            span { class: "dg-wert",
                                "{wert}"
                                if let Some(d) = delta {
                                    small { class: if d >= 0.0 { "plus" } else { "minus" },
                                        if d >= 0.0 { "+" } else { "−" }
                                        "{zahl(d.abs(), s)}"
                                    }
                                }
                            }
                            span { class: "dg-spur",
                                span { class: "dg-bar", style: "{bar}" }
                                if mit_vorher {
                                    span { class: "dg-vorher", style: "{alt}" }
                                }
                            }
                            span { class: "dg-name", "{e.name}" }
                        }
                    }
                }
            }
        }
        if mit_vorher {
            div { class: "dg-legende",
                span { i { class: "dg-punkt" } "Jetzt" }
                span { i { class: "dg-punkt grau" } "Vorher" }
            }
        }
    }
}

fn torte(daten: &[Eintrag], einheit: &str) -> Element {
    let summe: f64 = daten.iter().map(|e| e.wert).sum();
    let summe = if summe == 0.0 { 1.0 } else { summe };
    let s = stellen(daten.iter().map(|e| e.wert));
    let mut bisher = 0.0;
    let stuecke: Vec<String> = daten
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let von = bisher;
            bisher += e.wert / summe * 100.0;
            format!("{} {von}% {bisher}%", farbe(&e.farbe, &e.name, i))
        })
        .collect();
    let ring = format!("background:conic-gradient({})", stuecke.join(","));
    rsx! {
        div { class: "dg-torte",
            div { class: "dg-ring", style: "{ring}" }
            ul { class: "dg-liste",
                for (i, e) in daten.iter().enumerate() {
                    li { key: "{i}",
                        i { class: "dg-punkt", style: "background:{farbe(&e.farbe, &e.name, i)}" }
                        "{e.name}"
                        b { "{zahl_mit(e.wert, s, einheit)}" }
                    }
                }
            }
        }
    }
}

fn linie(reihen: &[Reihe], einheit: &str, titel: &str) -> Element {
    let mut xs: Vec<String> = Vec::new();
    for p in reihen.iter().flat_map(|r| &r.punkte) {
        if !xs.contains(&p.0) {
            xs.push(p.0.clone());
        }
    }
    let werte: Vec<f64> = reihen.iter().flat_map(|r| r.punkte.iter().map(|p| p.1)).collect();
    let mut min = werte.iter().copied().fold(f64::INFINITY, f64::min);
    let mut max = werte.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if min > 0.0 && min / (if max == 0.0 { 1.0 } else { max }) < 0.5 {
        min = 0.0;
    }
    if max == min {
        max = min + 1.0;
    }
    let (b, h, l, r, o, u) = (640.0, 220.0, 48.0, 12.0, 12.0, 26.0);
    let teiler = (xs.len().max(2) - 1) as f64;
    let x = |k: &str| l + xs.iter().position(|v| v == k).unwrap_or(0) as f64 / teiler * (b - l - r);
    let y = |w: f64| o + (1.0 - (w - min) / (max - min)) * (h - o - u);
    let s = stellen(werte.iter().copied());
    let gitter = [min, min + 0.5 * (max - min), max];
    let schritt = xs.len().div_ceil(6).max(1);
    let beschriftet: Vec<&String> = xs.iter().enumerate().filter(|(i, _)| xs.len() <= 8 || i % schritt == 0 || *i == xs.len() - 1).map(|(_, k)| k).collect();
    let name = if titel.is_empty() { "Diagramm" } else { titel };
    let farben: Vec<String> = reihen.iter().enumerate().map(|(i, r)| farbe(&r.farbe, &r.name, i)).collect();
    rsx! {
        svg { class: "dg-linie", view_box: "0 0 {b} {h}", role: "img", "aria-label": "{name}",
            for (i, w) in gitter.iter().enumerate() {
                line { key: "g{i}", x1: "{l}", x2: "{b - r}", y1: "{y(*w)}", y2: "{y(*w)}", class: "dg-gitter" }
                text { x: "{l - 6.0}", y: "{y(*w) + 4.0}", text_anchor: "end", "{zahl(*w, s)}" }
            }
            for k in beschriftet {
                text { key: "x{k}", x: "{x(k)}", y: "{h - 6.0}", text_anchor: "middle", "{k}" }
            }
            for (i, rh) in reihen.iter().enumerate() {
                polyline {
                    key: "l{i}",
                    fill: "none",
                    stroke: "{farben[i]}",
                    stroke_width: "2.5",
                    stroke_linejoin: "round",
                    points: rh.punkte.iter().map(|p| format!("{},{}", x(&p.0), y(p.1))).collect::<Vec<_>>().join(" "),
                }
                for (j, p) in rh.punkte.iter().enumerate() {
                    circle { key: "c{i}-{j}", cx: "{x(&p.0)}", cy: "{y(p.1)}", r: "3.5", fill: "{farben[i]}",
                        "data-titel": "{rh.name} {p.0}: {zahl_mit(p.1, s, einheit)}",
                    }
                }
            }
        }
        if reihen.len() > 1 {
            div { class: "dg-legende",
                for (i, rh) in reihen.iter().enumerate() {
                    span { key: "{i}", i { class: "dg-punkt", style: "background:{farben[i]}" } "{rh.name}" }
                }
            }
        }
    }
}

/// Benchmarks: je Test (z. B. ein Spiel) ein Balken pro Seite, quer, mit Wert.
/// "besser": "weniger" faerbt den kleineren Wert als Sieger (Verbrauch, Preis).
fn gruppen(reihen: &[String], daten: &[Eintrag], zeile: bool, besser: &str, einheit: &str) -> Element {
    let alle: Vec<f64> = daten.iter().flat_map(|e| e.werte.iter().flatten().copied().filter(|w| w.is_finite())).collect();
    let max_von = |w: &[f64]| {
        let m = w.iter().copied().filter(|x| x.is_finite()).fold(0.0f64, f64::max);
        if m == 0.0 { 1.0 } else { m }
    };
    let max = max_von(&alle);
    let s = stellen(alle.iter().copied());
    let b = besser.to_lowercase();
    let weniger = b.starts_with("weniger") || b.starts_with("lower") || b.starts_with("niedrig");
    rsx! {
        div { class: "dg-gruppen",
            for (n, e) in daten.iter().enumerate() {
                {
                    let werte: Vec<f64> = (0..reihen.len()).map(|i| e.werte.as_ref().and_then(|w| w.get(i)).copied().unwrap_or(f64::NAN)).collect();
                    let gut: Vec<f64> = werte.iter().copied().filter(|w| w.is_finite()).collect();
                    let bester = (gut.len() > 1).then(|| if weniger { gut.iter().copied().fold(f64::INFINITY, f64::min) } else { gut.iter().copied().fold(f64::NEG_INFINITY, f64::max) });
                    let m = if zeile { max_von(&werte) } else { max };
                    rsx! {
                        div { key: "{n}", class: "dg-gruppe",
                            div { class: "dg-gruppe-name", "{e.name}" }
                            for (i, r) in reihen.iter().enumerate() {
                                {
                                    let w = werte[i];
                                    let anzeige = if !w.is_finite() {
                                        "–".to_string()
                                    } else {
                                        e.texte.get(i).filter(|t| !t.is_empty()).cloned().unwrap_or_else(|| zahl_mit(w, s, einheit))
                                    };
                                    let sieger = Some(w) == bester && !e.neutral;
                                    let a = if w.is_finite() { w / m * 100.0 } else { 0.0 };
                                    rsx! {
                                        div { key: "{i}", class: if sieger { "dg-zeile sieger" } else { "dg-zeile" }, title: "{r}: {anzeige}",
                                            span { class: "dg-spur", span { class: "dg-bar", style: "--a:{a}%;--f:{FARBEN[i]}" } }
                                            span { class: "dg-wert", "{anzeige}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "dg-legende",
            for (i, r) in reihen.iter().enumerate() {
                span { key: "{i}", i { class: "dg-punkt", style: "background:{FARBEN[i]}" } "{r}" }
            }
            if !besser.is_empty() {
                span { class: "dg-besser", if weniger { "weniger ist besser" } else { "mehr ist besser" } }
            }
        }
    }
}
