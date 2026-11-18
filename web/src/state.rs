//! Der globale Zustand der Oberflaeche (Dioxus GlobalSignals).
//!
//! - `ZUSTAND`: Frage, Reiter (Kategorie) und Seite - steht auch in der Adresse (`/?q=…&kategorie=…&seite=…`).
//! - `EINSTELLUNGEN`: Sprache, Zeitraum, SafeSearch, Suchmaschinen - nur im eigenen Browser.
//! - `SUCHE`: die Treffer der laufenden/letzten Suche (Module lesen sie reaktiv, z. B. die Info-Karte).
//! - `EINGABE`: der Text, der gerade im Suchfeld steht (der Rechner rechnet beim Tippen mit).
//! - `LAYOUT_LAUF`: zaehlt jeden neuen Aufbau der Ergebnisseite; Module pruefen damit, ob sie noch aktuell sind.
//! - `ANHAENGE`: Dateien, die in der Suchleiste haengen (fuer den Arbeitsbereich im KI-Modus).

use crate::werkzeug::{speicher_lesen, speicher_schreiben};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Die Reiter: id, Schluessel des Textes (i18n), Stern (KI).
pub const KATEGORIEN: &[(&str, &str, bool)] = &[
    ("ki", "ki_modus", true),
    ("general", "alle", false),
    ("images", "bilder", false),
    ("videos", "videos", false),
    ("news", "news", false),
    ("science", "wissenschaft", false),
    ("files", "dateien", false),
    ("it", "it", false),
    ("music", "musik", false),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Zustand {
    pub q: String,
    pub kategorie: String,
    pub seite: u32,
    /// Zaehlt jede gestartete Suche (auch dieselbe Frage noch einmal).
    pub lauf: u64,
}

impl Default for Zustand {
    fn default() -> Self {
        Self { q: String::new(), kategorie: "general".into(), seite: 1, lauf: 0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Einstellungen {
    pub sprache: String,
    pub zeitraum: String,
    pub safesearch: String,
    /// [] = Standard, ["alle"] = alle fuer den Reiter, sonst die Namen.
    pub motoren: Vec<String>,
}

impl Einstellungen {
    pub fn laden() -> Self {
        Self {
            sprache: crate::i18n::aktuelle_sprache(),
            zeitraum: speicher_lesen("nrz_zeitraum").and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
            safesearch: speicher_lesen("nrz_safesearch").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "2".into()),
            motoren: speicher_lesen("nrz_motoren").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default(),
        }
    }

    pub fn speichern(&self) {
        speicher_schreiben("nrz_sprache", &Value::from(self.sprache.clone()));
        speicher_schreiben("nrz_zeitraum", &Value::from(self.zeitraum.clone()));
        speicher_schreiben("nrz_safesearch", &Value::from(self.safesearch.clone()));
        speicher_schreiben("nrz_motoren", &serde_json::to_value(&self.motoren).unwrap_or_default());
    }
}

/// Ein Treffer, wie ihn der Server schickt (`/api/suche`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Treffer {
    pub titel: String,
    pub url: String,
    pub host: String,
    pub pfad: String,
    pub text: String,
    pub kategorie: String,
    pub bild: String,
    pub vorschaubild: String,
    pub groesse: String,
    pub autor: String,
    pub dauer: String,
    pub datum: Option<String>,
    pub video: String,
    pub engines: Vec<String>,
    pub position: usize,
    pub bewertung: f64,
}

/// Die Daten einer Suche (laufend oder fertig).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SuchDaten {
    pub frage: String,
    pub kategorie: String,
    pub seite: u32,
    pub treffer: Vec<Treffer>,
    pub antworten: Vec<Value>,
    pub korrekturen: Vec<Value>,
    pub hinweise: Vec<Value>,
    pub vorschlaege: Vec<Value>,
    pub kaputte: Vec<Value>,
    pub dauer_ms: u64,
    pub motoren_anzahl: u64,
    pub ausgefiltert: u64,
    pub mehr: bool,
    pub fertig: bool,
    pub laeuft: bool,
    pub fehler: Option<String>,
    pub cache: bool,
    /// Die Frage war eindeutig pornografisch - keine Treffer, nur dieser Hinweis.
    pub jugendschutz: Option<String>,
}

/// Eine Datei im Anhang / Arbeitsbereich.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Datei {
    pub name: String,
    pub typ: String,
    pub groesse: u64,
    /// Der gelesene Text (PDF, Word, Text …).
    pub text: Option<String>,
    /// Bilder: verkleinert als data:-URL.
    pub daten: Option<String>,
    pub seiten: Option<u32>,
    /// Nicht lesbar: warum (der Chip wird rot). Ergaenzt vom KI-Paket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fehler: Option<String>,
    /// Wird gerade gelesen (Chip "liest …").
    #[serde(skip)]
    pub lesen: bool,
    /// Laufende Nummer im Browser, um die Datei nach dem Lesen wiederzufinden.
    #[serde(skip)]
    pub nr: u64,
}

pub static ZUSTAND: GlobalSignal<Zustand> = Signal::global(Zustand::default);
pub static EINSTELLUNGEN: GlobalSignal<Einstellungen> = Signal::global(Einstellungen::laden);
pub static SUCHE: GlobalSignal<SuchDaten> = Signal::global(SuchDaten::default);
pub static EINGABE: GlobalSignal<String> = Signal::global(String::new);
pub static LAYOUT_LAUF: GlobalSignal<u64> = Signal::global(|| 0);
pub static ANHAENGE: GlobalSignal<Vec<Datei>> = Signal::global(Vec::new);
/// Die Konfiguration der Module (`/api/module`), sobald geladen.
pub static MODUL_KONFIG: GlobalSignal<Option<Value>> = Signal::global(|| None);

/// Die Adresse zum aktuellen Zustand.
pub fn url_bauen(z: &Zustand) -> String {
    let mut p: Vec<(&str, String)> = Vec::new();
    if !z.q.is_empty() {
        p.push(("q", z.q.clone()));
    }
    if z.kategorie != "general" {
        p.push(("kategorie", z.kategorie.clone()));
    }
    if z.seite > 1 {
        p.push(("seite", z.seite.to_string()));
    }
    let teile: Vec<String> = p.iter().map(|(k, v)| format!("{k}={}", urlencoding::encode(v))).collect();
    format!("/?{}", teile.join("&"))
}

/// Den Zustand aus der Adresse lesen (beim Start und bei Zurueck/Vor).
pub fn url_lesen() -> Zustand {
    let suche = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
    let mut z = Zustand::default();
    for paar in suche.trim_start_matches('?').split('&').filter(|s| !s.is_empty()) {
        let (k, v) = paar.split_once('=').unwrap_or((paar, ""));
        let v = urlencoding::decode(&v.replace('+', " ")).map(|c| c.into_owned()).unwrap_or_default();
        match k {
            "q" => z.q = v.trim().to_string(),
            "kategorie" if KATEGORIEN.iter().any(|(id, _, _)| *id == v) => z.kategorie = v,
            "seite" => z.seite = v.parse().unwrap_or(1).max(1),
            _ => {}
        }
    }
    z
}

/// Nur schreiben, wenn sich die Adresse aendert - ein Verlaufseintrag je Seite.
pub fn url_setzen(z: &Zustand) {
    let ziel = url_bauen(z);
    if let Some(w) = web_sys::window() {
        let jetzt = format!("{}{}", w.location().pathname().unwrap_or_default(), w.location().search().unwrap_or_default());
        if jetzt != ziel {
            let _ = w.history().and_then(|h| h.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&ziel)));
        }
    }
}

/// Eine neue Suche starten (Suchfeld, Vorschlag, Link in einem Modul …): Reiter bleibt,
/// ausser er ist "ki" - dann "Alle". Seite 1.
pub fn suchen(q: &str) {
    let q = q.trim().to_string();
    let mut z = ZUSTAND.write();
    if z.kategorie == "ki" {
        z.kategorie = "general".into();
    }
    z.q = q.clone();
    z.seite = 1;
    z.lauf += 1;
    *EINGABE.write() = q;
}

/// Den Reiter wechseln (Frage bleibt, Seite 1).
pub fn kategorie_setzen(kat: &str) {
    let mut z = ZUSTAND.write();
    z.kategorie = kat.to_string();
    z.seite = 1;
    z.lauf += 1;
}

/// Die Seite wechseln.
pub fn seite_setzen(seite: u32) {
    let mut z = ZUSTAND.write();
    z.seite = seite.max(1);
    z.lauf += 1;
}

/// Ist die Ergebnisseite noch die, fuer die ein Modul gestartet wurde?
pub fn aktuell(lauf: u64) -> bool {
    *LAYOUT_LAUF.peek() == lauf
}
