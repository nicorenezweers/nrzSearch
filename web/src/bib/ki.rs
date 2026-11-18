//! Die KI im Browser - Portierung von public/lib/ki.js: der Strom von /api/ki und die Antwort
//! als Dioxus-Komponenten (Absaetze, Ueberschriften, Listen, **fett**, *kursiv*, Links,
//! Markdown-Tabellen, Code-/Textkaesten zum Kopieren, Diagramme) mit Quellen-Chips.
//! Gebraucht von der "Uebersicht mit KI" (module/ki.rs), dem Vergleich und dem KI-Modus.
//!
//! Schnittstelle:
//! - [`abbrechen`] - alles abbrechen, was die KI gerade schreibt (neue Suche, Klick auf Treffer).
//! - [`fragen`] - POST /api/ki; jedes Ereignis (suche, quellen, material, text, fehler, fertig)
//!   geht an `an(name, daten)`.
//! - [`KiAntwort`] `{ text, quellen, dateien }` - die Antwort; [`QuellenZeile`] `{ quellen }` -
//!   "7 Quellen" mit Logos und Liste beim Drueberfahren; [`Zeile`], [`Zitat`].
//! - [`bloecke`] - die Antwort in Bloecke zerlegt (leer = nichts zu zeigen).

use crate::api::{self, Abbruch, Ergebnis};
use crate::bib::dateien::datei_art;
use crate::bib::diagramm::{Diagramm, json_lesen, tabellen_diagramm, teile, wert_text};
use crate::i18n::t;
use crate::state::Datei;
use crate::werkzeug::{FaviconKreis, KopierKnopf, quelle_name};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;

/// Eine Quelle, wie der Server sie im Ereignis "quellen" schickt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Quelle {
    #[serde(skip_serializing_if = "Value::is_null")]
    pub nr: Value,
    pub titel: String,
    pub url: String,
    pub host: String,
}

impl Quelle {
    /// Die Quellen aus einem JSON-Feld (fehlerhafte Eintraege fallen weg).
    pub fn liste(v: &Value) -> Vec<Quelle> {
        v.as_array().into_iter().flatten().filter_map(|q| serde_json::from_value(q.clone()).ok()).collect()
    }
}

// ---- Der Strom ---------------------------------------------------------------------------

thread_local! {
    /// Alle laufenden Anfragen - [`abbrechen`] beendet sie.
    static LAUFEND: RefCell<Vec<Abbruch>> = const { RefCell::new(Vec::new()) };
}

/// Alles abbrechen, was die KI gerade schreibt (neue Suche, Klick auf einen Treffer). Der Server
/// merkt das und hoert auch auf.
pub fn abbrechen() {
    let alle: Vec<Abbruch> = LAUFEND.with(|l| l.borrow_mut().drain(..).collect());
    for a in alle {
        a.abbrechen();
    }
}

/// Meldet eine laufende Anfrage ab, auch wenn der Future vorher fallen gelassen wird.
struct Abmelden(Abbruch);

impl Drop for Abmelden {
    fn drop(&mut self) {
        let a = self.0.clone();
        LAUFEND.with(|l| l.borrow_mut().retain(|x| *x != a));
    }
}

/// POST /api/ki - jedes Ereignis (suche, quellen, material, text, fehler, fertig) geht an
/// `an(name, daten)`. Endet, wenn der Server fertig ist oder abgebrochen wurde.
pub async fn fragen(anfrage: &Value, abbruch: &Abbruch, an: impl FnMut(&str, Value)) -> Ergebnis<()> {
    LAUFEND.with(|l| l.borrow_mut().push(abbruch.clone()));
    let _abmelden = Abmelden(abbruch.clone());
    api::post_strom("/api/ki", anfrage, abbruch, an).await
}

/// Ist die KI eingerichtet (OpenRouter-Schluessel)? Einmal je Seite gefragt.
pub async fn bereit() -> bool {
    thread_local! { static STATUS: RefCell<Option<bool>> = const { RefCell::new(None) }; }
    if let Some(s) = STATUS.with(|s| *s.borrow()) {
        return s;
    }
    let s = api::get_oder_nichts("/api/ki/status", &[]).await.and_then(|v| v.get("bereit").and_then(Value::as_bool)).unwrap_or(false);
    STATUS.with(|x| *x.borrow_mut() = Some(s));
    s
}

// ---- Zerlegen ----------------------------------------------------------------------------

/// Ein Stueck einer Zeile.
#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Fett(Vec<Inline>),
    Kursiv(Vec<Inline>),
    Link { text: Vec<Inline>, url: String },
    /// [1][3] - die Nummern der Quellen.
    Zitat(Vec<usize>),
    /// [D2] - die Nummer der Datei im Arbeitsbereich.
    Datei(usize),
}

fn wortzeichen(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Ziffern ab `i` lesen: (Zahl, Ende).
fn ziffern(z: &[char], i: usize) -> Option<(usize, usize)> {
    let ende = z[i..].iter().position(|c| !c.is_ascii_digit()).map_or(z.len(), |p| i + p);
    if ende == i {
        return None;
    }
    z[i..ende].iter().collect::<String>().parse().ok().map(|n| (n, ende))
}

fn leer_ab(z: &[char], mut i: usize) -> usize {
    while i < z.len() && z[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Eine Gruppe "[1, 2]" ab `i`: (Nummern, Ende).
fn zitat_gruppe(z: &[char], i: usize) -> Option<(Vec<usize>, usize)> {
    if z.get(i) != Some(&'[') {
        return None;
    }
    let mut nummern = Vec::new();
    let (n, mut j) = ziffern(z, i + 1)?;
    nummern.push(n);
    loop {
        let k = leer_ab(z, j);
        match z.get(k) {
            Some(']') => return Some((nummern, k + 1)),
            Some(',') => {
                let (n, e) = ziffern(z, leer_ab(z, k + 1))?;
                nummern.push(n);
                j = e;
            }
            _ => return None,
        }
    }
}

/// Ein Link "[Text](https://…)" ab `i`: (Text, Adresse, Ende).
fn link_ab(z: &[char], i: usize) -> Option<(String, String, usize)> {
    let zu = i + 1 + z[i + 1..].iter().position(|c| *c == ']')?;
    if zu == i + 1 || z.get(zu + 1) != Some(&'(') {
        return None;
    }
    let start = zu + 2;
    let ende = start + z[start..].iter().position(|c| *c == ')' || c.is_whitespace())?;
    if z[ende] != ')' {
        return None;
    }
    let url: String = z[start..ende].iter().collect();
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    if rest.is_empty() {
        return None;
    }
    Some((z[i + 1..zu].iter().collect(), url, ende + 1))
}

/// Eine Zeile Markdown in Stuecke zerlegen: [D2], [Text](Link), **fett**, *kursiv*, [1][3].
pub fn zeile_teile(text: &str) -> Vec<Inline> {
    let z: Vec<char> = text.chars().collect();
    let mut aus: Vec<Inline> = Vec::new();
    let mut puffer = String::new();
    let mut vorher: Option<char> = None;
    let mut i = 0;
    let flush = |puffer: &mut String, aus: &mut Vec<Inline>, kuerzen: bool| {
        if kuerzen {
            let l = puffer.trim_end().len();
            puffer.truncate(l);
        }
        if !puffer.is_empty() {
            aus.push(Inline::Text(std::mem::take(puffer)));
        }
    };
    while i < z.len() {
        let c = z[i];
        if c == '[' {
            // [D2]
            if z.get(i + 1) == Some(&'D') {
                if let Some((n, e)) = ziffern(&z, i + 2).filter(|(_, e)| z.get(*e) == Some(&']')) {
                    flush(&mut puffer, &mut aus, true);
                    aus.push(Inline::Datei(n));
                    i = e + 1;
                    vorher = Some('>');
                    continue;
                }
            }
            // [1][3], [1, 2] - mehrere Gruppen hintereinander sind ein Chip.
            if let Some((mut nummern, mut e)) = zitat_gruppe(&z, i) {
                e = leer_ab(&z, e);
                while let Some((n, e2)) = zitat_gruppe(&z, e) {
                    nummern.extend(n);
                    e = leer_ab(&z, e2);
                }
                flush(&mut puffer, &mut aus, true);
                aus.push(Inline::Zitat(nummern));
                i = e;
                vorher = Some('>');
                continue;
            }
            if let Some((text, url, e)) = link_ab(&z, i) {
                flush(&mut puffer, &mut aus, false);
                aus.push(Inline::Link { text: zeile_teile(&text), url });
                i = e;
                vorher = Some('>');
                continue;
            }
        }
        if c == '*' && z.get(i + 1) == Some(&'*') {
            // **fett** (mindestens ein Zeichen dazwischen)
            let zu = (i + 3..z.len().saturating_sub(1)).find(|&k| z[k] == '*' && z[k + 1] == '*');
            if let Some(zu) = zu {
                flush(&mut puffer, &mut aus, false);
                aus.push(Inline::Fett(zeile_teile(&z[i + 2..zu].iter().collect::<String>())));
                i = zu + 2;
                vorher = Some('>');
                continue;
            }
        } else if c == '*' {
            // *kursiv* - nicht mitten im Wort, nicht "* " und nicht "a*b".
            let davor_ok = vorher.is_none_or(|v| v != '*' && !wortzeichen(v));
            let erstes_ok = z.get(i + 1).is_some_and(|n| *n != '*' && !n.is_whitespace());
            if davor_ok && erstes_ok {
                if let Some(zu) = (i + 2..z.len()).find(|&k| z[k] == '*') {
                    if z.get(zu + 1).is_none_or(|n| !wortzeichen(*n)) {
                        flush(&mut puffer, &mut aus, false);
                        aus.push(Inline::Kursiv(zeile_teile(&z[i + 1..zu].iter().collect::<String>())));
                        i = zu + 1;
                        vorher = Some('>');
                        continue;
                    }
                }
            }
        }
        puffer.push(c);
        vorher = Some(c);
        i += 1;
    }
    flush(&mut puffer, &mut aus, false);
    aus
}

/// Ein Block der Antwort.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Absatz(String),
    Titel(String),
    Liste { geordnet: bool, punkte: Vec<String> },
    /// Eine Markdown-Tabelle (Zeilen roh) - davor optional ein selbst gezeichnetes Diagramm.
    Tabelle { zeilen: Vec<String>, bild: Option<Value> },
    /// Ein Kasten zum Kopieren (E-Mail, Brief, Code).
    Kasten { sprache: String, inhalt: String },
    Diagramm(Value),
    /// Ein Diagramm-Block, der noch nicht zu ist (die Antwort kommt gerade).
    DiagrammLaedt,
}

/// Sieht der Text nach Diagramm-Daten aus? (`"?(typ|type|kopf|labels)"?\s*:`)
pub fn daten_muster(s: &str) -> bool {
    ["typ", "type", "kopf", "labels"].iter().any(|w| {
        s.match_indices(w).any(|(p, _)| {
            let rest = &s[p + w.len()..];
            let rest = rest.strip_prefix('"').unwrap_or(rest);
            rest.trim_start().starts_with(':')
        })
    })
}

/// Alle ```-Bloecke: (Sprache, Inhalt) - wie /```(\w*)\n([\s\S]*?)```/g.
fn code_bloecke(text: &str) -> Vec<(String, String)> {
    let mut aus = Vec::new();
    let mut pos = 0;
    while let Some(p) = text[pos..].find("```") {
        let start = pos + p + 3;
        let sprache: String = text[start..].chars().take_while(|c| wortzeichen(*c)).collect();
        let nach = start + sprache.len();
        if text[nach..].starts_with('\n') {
            if let Some(e) = text[nach + 1..].find("```") {
                aus.push((sprache, text[nach + 1..nach + 1 + e].to_string()));
                pos = nach + 1 + e + 3;
                continue;
            }
            break;
        }
        pos = pos + p + 1;
    }
    aus
}

/// "- Punkt", "* Punkt", "• Punkt", "1. Punkt", "2) Punkt" -> (geordnet, Text).
fn punkt(z: &str) -> Option<(bool, &str)> {
    let (geordnet, rest) = if let Some(r) = z.strip_prefix(['-', '*', '•']) {
        (false, r)
    } else {
        let n = z.find(|c: char| !c.is_ascii_digit()).unwrap_or(z.len());
        if n == 0 {
            return None;
        }
        (true, z[n..].strip_prefix(['.', ')'])?)
    };
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let text = rest.trim_start();
    (!text.is_empty()).then_some((geordnet, text))
}

fn trennlinie(z: &str) -> bool {
    z.len() >= 3 && ['-', '*', '_'].iter().any(|c| z.chars().all(|x| x == *c))
}

/// Die Antwort in Bloecke zerlegen (wie `kiHtml`). Leer = nichts zu zeigen.
pub fn bloecke(text: &str) -> Vec<Block> {
    let roh = text.trim();
    let text = if roh.starts_with('{') && daten_muster(roh) && json_lesen(roh).is_some() { format!("```diagramm\n{roh}\n```") } else { text.to_string() };
    let mut aus: Vec<Block> = Vec::new();
    let mut liste: Option<(bool, Vec<String>)> = None;
    let mut tabelle: Vec<String> = Vec::new();
    let mut block: Option<(String, Vec<String>)> = None;
    // Ohne lesbares Diagramm der KI: aus der ersten Zahlentabelle eins zeichnen.
    let mut selbst = !code_bloecke(&text).iter().any(|(s, inhalt)| (s == "diagramm" || daten_muster(inhalt)) && json_lesen(inhalt).is_some());

    fn liste_zu(liste: &mut Option<(bool, Vec<String>)>, aus: &mut Vec<Block>) {
        if let Some((geordnet, punkte)) = liste.take() {
            aus.push(Block::Liste { geordnet, punkte });
        }
    }
    let tabelle_zu = |tabelle: &mut Vec<String>, aus: &mut Vec<Block>, selbst: &mut bool| {
        if tabelle.is_empty() {
            return;
        }
        let bild = if *selbst { tabellen_diagramm(tabelle) } else { None };
        if bild.is_some() {
            *selbst = false;
        }
        aus.push(Block::Tabelle { zeilen: std::mem::take(tabelle), bild });
    };
    // ```diagramm (oder JSON, das wie Diagramme aussieht) wird gezeichnet, anderes kommt in
    // einen Kasten zum Kopieren.
    let block_zu = |(sprache, zeilen): (String, Vec<String>), fertig: bool| -> Option<Block> {
        let inhalt = zeilen.join("\n");
        let daten = sprache == "diagramm" || ((sprache == "json" || sprache.is_empty()) && daten_muster(&inhalt));
        if !daten {
            return Some(Block::Kasten { sprache, inhalt });
        }
        if !fertig {
            return Some(Block::DiagrammLaedt);
        }
        match json_lesen(&inhalt).filter(|v| !teile(v).is_empty()) {
            Some(v) => Some(Block::Diagramm(v)),
            None if sprache == "diagramm" => None,
            None => Some(Block::Kasten { sprache, inhalt }),
        }
    };

    for roh in text.split('\n') {
        let z = roh.trim();
        if let Some(rest) = z.strip_prefix("```") {
            if let Some(b) = block.take() {
                aus.extend(block_zu(b, true));
            } else {
                tabelle_zu(&mut tabelle, &mut aus, &mut selbst);
                liste_zu(&mut liste, &mut aus);
                block = Some((rest.trim().to_lowercase(), Vec::new()));
            }
            continue;
        }
        if let Some((_, zeilen)) = block.as_mut() {
            zeilen.push(roh.to_string());
            continue;
        }
        if z.starts_with('|') {
            liste_zu(&mut liste, &mut aus);
            tabelle.push(z.to_string());
            continue;
        }
        tabelle_zu(&mut tabelle, &mut aus, &mut selbst);
        if let Some((geordnet, text)) = punkt(z) {
            liste.get_or_insert_with(|| (geordnet, Vec::new())).1.push(text.to_string());
            continue;
        }
        liste_zu(&mut liste, &mut aus);
        if z.is_empty() || trennlinie(z) {
            continue;
        }
        let titel = z.find(|c: char| c != '#').filter(|&n| (1..=6).contains(&n) && z[n..].starts_with(char::is_whitespace));
        match titel {
            Some(n) => aus.push(Block::Titel(z[n..].trim().to_string())),
            None => aus.push(Block::Absatz(z.to_string())),
        }
    }
    tabelle_zu(&mut tabelle, &mut aus, &mut selbst);
    liste_zu(&mut liste, &mut aus);
    // Noch offen (die Antwort kommt gerade): ein Diagramm wird erst fertig gezeichnet.
    if let Some(b) = block {
        aus.extend(block_zu(b, false));
    }
    aus
}

// ---- Zeichnen ----------------------------------------------------------------------------

/// [1][3] wird zu einem Chip "minecraft.wiki +1" mit Link zur ersten Quelle.
#[component]
pub fn Zitat(nummern: Vec<usize>, quellen: Vec<Quelle>) -> Element {
    let mut gesehen: Vec<usize> = Vec::new();
    for n in nummern {
        if !gesehen.contains(&n) {
            gesehen.push(n);
        }
    }
    let qs: Vec<&Quelle> = gesehen.iter().filter_map(|n| n.checked_sub(1).and_then(|i| quellen.get(i))).collect();
    let Some(erste) = qs.first() else { return rsx! {} };
    let titel = qs.iter().map(|q| q.titel.as_str()).collect::<Vec<_>>().join("\n");
    let mehr = qs.len() - 1;
    rsx! {
        " "
        a { class: "ki-quelle", href: "{erste.url}", target: "_blank", rel: "noreferrer", title: "{titel}",
            FaviconKreis { host: erste.host.clone() }
            span {
                "{quelle_name(&erste.host)}"
                if mehr > 0 { " +{mehr}" }
            }
        }
    }
}

fn stuecke(teile: &[Inline], quellen: &[Quelle], dateien: &[Datei]) -> Element {
    rsx! {
        for (i, s) in teile.iter().enumerate() {
            match s {
                Inline::Text(t) => rsx! { span { key: "{i}", "{t}" } },
                Inline::Fett(innen) => rsx! { b { key: "{i}", {stuecke(innen, quellen, dateien)} } },
                Inline::Kursiv(innen) => rsx! { i { key: "{i}", {stuecke(innen, quellen, dateien)} } },
                Inline::Link { text, url } => rsx! {
                    a { key: "{i}", href: "{url}", target: "_blank", rel: "noreferrer", {stuecke(text, quellen, dateien)} }
                },
                Inline::Zitat(n) => rsx! { Zitat { key: "{i}", nummern: n.clone(), quellen: quellen.to_vec() } },
                Inline::Datei(n) => match n.checked_sub(1).and_then(|x| dateien.get(x)) {
                    // [D2] wird zum Chip mit dem Namen der Datei aus dem Arbeitsbereich.
                    Some(d) => rsx! {
                        " "
                        span { class: "ki-quelle ki-datei-quelle", title: "{d.name}",
                            span { class: "datei-art", "{datei_art(&d.name, &d.typ)}" }
                            span { "{d.name}" }
                        }
                    },
                    None => rsx! {},
                },
            }
        }
    }
}

/// Eine Zeile Markdown (fett, kursiv, Links, Zitate als Chips).
#[component]
pub fn Zeile(text: String, #[props(default)] quellen: Vec<Quelle>, #[props(default)] dateien: Vec<Datei>) -> Element {
    let teile = zeile_teile(&text);
    stuecke(&teile, &quellen, &dateien)
}

/// Ein Textkasten zum Kopieren - fuer E-Mails, Briefe und Code.
#[component]
fn Kasten(sprache: String, inhalt: String) -> Element {
    let name = match sprache.as_str() {
        "" | "text" => "Text".to_string(),
        "email" | "mail" => "E-Mail".to_string(),
        "brief" => "Brief".to_string(),
        s => s.to_string(),
    };
    rsx! {
        div { class: "ki-kasten",
            div { class: "ki-kasten-kopf",
                span { "{name}" }
                KopierKnopf { text: inhalt.clone(), class: "ki-kasten-kopieren" }
            }
            pre { "{inhalt}" }
        }
    }
}

/// Eine Markdown-Tabelle: erste Zeile Kopf, die Trennzeile (|---|) faellt weg.
#[component]
fn Tabelle(zeilen: Vec<String>, quellen: Vec<Quelle>, dateien: Vec<Datei>) -> Element {
    use crate::bib::diagramm::{trennzeile, zellen};
    let alle: Vec<Vec<String>> = zeilen.iter().filter(|z| !trennzeile(z)).map(|z| zellen(z)).collect();
    let Some((kopf, rest)) = alle.split_first() else { return rsx! {} };
    rsx! {
        div { class: "ki-tabelle",
            table {
                thead { tr { for k in kopf.iter() { th { {stuecke(&zeile_teile(k), &quellen, &dateien)} } } } }
                tbody {
                    for z in rest.iter() {
                        tr { for c in z.iter() { td { {stuecke(&zeile_teile(c), &quellen, &dateien)} } } }
                    }
                }
            }
        }
    }
}

/// Die Bloecke einer Antwort zeichnen.
pub fn bloecke_zeigen(bloecke: &[Block], quellen: &[Quelle], dateien: &[Datei]) -> Element {
    rsx! {
        for (i, b) in bloecke.iter().enumerate() {
            match b {
                Block::Absatz(z) => rsx! { p { key: "{i}", {stuecke(&zeile_teile(z), quellen, dateien)} } },
                Block::Titel(z) => rsx! { h3 { key: "{i}", {stuecke(&zeile_teile(z), quellen, dateien)} } },
                Block::Liste { geordnet: true, punkte } => rsx! {
                    ol { key: "{i}", for p in punkte.iter() { li { {stuecke(&zeile_teile(p), quellen, dateien)} } } }
                },
                Block::Liste { geordnet: false, punkte } => rsx! {
                    ul { key: "{i}", for p in punkte.iter() { li { {stuecke(&zeile_teile(p), quellen, dateien)} } } }
                },
                Block::Tabelle { zeilen, bild } => rsx! {
                    if let Some(spec) = bild {
                        Diagramm { key: "d{i}", spec: spec.clone(), quellen: quellen.to_vec(), dateien: dateien.to_vec() }
                    }
                    Tabelle { zeilen: zeilen.clone(), quellen: quellen.to_vec(), dateien: dateien.to_vec() }
                },
                Block::Kasten { sprache, inhalt } => rsx! { Kasten { key: "{i}", sprache: sprache.clone(), inhalt: inhalt.clone() } },
                Block::Diagramm(spec) => rsx! { Diagramm { key: "{i}", spec: spec.clone(), quellen: quellen.to_vec(), dateien: dateien.to_vec() } },
                Block::DiagrammLaedt => rsx! { div { key: "{i}", class: "ki-diagramm laedt", "Diagramm wird gezeichnet …" } },
            }
        }
    }
}

/// Die Antwort von Dieter (Markdown mit Quellen-Chips, Tabellen, Diagrammen).
#[component]
pub fn KiAntwort(text: String, #[props(default)] quellen: Vec<Quelle>, #[props(default)] dateien: Vec<Datei>) -> Element {
    let b = bloecke(&text);
    bloecke_zeigen(&b, &quellen, &dateien)
}

/// Die Quellen klein: "7 Quellen" mit den ersten Logos - beim Drueberfahren (oder Antippen)
/// klappt die Liste auf, jede Quelle ist ein Link.
#[component]
pub fn QuellenZeile(quellen: Vec<Quelle>) -> Element {
    let anzahl = quellen.len();
    let wort = t("quellen");
    rsx! {
        div { class: "ki-quellen-zeile",
            div { class: "ki-quellen-knopf", tabindex: "0",
                span { class: "ki-quellen-logos",
                    for (i, q) in quellen.iter().take(3).enumerate() {
                        FaviconKreis { key: "{i}", host: q.host.clone() }
                    }
                }
                " {anzahl} {wort}"
                div { class: "ki-quellen-liste", role: "list",
                    for (i, q) in quellen.iter().enumerate() {
                        a { key: "{i}", role: "listitem", href: "{q.url}", target: "_blank", rel: "noreferrer",
                            span { class: "ki-karte-kopf",
                                FaviconKreis { host: q.host.clone() }
                                span { "{quelle_name(&q.host)}" }
                                small { "{wert_text(&q.nr)}" }
                            }
                            span { class: "ki-karte-titel", "{q.titel}" }
                        }
                    }
                }
            }
        }
    }
}
