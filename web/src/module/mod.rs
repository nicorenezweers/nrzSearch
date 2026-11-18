//! Die Module der Ergebnisseite (Instant Answers) - je Modul eine Datei.
//!
//! Ablauf (siehe [`starten`]): fuer jedes Modul in `module.json` (Reihenfolge = Rangfolge), das
//! `an` ist, werden die `ausloeser` (JS-RegExp mit Flags "iu") auf die ganze Frage geprueft. Passt
//! einer, ruft der Kern `<modul>::erkennen(&ctx)` auf; liefert das `Some(daten)`, wird
//! `<modul>::Zeigen { ctx, daten }` in seinem Platz gezeichnet (`haupt` ueber den Treffern, `oben`
//! ueber allem, `seite` rechte Spalte). Mit `allein: true` werden die folgenden nicht mehr geprueft.
//!
//! Jedes Modul exportiert genau:
//!   pub async fn erkennen(ctx: &ModulCtx) -> Option<Value>
//!   #[component] pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element
//! Module gibt es nur im Reiter "Alle" auf Seite 1. Zustand der Suche: `crate::state::SUCHE`
//! (reaktiv), Text im Suchfeld: `crate::state::EINGABE`, neue Suche: `crate::state::suchen(q)`.

pub mod speedtest;
pub mod timer;
pub mod stoppuhr;
pub mod weltzeit;
pub mod sonne;
pub mod kalender;
pub mod ip;
pub mod passwort;
pub mod farbe;
pub mod kodieren;
pub mod json;
pub mod regex;
pub mod subnetz;
pub mod plot;
pub mod einheiten;
pub mod finanzrechner;
pub mod zufall;
pub mod bpm;
pub mod snake;
pub mod tictactoe;
pub mod labyrinth;
pub mod solitaer;
pub mod uebersetzer;
pub mod rechner;
pub mod vergleich;
pub mod kurs;
pub mod wetter;
pub mod ki;
pub mod info;

use crate::state::{EINSTELLUNGEN, Einstellungen};
use dioxus::prelude::*;
use serde_json::{Value, json};

/// Was ein Modul ueber seinen Aufruf weiss.
#[derive(Clone, Debug, PartialEq)]
pub struct ModulCtx {
    /// Name des Moduls (Schluessel in module.json).
    pub name: String,
    /// Die ganze Suchfrage.
    pub frage: String,
    /// Die Gruppen des passenden Ausloesers: [0] = ganzer Treffer, [1..] = Gruppen (None = leer).
    pub gruppen: Vec<Option<String>>,
    /// Der Eintrag des Moduls in module.json (mit eigenen Einstellungen wie "standard", "ort" …).
    pub cfg: Value,
    /// Platz: "haupt", "oben" oder "seite".
    pub platz: String,
    /// Der Aufbau der Ergebnisseite, fuer den das Modul gestartet wurde.
    pub lauf: u64,
}

impl ModulCtx {
    /// Gruppe i des Ausloesers (getrimmt), None wenn leer.
    pub fn gruppe(&self, i: usize) -> Option<&str> {
        self.gruppen.get(i).and_then(|g| g.as_deref()).map(str::trim).filter(|g| !g.is_empty())
    }
    /// Die Gruppen als JSON-Liste (Standard-Daten, wenn ein Modul kein eigenes erkennen braucht).
    pub fn gruppen_json(&self) -> Value {
        json!(self.gruppen)
    }
    /// Eine Einstellung des Moduls aus module.json.
    pub fn cfg(&self, k: &str) -> &Value {
        self.cfg.get(k).unwrap_or(&Value::Null)
    }
    pub fn cfg_str(&self, k: &str) -> Option<&str> {
        self.cfg.get(k).and_then(Value::as_str)
    }
    pub fn cfg_f64(&self, k: &str) -> Option<f64> {
        self.cfg.get(k).and_then(Value::as_f64)
    }
    pub fn cfg_bool(&self, k: &str) -> Option<bool> {
        self.cfg.get(k).and_then(Value::as_bool)
    }
    /// Die Einstellungen des Browsers (Sprache …).
    pub fn einstellungen(&self) -> Einstellungen {
        EINSTELLUNGEN.peek().clone()
    }
    /// Steht die Ergebnisseite noch, fuer die das Modul gestartet wurde?
    pub fn aktuell(&self) -> bool {
        crate::state::aktuell(self.lauf)
    }
}

/// Ein Modul, das gezeichnet wird.
#[derive(Clone, Debug, PartialEq)]
pub struct Aktiv {
    pub ctx: ModulCtx,
    pub daten: Value,
}

/// Die Module fuer eine Frage pruefen (in der Reihenfolge von module.json) und die passenden
/// melden. `melden` wird fuer jedes passende Modul sofort aufgerufen.
pub async fn starten(frage: &str, konfig: &Value, lauf: u64, mut melden: impl FnMut(Aktiv)) {
    let Some(module) = konfig.get("module").and_then(Value::as_object) else { return };
    for (name, cfg) in module {
        if cfg.get("an").and_then(Value::as_bool) == Some(false) || !NAMEN.contains(&name.as_str()) {
            continue;
        }
        let gruppen = cfg
            .get("ausloeser")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .find_map(|muster| crate::werkzeug::re_treffer(muster, "iu", frage));
        let Some(gruppen) = gruppen else { continue };
        let ctx = ModulCtx {
            name: name.clone(),
            frage: frage.to_string(),
            gruppen,
            cfg: cfg.clone(),
            platz: cfg.get("platz").and_then(Value::as_str).unwrap_or("haupt").to_string(),
            lauf,
        };
        let Some(daten) = erkennen(&ctx).await else { continue };
        if !crate::state::aktuell(lauf) {
            return;
        }
        melden(Aktiv { ctx, daten });
        if cfg.get("allein").and_then(Value::as_bool) == Some(true) {
            return;
        }
    }
}

/// Alle Module, die es gibt.
pub const NAMEN: &[&str] = &["speedtest", "timer", "stoppuhr", "weltzeit", "sonne", "kalender", "ip", "passwort", "farbe", "kodieren", "json", "regex", "subnetz", "plot", "einheiten", "finanzrechner", "zufall", "bpm", "snake", "tictactoe", "labyrinth", "solitaer", "uebersetzer", "rechner", "vergleich", "kurs", "wetter", "ki", "info"];

/// Das erkennen des Moduls.
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    match ctx.name.as_str() {
        "speedtest" => speedtest::erkennen(ctx).await,
        "timer" => timer::erkennen(ctx).await,
        "stoppuhr" => stoppuhr::erkennen(ctx).await,
        "weltzeit" => weltzeit::erkennen(ctx).await,
        "sonne" => sonne::erkennen(ctx).await,
        "kalender" => kalender::erkennen(ctx).await,
        "ip" => ip::erkennen(ctx).await,
        "passwort" => passwort::erkennen(ctx).await,
        "farbe" => farbe::erkennen(ctx).await,
        "kodieren" => kodieren::erkennen(ctx).await,
        "json" => json::erkennen(ctx).await,
        "regex" => regex::erkennen(ctx).await,
        "subnetz" => subnetz::erkennen(ctx).await,
        "plot" => plot::erkennen(ctx).await,
        "einheiten" => einheiten::erkennen(ctx).await,
        "finanzrechner" => finanzrechner::erkennen(ctx).await,
        "zufall" => zufall::erkennen(ctx).await,
        "bpm" => bpm::erkennen(ctx).await,
        "snake" => snake::erkennen(ctx).await,
        "tictactoe" => tictactoe::erkennen(ctx).await,
        "labyrinth" => labyrinth::erkennen(ctx).await,
        "solitaer" => solitaer::erkennen(ctx).await,
        "uebersetzer" => uebersetzer::erkennen(ctx).await,
        "rechner" => rechner::erkennen(ctx).await,
        "vergleich" => vergleich::erkennen(ctx).await,
        "kurs" => kurs::erkennen(ctx).await,
        "wetter" => wetter::erkennen(ctx).await,
        "ki" => ki::erkennen(ctx).await,
        "info" => info::erkennen(ctx).await,
        _ => None,
    }
}

/// Zeichnet ein Modul in seiner Section.
#[component]
pub fn Modul(aktiv: Aktiv) -> Element {
    let Aktiv { ctx, daten } = aktiv;
    let klasse = format!("modul modul-{}", ctx.name);
    let inhalt = match ctx.name.as_str() {
        "speedtest" => rsx! { speedtest::Zeigen { ctx: ctx.clone(), daten } },
        "timer" => rsx! { timer::Zeigen { ctx: ctx.clone(), daten } },
        "stoppuhr" => rsx! { stoppuhr::Zeigen { ctx: ctx.clone(), daten } },
        "weltzeit" => rsx! { weltzeit::Zeigen { ctx: ctx.clone(), daten } },
        "sonne" => rsx! { sonne::Zeigen { ctx: ctx.clone(), daten } },
        "kalender" => rsx! { kalender::Zeigen { ctx: ctx.clone(), daten } },
        "ip" => rsx! { ip::Zeigen { ctx: ctx.clone(), daten } },
        "passwort" => rsx! { passwort::Zeigen { ctx: ctx.clone(), daten } },
        "farbe" => rsx! { farbe::Zeigen { ctx: ctx.clone(), daten } },
        "kodieren" => rsx! { kodieren::Zeigen { ctx: ctx.clone(), daten } },
        "json" => rsx! { json::Zeigen { ctx: ctx.clone(), daten } },
        "regex" => rsx! { regex::Zeigen { ctx: ctx.clone(), daten } },
        "subnetz" => rsx! { subnetz::Zeigen { ctx: ctx.clone(), daten } },
        "plot" => rsx! { plot::Zeigen { ctx: ctx.clone(), daten } },
        "einheiten" => rsx! { einheiten::Zeigen { ctx: ctx.clone(), daten } },
        "finanzrechner" => rsx! { finanzrechner::Zeigen { ctx: ctx.clone(), daten } },
        "zufall" => rsx! { zufall::Zeigen { ctx: ctx.clone(), daten } },
        "bpm" => rsx! { bpm::Zeigen { ctx: ctx.clone(), daten } },
        "snake" => rsx! { snake::Zeigen { ctx: ctx.clone(), daten } },
        "tictactoe" => rsx! { tictactoe::Zeigen { ctx: ctx.clone(), daten } },
        "labyrinth" => rsx! { labyrinth::Zeigen { ctx: ctx.clone(), daten } },
        "solitaer" => rsx! { solitaer::Zeigen { ctx: ctx.clone(), daten } },
        "uebersetzer" => rsx! { uebersetzer::Zeigen { ctx: ctx.clone(), daten } },
        "rechner" => rsx! { rechner::Zeigen { ctx: ctx.clone(), daten } },
        "vergleich" => rsx! { vergleich::Zeigen { ctx: ctx.clone(), daten } },
        "kurs" => rsx! { kurs::Zeigen { ctx: ctx.clone(), daten } },
        "wetter" => rsx! { wetter::Zeigen { ctx: ctx.clone(), daten } },
        "ki" => rsx! { ki::Zeigen { ctx: ctx.clone(), daten } },
        "info" => rsx! { info::Zeigen { ctx: ctx.clone(), daten } },
        _ => rsx! {},
    };
    rsx! { section { class: "{klasse}", {inhalt} } }
}
