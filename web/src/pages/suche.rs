//! Die Ergebnisseite - Portierung von suchen(), aufbauen() und der Darstellung in public/app.js:
//! die Suche als Strom (/api/suche, Ereignisse "treffer" und "fertig"), Ladeskelette, Treffer,
//! Bilder (Raster, Vorschau rechts), Videos (Lightbox), News, Videos und Bilder zwischen den
//! Treffern, Module (oben, ueber den Treffern, rechts), "Meintest du", Antworten und Infoboxen
//! der Instanz, aehnliche Suchanfragen und die Seitenzahlen "nrzSeeeearch".

use crate::components::helfer::{Lupe, nach_oben, ziel_in};
use crate::i18n::t;
use crate::module::{self, Aktiv};
use crate::state::{self, EINSTELLUNGEN, LAYOUT_LAUF, MODUL_KONFIG, SUCHE, SuchDaten, Treffer, ZUSTAND, Zustand};
use crate::werkzeug::{FaviconKreis, Fett, NewsListe, datum_text, frage_url, im_scope, kopieren, quelle_name, use_tasten};
use crate::{api, bib};
use dioxus::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure};

/// Laeuft gerade eine Suche? (Ladebalken oben, Kreis am aktiven Reiter.)
pub static LAEDT: GlobalSignal<bool> = Signal::global(|| false);
/// Zaehlt "von vorn beginnen" (neue Einstellung, Zurueck/Vor): dann wird alles neu aufgebaut,
/// auch bei gleicher Frage und gleichem Reiter.
pub static NEU_AUFBAU: GlobalSignal<u64> = Signal::global(|| 0);
/// Dieses Bild nach dem Wechsel in "Bilder" in der Vorschau zeigen.
static VORSCHAU_NACH: GlobalSignal<Option<Treffer>> = Signal::global(|| None);

const TREFFER_PRO_SEITE: usize = 10;
const BILDER_PRO_SEITE: usize = 60;
/// Die Module, neben denen Videos und Bilder stehen duerfen - hat ein Werkzeug die Frage
/// beantwortet (Timer, Rechner, Spiel …), bleiben sie weg.
const NEBEN_EINSCHUEBEN: &[&str] = &["ki", "info", "kurs", "wetter", "vergleich"];

thread_local! {
    /// Das Seitenverhaeltnis geladener Bilder - gemerkt, damit nichts springt.
    static VERHAELTNISSE: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// Eine neue Einstellung heisst: wieder von vorn beginnen.
pub fn neu_suchen() {
    if ZUSTAND.peek().q.is_empty() {
        return;
    }
    *NEU_AUFBAU.write() += 1;
    let mut z = ZUSTAND.write();
    z.seite = 1;
    z.lauf += 1;
}

/// Die Sprache waehlen (Footer und Tools): gespeichert, die Suche beginnt neu.
pub fn sprache_setzen(code: &str) {
    {
        let mut e = EINSTELLUNGEN.write();
        e.sprache = code.to_string();
        e.speichern();
    }
    if ZUSTAND.peek().kategorie != "ki" {
        neu_suchen();
    }
}

/// Videos und Bilder zwischen den Treffern ("Alle", Seite 1).
#[derive(Clone, Debug, Default, PartialEq)]
struct Einschuebe {
    videos: Vec<Treffer>,
    videos_nach: usize,
    bilder: Vec<Treffer>,
    bilder_nach: usize,
    bilder_anzahl: usize,
    /// Bilder, die die Quelle sperrt - das naechste rueckt nach.
    kaputt: HashSet<String>,
}

/// Der Zustand der Ergebnisseite (alles Signale - `Copy`).
#[derive(Clone, Copy, PartialEq)]
struct Seite {
    /// Frage, Reiter und NEU_AUFBAU, fuer die der Aufbau steht.
    layout: Signal<String>,
    module: Signal<Vec<Aktiv>>,
    module_fertig: Signal<bool>,
    einschuebe: Signal<Einschuebe>,
    aehnlich: Signal<Vec<String>>,
    /// Die Infoboxen der Instanz zeigen (nur wenn kein Modul die Frage beantwortet).
    karten: Signal<bool>,
    /// Fuer welchen Aufbau das Ende schon verarbeitet ist.
    ende_lauf: Signal<u64>,
    /// Die vorige Seite, solange die neue laedt (gleiche Frage, andere Seite).
    alt: Signal<Option<SuchDaten>>,
    vorschau: Signal<Option<Treffer>>,
    lightbox: Signal<Option<(String, String)>>,
    strom: Signal<Option<Rc<api::Strom>>>,
}

fn treffer_aus(v: &Value) -> Vec<Treffer> {
    v.get("treffer")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|t| serde_json::from_value(t.clone()).ok()).collect())
        .unwrap_or_default()
}

/// Die Felder einer Antwort uebernehmen; `alles` = aus dem Cache (alles auf einmal).
fn uebernehmen(d: &mut SuchDaten, v: &Value, alles: bool) {
    if alles {
        d.treffer = treffer_aus(v);
        d.cache = true;
    }
    let liste = |k: &str| v.get(k).and_then(Value::as_array).cloned();
    if let Some(x) = v.get("dauerMs").and_then(Value::as_u64) {
        d.dauer_ms = x;
    }
    if let Some(x) = v.get("mehr").and_then(Value::as_bool) {
        d.mehr = x;
    }
    if let Some(x) = v.get("motorenAnzahl").and_then(Value::as_u64) {
        d.motoren_anzahl = x;
    }
    if let Some(x) = v.get("ausgefiltert").and_then(Value::as_u64) {
        d.ausgefiltert = x;
    }
    if v.get("jugendschutz").and_then(Value::as_bool) == Some(true) {
        d.jugendschutz = Some(v.get("hinweis").and_then(Value::as_str).unwrap_or("Keine Treffer (Jugendschutz-Filter).").to_string());
    }
    if let Some(x) = liste("vorschlaege") {
        d.vorschlaege = x;
    }
    if let Some(x) = liste("kaputte") {
        d.kaputte = x;
    }
    for (k, ziel) in [("korrekturen", &mut d.korrekturen), ("antworten", &mut d.antworten), ("hinweise", &mut d.hinweise)] {
        if let Some(x) = liste(k).filter(|x| alles || !x.is_empty()) {
            *ziel = x;
        }
    }
}

/// Passt ein Titel zur Frage? Mindestens die Haelfte der Suchwoerter steht darin.
fn passt(titel: &str, frage: &str) -> bool {
    let woerter: Vec<String> = frage.to_lowercase().split_whitespace().filter(|w| w.chars().count() > 2).map(String::from).collect();
    let klein = titel.to_lowercase();
    woerter.is_empty() || woerter.iter().filter(|w| klein.contains(w.as_str())).count() * 2 >= woerter.len()
}

/// Die Bilder im Raster (in seiner Reihenfolge).
fn bilder_von(d: &SuchDaten) -> Vec<Treffer> {
    d.treffer.iter().take(BILDER_PRO_SEITE).filter(|t| !t.bild.is_empty()).cloned().collect()
}

/// Das Seitenverhaeltnis eines Bildes: aus der Groesse, die die Instanz meldet, sonst aus dem
/// geladenen Vorschaubild.
fn verhaeltnis(t: &Treffer) -> f64 {
    if let Some(v) = VERHAELTNISSE.with(|m| m.borrow().get(&t.bild).copied()) {
        return v;
    }
    let teile: Vec<f64> = t.groesse.split(['x', '×']).map(|s| s.trim().parse().unwrap_or(0.0)).collect();
    match teile.as_slice() {
        [b, h] if *b > 0.0 && *h > 0.0 => b / h,
        _ => 4.0 / 3.0,
    }
}

/// Eindeutige Schluessel fuer eine Liste (doppelte bekommen die Stelle dazu).
fn schluessel<'a>(namen: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut gesehen = HashSet::new();
    namen
        .enumerate()
        .map(|(i, n)| if gesehen.insert(n.to_string()) { n.to_string() } else { format!("{n}#{i}") })
        .collect()
}

fn seite_wechseln(seite: u32) {
    let ziel = seite.clamp(1, 50);
    nach_oben(false);
    if ziel == ZUSTAND.peek().seite {
        return;
    }
    state::seite_setzen(ziel);
}

fn kategorie_wechseln(kat: &str) {
    nach_oben(false);
    state::kategorie_setzen(kat);
}

impl Seite {
    /// Eine Suche starten (bei jeder Aenderung von ZUSTAND).
    fn suchen(mut self, z: Zustand) {
        if z.kategorie == "ki" || z.q.is_empty() {
            return;
        }
        // Der alte Strom wird geschlossen (fallen gelassen).
        self.strom.set(None);
        *LAEDT.write() = true;
        self.aufbauen(&z);
        *SUCHE.write() = SuchDaten {
            frage: z.q.clone(),
            kategorie: z.kategorie.clone(),
            seite: z.seite,
            mehr: true,
            laeuft: true,
            ..Default::default()
        };

        let e = EINSTELLUNGEN.peek().clone();
        let seite = z.seite.to_string();
        let motoren = e.motoren.join(",");
        let url = api::url_mit(
            "/api/suche",
            &[
                ("q", &z.q),
                ("kategorie", &z.kategorie),
                ("page", &seite),
                ("sprache", &e.sprache),
                ("zeitraum", &e.zeitraum),
                ("safesearch", &e.safesearch),
                ("motoren", &motoren),
            ],
        );
        let lauf = z.lauf;
        let fertig = Rc::new(Cell::new(false));
        let verarbeiten = im_scope({
            let fertig = fertig.clone();
            move |(name, v): (String, Value)| {
                if ZUSTAND.peek().lauf != lauf || fertig.get() {
                    return;
                }
                match name.as_str() {
                    "treffer" => {
                        // Aus dem Cache kommt die ganze Antwort auf einmal.
                        if v.get("cache").and_then(Value::as_bool) == Some(true) {
                            uebernehmen(&mut SUCHE.write(), &v, true);
                            self.abschluss(&fertig);
                            return;
                        }
                        let neu = treffer_aus(&v);
                        {
                            let mut d = SUCHE.write();
                            // Der Server schickt die ganze, neu sortierte Rangliste.
                            if v.get("ersetzen").and_then(Value::as_bool) == Some(true) {
                                d.treffer = neu;
                            } else {
                                d.treffer.extend(neu);
                            }
                        }
                        if self.alt.peek().is_some() {
                            self.alt.set(None);
                        }
                        self.vorschau_nach();
                    }
                    "fertig" => {
                        uebernehmen(&mut SUCHE.write(), &v, false);
                        self.abschluss(&fertig);
                    }
                    _ => {
                        if !SUCHE.peek().treffer.is_empty() {
                            self.abschluss(&fertig);
                        } else {
                            fertig.set(true);
                            self.schliessen();
                            *LAEDT.write() = false;
                            let mut d = SUCHE.write();
                            d.fehler = Some("Die Verbindung zur Suchmaschine ist abgebrochen.".into());
                            d.laeuft = false;
                            drop(d);
                            self.alt.set(None);
                        }
                    }
                }
            }
        });
        let an = {
            let v = verarbeiten.clone();
            move |name: &str, daten: Value| v((name.to_string(), daten))
        };
        let bei_fehler = move || verarbeiten(("fehler".to_string(), Value::Null));
        match api::Strom::oeffnen(&url, &["treffer", "fertig"], an, bei_fehler) {
            Some(s) => self.strom.set(Some(Rc::new(s))),
            None => {
                *LAEDT.write() = false;
                let mut d = SUCHE.write();
                d.fehler = Some("Keine Verbindung zum Server.".into());
                d.laeuft = false;
            }
        }
    }

    fn schliessen(self) {
        if let Some(s) = self.strom.peek().as_ref() {
            s.schliessen();
        }
    }

    fn abschluss(mut self, fertig: &Cell<bool>) {
        fertig.set(true);
        self.schliessen();
        *LAEDT.write() = false;
        {
            let mut d = SUCHE.write();
            d.fertig = true;
            d.laeuft = false;
        }
        if self.alt.peek().is_some() {
            self.alt.set(None);
        }
        self.vorschau_nach();
        self.nach_dem_ende();
    }

    /// Der Aufbau der Ergebnisseite. Bleiben Frage und Reiter gleich (andere Seite), bleibt
    /// alles stehen - auch laufende Module wie ein Timer - und nur die Treffer werden getauscht.
    fn aufbauen(mut self, z: &Zustand) {
        let schluessel = format!("{}\n{}\n{}", z.q, z.kategorie, NEU_AUFBAU.peek());
        if self.vorschau.peek().is_some() {
            self.vorschau.set(None);
        }
        if *self.layout.peek() == schluessel {
            let alte = SUCHE.peek().clone();
            if !alte.treffer.is_empty() {
                self.alt.set(Some(alte));
            }
            return;
        }
        self.layout.set(schluessel);
        *LAYOUT_LAUF.write() += 1;
        let lauf = *LAYOUT_LAUF.peek();
        self.module.set(Vec::new());
        self.module_fertig.set(false);
        self.einschuebe.set(Einschuebe::default());
        self.aehnlich.set(Vec::new());
        self.karten.set(false);
        self.alt.set(None);
        nach_oben(false);
        // Instant Answers gibt es - wie bei Google - nur im Reiter "Alle".
        if z.kategorie == "general" {
            let q = z.q.clone();
            spawn(async move { self.module_starten(q, lauf).await });
        } else {
            self.module_fertig.set(true);
        }
    }

    async fn module_starten(mut self, q: String, lauf: u64) {
        // Erst kopieren, dann pruefen: der Lese-Guard darf beim Schreiben unten nicht mehr leben.
        let gemerkt = MODUL_KONFIG.peek().clone();
        let konfig = match gemerkt {
            Some(k) => k,
            None => {
                let k = api::get_oder_nichts("/api/module", &[]).await;
                if let Some(k) = &k {
                    *MODUL_KONFIG.write() = Some(k.clone());
                }
                k.unwrap_or(Value::Null)
            }
        };
        if !state::aktuell(lauf) {
            return;
        }
        let mut liste = self.module;
        module::starten(&q, &konfig, lauf, move |a| liste.write().push(a)).await;
        if !state::aktuell(lauf) {
            return;
        }
        self.module_fertig.set(true);
        self.einschuebe_holen(&q, &konfig, lauf);
        self.nach_dem_ende();
    }

    /// Videos und Bilder zwischen den Treffern - wie bei Google.
    fn einschuebe_holen(self, q: &str, konfig: &Value, lauf: u64) {
        if self.module.peek().iter().any(|m| !NEBEN_EINSCHUEBEN.contains(&m.ctx.name.as_str())) {
            return;
        }
        let cfg = konfig.get("einschuebe").cloned().unwrap_or(Value::Null);
        for (art, kat) in [("videos", "videos"), ("bilder", "images")] {
            let Some(c) = cfg.get(art).cloned() else { continue };
            if c.get("an").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let (q, mut einschuebe) = (q.to_string(), self.einschuebe);
            spawn(async move {
                let e = EINSTELLUNGEN.peek().clone();
                let d = api::get_oder_nichts("/api/search", &[("q", &q), ("kategorie", kat), ("sprache", &e.sprache), ("safesearch", &e.safesearch)]).await;
                let zahl = |k: &str, standard: u64| c.get(k).and_then(Value::as_u64).unwrap_or(standard) as usize;
                let anzahl = zahl("anzahl", 3);
                // Ein paar Bilder mehr als Ersatz - manche sperrt die Quelle.
                let liste: Vec<Treffer> = d
                    .as_ref()
                    .map(treffer_aus)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|t| (if art == "videos" { !t.vorschaubild.is_empty() } else { !t.bild.is_empty() }) && passt(&t.titel, &q))
                    .take(anzahl + if art == "bilder" { 6 } else { 0 })
                    .collect();
                if !state::aktuell(lauf) || liste.len() < zahl("mindestens", 2) {
                    return;
                }
                let mut w = einschuebe.write();
                if art == "videos" {
                    w.videos = liste;
                    w.videos_nach = zahl("nach", 3);
                } else {
                    w.bilder = liste;
                    w.bilder_nach = zahl("nach", 3);
                    w.bilder_anzahl = anzahl;
                }
            });
        }
    }

    /// Am Ende (und wenn die Module geprueft sind - einmal je Aufbau): die Infoboxen der
    /// Instanz (nur ohne Modul) und die aehnlichen Suchanfragen.
    fn nach_dem_ende(mut self) {
        let lauf = *LAYOUT_LAUF.peek();
        if ZUSTAND.peek().kategorie != "general" || !*self.module_fertig.peek() || *self.ende_lauf.peek() == lauf || !SUCHE.peek().fertig {
            return;
        }
        self.ende_lauf.set(lauf);
        // Bei "pacman" will niemand den Paketmanager sehen.
        let ohne_module = self.module.peek().is_empty();
        self.karten.set(ohne_module);
        let (frage, vorschlaege) = {
            let d = SUCHE.peek();
            (d.frage.clone(), d.vorschlaege.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
        };
        let mut aehnlich = self.aehnlich;
        spawn(async move {
            let mut liste = vorschlaege;
            if liste.is_empty() {
                liste = api::get_oder_nichts("/api/vorschlaege", &[("q", &frage)])
                    .await
                    .as_ref()
                    .and_then(|d| d.get("vorschlaege"))
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
            }
            let klein = frage.to_lowercase();
            let liste: Vec<String> = liste.into_iter().filter(|v| v.to_lowercase() != klein).take(8).collect();
            if state::aktuell(lauf) && !liste.is_empty() {
                aehnlich.set(liste);
            }
        });
    }

    /// Ein Bild aus "Alle" nach dem Wechsel in "Bilder" in der Vorschau oeffnen.
    fn vorschau_nach(mut self) {
        if ZUSTAND.peek().kategorie != "images" || SUCHE.peek().treffer.is_empty() {
            return;
        }
        let Some(gesucht) = VORSCHAU_NACH.peek().clone() else { return };
        *VORSCHAU_NACH.write() = None;
        let t = bilder_von(&SUCHE.peek()).into_iter().find(|b| b.bild == gesucht.bild).unwrap_or(gesucht);
        self.vorschau.set(Some(t));
    }

    fn blaettern(mut self, schritt: i64) {
        let Some(jetzt) = self.vorschau.peek().clone() else { return };
        let bilder = bilder_von(&SUCHE.peek());
        let Some(i) = bilder.iter().position(|b| b.bild == jetzt.bild) else { return };
        let ziel = i as i64 + schritt;
        if ziel >= 0 && (ziel as usize) < bilder.len() {
            self.vorschau.set(Some(bilder[ziel as usize].clone()));
        }
    }
}

#[component]
pub fn Ergebnisseite() -> Element {
    let mut seite = Seite {
        layout: use_signal(String::new),
        module: use_signal(Vec::new),
        module_fertig: use_signal(|| false),
        einschuebe: use_signal(Einschuebe::default),
        aehnlich: use_signal(Vec::new),
        karten: use_signal(|| false),
        ende_lauf: use_signal(|| 0),
        alt: use_signal(|| None),
        vorschau: use_signal(|| None),
        lightbox: use_signal(|| None),
        strom: use_signal(|| None),
    };

    // Jede neue Suche, jeder Reiter, jede Seite.
    use_effect(move || {
        let z = ZUSTAND.read().clone();
        seite.suchen(z);
    });
    use_drop(|| {
        *LAEDT.write() = false;
    });

    // Escape schliesst Lightbox und Vorschau, die Pfeile blaettern in der Vorschau.
    use_tasten(move |e| {
        let taste = e.key();
        if taste == "Escape" && seite.lightbox.peek().is_some() {
            seite.lightbox.set(None);
        } else if seite.vorschau.peek().is_some() {
            match taste.as_str() {
                "Escape" => seite.vorschau.set(None),
                "ArrowLeft" | "ArrowRight" => {
                    e.prevent_default();
                    seite.blaettern(if taste == "ArrowLeft" { -1 } else { 1 });
                }
                _ => {}
            }
        }
    });
    // Offene Lightbox: die Seite darunter rollt nicht.
    use_effect(move || {
        let offen = seite.lightbox.read().is_some();
        if let Some(b) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.body()) {
            let _ = b.style().set_property("overflow", if offen { "hidden" } else { "" });
        }
    });
    // Das gewaehlte Bild im Raster sichtbar halten.
    use_effect(move || {
        if seite.vorschau.read().is_none() {
            return;
        }
        let Some(d) = web_sys::window().and_then(|w| w.document()) else { return };
        if let Ok(Some(el)) = d.query_selector(".bilder .bild.gewaehlt") {
            let o = web_sys::ScrollIntoViewOptions::new();
            o.set_block(web_sys::ScrollLogicalPosition::Nearest);
            o.set_behavior(web_sys::ScrollBehavior::Smooth);
            el.scroll_into_view_with_scroll_into_view_options(&o);
        }
        if let Some(p) = d.get_element_by_id("vorschau") {
            p.set_scroll_top(0);
        }
    });

    let z = ZUSTAND.read().clone();
    let breit = z.kategorie == "images" || z.kategorie == "videos";
    let vorschau = seite.vorschau.read().clone();
    let klasse = match (breit, vorschau.is_some()) {
        (true, true) => "breit vorschau-offen",
        (true, false) => "breit",
        _ => "",
    };
    let module = seite.module.read();
    let im_platz = |p: &str| module.iter().filter(|a| a.ctx.platz == p).cloned().collect::<Vec<_>>();
    let (oben, haupt, rechts) = (im_platz("oben"), im_platz("haupt"), im_platz("seite"));

    rsx! {
        main {
            id: "inhalt",
            class: "{klasse}",
            // Ein Klick auf einen Treffer: die Uebersicht mit KI und der Vergleich hoeren auf -
            // wer schon weiterliest, braucht sie nicht mehr (spart Tokens).
            onclick: move |e: MouseEvent| {
                let daten = e.data();
                let Some(ev) = daten.downcast::<web_sys::MouseEvent>() else { return };
                let Some(a) = ziel_in(ev, "a[href]") else { return };
                let extern_ = a.get_attribute("href").is_some_and(|h| h.starts_with("http:") || h.starts_with("https:"));
                if extern_ && a.closest(".modul-ki .ki-fuss, .modul-vergleich .ki-fuss").ok().flatten().is_none() {
                    bib::ki::abbrechen();
                }
            },
            div { id: "oben", class: "oben", hidden: z.seite > 1,
                for a in oben {
                    module::Modul { key: "{a.ctx.name}", aktiv: a }
                }
            }
            div { class: "spalten", "data-seite": "{z.seite}",
                div { class: "spalte-haupt",
                    div { id: "korrektur", Korrektur {} }
                    div { id: "widgets",
                        for a in haupt {
                            module::Modul { key: "{a.ctx.name}", aktiv: a }
                        }
                    }
                    TrefferBereich { seite }
                    div { id: "aehnlich", Aehnlich { seite } }
                }
                aside { id: "seite", class: "seite-karte", "aria-label": "Info",
                    for a in rechts {
                        module::Modul { key: "{a.ctx.name}", aktiv: a }
                    }
                    Karten { seite }
                }
                aside { id: "vorschau", class: "vorschau", "aria-label": "Bildvorschau", hidden: vorschau.is_none(),
                    if let Some(t) = vorschau.clone() {
                        VorschauPanel { seite, t }
                    }
                }
            }
        }
        Lightbox { seite }
    }
}

// ---- Treffer -------------------------------------------------------------------------------

/// Ladeskelette - verhindern das Ueberblenden fremder Ergebnisse.
#[component]
fn Skelett(kategorie: String) -> Element {
    match kategorie.as_str() {
        "images" => rsx! {
            div { class: "bilder-skelett", "aria-busy": "true",
                for i in 0..18 {
                    div { key: "{i}", class: "skelett-rahmen" }
                }
            }
        },
        "videos" => rsx! {
            div { class: "videos-skelett", "aria-busy": "true",
                for i in 0..6 {
                    div { key: "{i}", class: "skelett-video",
                        div { class: "skelett-rahmen video" }
                        div { class: "skelett-zeile titel" }
                        div { class: "skelett-zeile kurz" }
                    }
                }
            }
        },
        _ => rsx! {
            div { class: "treffer-skelett", "aria-busy": "true",
                for i in 0..5 {
                    div { key: "{i}", class: "skelett-treffer",
                        div { class: "skelett-kopf",
                            div { class: "skelett-kreis" }
                            div { class: "skelett-meta",
                                div { class: "skelett-zeile mini" }
                                div { class: "skelett-zeile micro" }
                            }
                        }
                        div { class: "skelett-zeile titel" }
                        div { class: "skelett-zeile snippet" }
                        div { class: "skelett-zeile snippet kurz" }
                    }
                }
            }
        },
    }
}

/// Der Bereich mit den Treffern: Skelett, Liste, "keine Treffer" oder Fehler.
#[component]
fn TrefferBereich(seite: Seite) -> Element {
    let z = ZUSTAND.read();
    let suche = SUCHE.read();
    let alt = seite.alt.read();
    let aktuell = suche.frage == z.q && suche.kategorie == z.kategorie && suche.seite == z.seite;
    let laedt = alt.is_some();
    let d: &SuchDaten = match alt.as_ref() {
        Some(a) => a,
        None if aktuell => &suche,
        None => return rsx! { div { class: "treffer-bereich", Skelett { kategorie: z.kategorie.clone() } } },
    };

    let inhalt = if let (Some(f), false) = (&d.fehler, laedt) {
        rsx! {
            div { class: "meldung",
                p { b { "Die Suche ist gerade nicht möglich." } }
                p { "{f}" }
                button {
                    r#type: "button",
                    class: "knopf",
                    onclick: move |_| state::seite_setzen(ZUSTAND.peek().seite),
                    "Erneut versuchen"
                }
            }
        }
    } else if let Some(h) = &d.jugendschutz {
        rsx! {
            div { class: "keine jugendschutz",
                p { b { "{h}" } }
                p { "nrzSearch zeigt keine pornografischen Inhalte." }
            }
        }
    } else if d.treffer.is_empty() {
        if !d.fertig {
            rsx! { Skelett { kategorie: z.kategorie.clone() } }
        } else {
            rsx! {
                div { class: "keine",
                    p { "Die Suche nach " b { "{d.frage}" } " ergab keine Treffer." }
                    p { "Vorschläge:" }
                    ul {
                        li { "Achte darauf, dass alle Wörter richtig geschrieben sind." }
                        li { "Probiere andere Suchbegriffe." }
                        li { "Probiere allgemeinere Suchbegriffe." }
                    }
                }
            }
        }
    } else {
        let stueck = match z.kategorie.as_str() {
            "images" => rsx! { BilderRaster { seite, bilder: bilder_von(d) } },
            "videos" => rsx! { VideoListe { seite, treffer: d.treffer.iter().take(TREFFER_PRO_SEITE).cloned().collect::<Vec<_>>(), frage: d.frage.clone() } },
            "news" => rsx! { NewsListe { treffer: d.treffer.iter().take(TREFFER_PRO_SEITE).cloned().collect::<Vec<_>>(), frage: d.frage.clone() } },
            _ => rsx! {
                TrefferListe {
                    seite,
                    treffer: d.treffer.iter().take(TREFFER_PRO_SEITE).cloned().collect::<Vec<_>>(),
                    frage: d.frage.clone(),
                    einschuebe: z.kategorie == "general" && z.seite == 1,
                }
            },
        };
        rsx! {
            {stueck}
            if z.kategorie != "images" {
                Seitenzahlen { aktuell: z.seite, mehr: d.mehr }
            }
            if d.ausgefiltert > 0 {
                p { class: "filter-hinweis", "{d.ausgefiltert} Treffer wurden vom Filter entfernt." }
            }
        }
    };

    rsx! {
        div { class: if laedt { "treffer-bereich laedt" } else { "treffer-bereich" }, {inhalt} }
    }
}

const EINSCHUB_V: &str = "einschub-videos";
const EINSCHUB_B: &str = "einschub-bilder";

enum Stueck {
    Treffer(usize),
    Videos,
    Bilder,
}

/// Die Trefferliste - in "Alle" auf Seite 1 mit Videos und Bildern dazwischen.
#[component]
fn TrefferListe(seite: Seite, treffer: Vec<Treffer>, frage: String, einschuebe: bool) -> Element {
    let e = seite.einschuebe.read();
    let mut stuecke: Vec<Stueck> = (0..treffer.len()).map(Stueck::Treffer).collect();
    if einschuebe {
        let mut dazu: Vec<(usize, Stueck)> = Vec::new();
        if !e.videos.is_empty() {
            dazu.push((e.videos_nach, Stueck::Videos));
        }
        if !e.bilder.is_empty() {
            dazu.push((e.bilder_nach, Stueck::Bilder));
        }
        dazu.sort_by(|a, b| b.0.cmp(&a.0));
        for (nach, s) in dazu {
            let i = nach.min(stuecke.len());
            stuecke.insert(i, s);
        }
    }
    let schl = schluessel(treffer.iter().map(|t| t.url.as_str()));
    rsx! {
        for s in stuecke {
            match s {
                Stueck::Treffer(i) => rsx! { TrefferZeile { key: "{schl[i]}", t: treffer[i].clone(), frage: frage.clone() } },
                Stueck::Videos => rsx! { VideoEinschub { key: "{EINSCHUB_V}", seite, liste: e.videos.clone() } },
                Stueck::Bilder => rsx! { BilderEinschub { key: "{EINSCHUB_B}", seite } },
            }
        }
    }
}

#[component]
fn TrefferZeile(t: Treffer, frage: String) -> Element {
    rsx! {
        div { class: "treffer",
            a { class: "kopfzeile", href: "{t.url}", target: "_blank", rel: "noreferrer", tabindex: "-1",
                FaviconKreis { host: t.host.clone() }
                span { class: "adresse",
                    span { class: "name", "{t.host}" }
                    span { class: "pfad", "{t.pfad}" }
                }
            }
            h3 {
                a { href: "{t.url}", target: "_blank", rel: "noreferrer",
                    Fett { text: t.titel.clone(), frage: frage.clone() }
                }
            }
            p { class: "text",
                if let Some(d) = t.datum.as_deref().filter(|d| !d.is_empty()) {
                    span { class: "datum", "{datum_text(d)} — " }
                }
                Fett { text: t.text.clone(), frage: frage.clone() }
            }
        }
    }
}

/// Ein Bild; ist es an der Quelle gesperrt, wird es zur Textflaeche.
#[component]
fn Bild(src: String, #[props(default)] alt: String, #[props(default)] lazy: bool, onmass: Option<EventHandler<f64>>) -> Element {
    let mut kaputt = use_signal(|| false);
    if kaputt() {
        return rsx! { span { class: "platzhalter", "Bild nicht verfügbar" } };
    }
    rsx! {
        img {
            src: "{src}",
            alt: "{alt}",
            loading: if lazy { "lazy" } else { "eager" },
            onload: move |e: Event<ImageData>| {
                let Some(h) = onmass else { return };
                let Some(img) = e.data().downcast::<web_sys::Event>().and_then(|ev| ev.target()).and_then(|t| t.dyn_into::<web_sys::HtmlImageElement>().ok()) else { return };
                if img.natural_height() > 0 {
                    h.call(f64::from(img.natural_width()) / f64::from(img.natural_height()));
                }
            },
            onerror: move |_| kaputt.set(true),
        }
    }
}

const MEHR_PFEIL: &str = "›";

#[component]
fn MehrZeile(kat: String, text: String) -> Element {
    rsx! {
        div { class: "mehr-zeile",
            button { r#type: "button", class: "mehr-anzeigen", onclick: move |_| kategorie_wechseln(&kat),
                "{text} "
                span { "{MEHR_PFEIL}" }
            }
        }
    }
}

/// Ein Video: mit Einbettung oeffnet ein Klick die Lightbox.
fn video_klick(mut seite: Seite, t: &Treffer) -> impl FnMut(MouseEvent) + 'static {
    let (video, url) = (t.video.clone(), t.url.clone());
    move |e: MouseEvent| {
        if !video.is_empty() {
            e.prevent_default();
            seite.lightbox.set(Some((video.clone(), url.clone())));
        }
    }
}

#[component]
fn VideoEinschub(seite: Seite, liste: Vec<Treffer>) -> Element {
    let schl = schluessel(liste.iter().map(|t| t.url.as_str()));
    rsx! {
        section { class: "einschub",
            h2 { {t("videos")} }
            for (i, v) in liste.iter().enumerate() {
                a { key: "{schl[i]}", class: "ev", href: "{v.url}", target: "_blank", rel: "noreferrer", onclick: video_klick(seite, v),
                    span { class: "ev-bild",
                        Bild { src: v.vorschaubild.clone(), lazy: true }
                        span { class: "ev-play" }
                        if !v.dauer.is_empty() {
                            span { class: "ev-dauer", "{v.dauer}" }
                        }
                    }
                    span { class: "ev-text",
                        span { class: "ev-titel", "{v.titel}" }
                        span { class: "ev-meta",
                            {quelle_name(&v.host)}
                            if !v.autor.is_empty() { " · {v.autor}" }
                        }
                        if let Some(d) = v.datum.as_deref().filter(|d| !d.is_empty()) {
                            span { class: "ev-meta", {datum_text(d)} }
                        }
                    }
                }
            }
            MehrZeile { kat: "videos", text: t("mehr_anzeigen") }
        }
    }
}

#[component]
fn BilderEinschub(seite: Seite) -> Element {
    let e = seite.einschuebe.read();
    let sichtbar: Vec<Treffer> = e.bilder.iter().filter(|b| !e.kaputt.contains(&b.bild)).take(e.bilder_anzahl).cloned().collect();
    let schl = schluessel(sichtbar.iter().map(|t| t.bild.as_str()));
    rsx! {
        section { class: "einschub",
            h2 { {t("bilder")} }
            div { class: "eb-raster",
                for (i, b) in sichtbar.into_iter().enumerate() {
                    a {
                        key: "{schl[i]}",
                        class: "eb",
                        href: "{b.url}",
                        target: "_blank",
                        rel: "noreferrer",
                        onclick: {
                            let b = b.clone();
                            move |ev: MouseEvent| {
                                ev.prevent_default();
                                *VORSCHAU_NACH.write() = Some(b.clone());
                                kategorie_wechseln("images");
                            }
                        },
                        span { class: "rahmen",
                            img {
                                src: if b.vorschaubild.is_empty() { b.bild.clone() } else { b.vorschaubild.clone() },
                                alt: "{b.titel}",
                                loading: "lazy",
                                // Zwischen den Treffern rueckt das naechste Bild nach.
                                onerror: {
                                    let bild = b.bild.clone();
                                    let mut einschuebe = seite.einschuebe;
                                    move |_| {
                                        einschuebe.write().kaputt.insert(bild.clone());
                                    }
                                },
                            }
                        }
                        span { class: "bt", "{b.titel}" }
                        span { class: "quelle",
                            FaviconKreis { host: b.host.clone() }
                            {quelle_name(&b.host)}
                        }
                    }
                }
            }
            MehrZeile { kat: "images", text: t("mehr_bilder") }
        }
    }
}

/// Die Bilder wie bei Google: Zeilen gleicher Hoehe, jedes Bild in seinem eigenen Format.
/// Ein Klick oeffnet rechts die Vorschau.
#[component]
fn BilderRaster(seite: Seite, bilder: Vec<Treffer>) -> Element {
    let gewaehlt = seite.vorschau.read().as_ref().map(|t| t.bild.clone());
    let schl = schluessel(bilder.iter().map(|t| t.bild.as_str()));
    rsx! {
        div { class: "bilder",
            for (i, b) in bilder.into_iter().enumerate() {
                BildKachel {
                    key: "{schl[i]}",
                    gewaehlt: gewaehlt.as_deref() == Some(b.bild.as_str()),
                    nr: i,
                    t: b,
                    onwahl: move |t: Treffer| {
                        let mut v = seite.vorschau;
                        v.set(Some(t));
                    },
                }
            }
        }
    }
}

#[component]
fn BildKachel(t: Treffer, nr: usize, gewaehlt: bool, onwahl: EventHandler<Treffer>) -> Element {
    let mut v = use_signal(|| verhaeltnis(&t));
    let stil = format!("--v:{:.3}", v().clamp(0.5, 2.4));
    let bild = t.bild.clone();
    let wahl = t.clone();
    rsx! {
        a {
            class: if gewaehlt { "bild gewaehlt" } else { "bild" },
            href: "{t.url}",
            target: "_blank",
            rel: "noreferrer",
            "data-nr": "{nr}",
            style: "{stil}",
            onclick: move |e: MouseEvent| {
                e.prevent_default();
                onwahl.call(wahl.clone());
            },
            span { class: "rahmen",
                Bild {
                    src: if t.vorschaubild.is_empty() { t.bild.clone() } else { t.vorschaubild.clone() },
                    alt: t.titel.clone(),
                    // Fehlt die Groesse, gilt das Format des geladenen Vorschaubildes.
                    onmass: move |x: f64| {
                        let neu = VERHAELTNISSE.with(|m| {
                            let mut m = m.borrow_mut();
                            if m.contains_key(&bild) {
                                false
                            } else {
                                m.insert(bild.clone(), x);
                                true
                            }
                        });
                        if neu {
                            v.set(x);
                        }
                    },
                }
            }
            span { class: "quelle",
                FaviconKreis { host: t.host.clone() }
                "{t.host}"
            }
            span { class: "bt", "{t.titel}" }
        }
    }
}

#[component]
fn VideoListe(seite: Seite, treffer: Vec<Treffer>, frage: String) -> Element {
    let schl = schluessel(treffer.iter().map(|t| t.url.as_str()));
    rsx! {
        div { class: "videos",
            for (i, v) in treffer.iter().enumerate() {
                a { key: "{schl[i]}", class: "video", href: "{v.url}", target: "_blank", rel: "noreferrer", onclick: video_klick(seite, v),
                    span { class: "rahmen",
                        if !v.vorschaubild.is_empty() {
                            Bild { src: v.vorschaubild.clone(), lazy: true }
                        }
                        if !v.dauer.is_empty() {
                            span { class: "dauer", "{v.dauer}" }
                        }
                    }
                    span { class: "innen",
                        span { class: "bt", Fett { text: v.titel.clone(), frage: frage.clone() } }
                        span { class: "quelle",
                            FaviconKreis { host: v.host.clone() }
                            "{v.host}"
                            if let Some(d) = v.datum.as_deref().filter(|d| !d.is_empty()) {
                                " · {datum_text(d)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---- Seitenzahlen ------------------------------------------------------------------------
// Wie bei Google: "nrzSeeeearch" mit einer Zahl unter jedem "e".

#[component]
fn Seitenzahlen(aktuell: u32, mehr: bool) -> Element {
    let mehr = mehr && aktuell < 50;
    if aktuell == 1 && !mehr {
        return rsx! {};
    }
    let erste = aktuell.saturating_sub(4).max(1);
    let letzte = if mehr { (erste + 9).min(50) } else { aktuell };
    let zurueck = (aktuell > 1).then(|| aktuell - 1);
    let weiter = mehr.then(|| aktuell + 1);
    rsx! {
        nav { class: "seiten", "aria-label": "Seitenzahlen",
            table {
                tbody {
                    tr {
                        td { class: "ende", SeitenEnde { text: "nrzS", seite: zurueck, wort: t("zurueck") } }
                        for s in erste..=letzte {
                            if s == aktuell {
                                td { key: "{s}", class: "aktiv",
                                    span { class: "buchstabe", "e" }
                                    span { "{s}" }
                                }
                            } else {
                                td { key: "{s}",
                                    a {
                                        href: "#",
                                        onclick: move |e: MouseEvent| {
                                            e.prevent_default();
                                            seite_wechseln(s);
                                        },
                                        span { class: "buchstabe", "e" }
                                        span { "{s}" }
                                    }
                                }
                            }
                        }
                        td { class: "ende", SeitenEnde { text: "arch", seite: weiter, wort: t("weiter") } }
                    }
                }
            }
        }
    }
}

#[component]
fn SeitenEnde(text: String, seite: Option<u32>, wort: String) -> Element {
    match seite {
        Some(s) => rsx! {
            a {
                href: "#",
                onclick: move |e: MouseEvent| {
                    e.prevent_default();
                    seite_wechseln(s);
                },
                span { class: "buchstabe", "{text}" }
                span { "{wort}" }
            }
        },
        None => rsx! { span { class: "buchstabe", "{text}" } },
    }
}

// ---- Am Ende: Korrekturen, Infoboxen, aehnliche Suchanfragen -----------------------------

/// Ein Link, der eine neue Suche startet.
#[component]
fn FrageLink(q: String, children: Element) -> Element {
    rsx! {
        a {
            href: frage_url(&q),
            "data-frage": "{q}",
            onclick: move |e: MouseEvent| {
                e.prevent_default();
                state::suchen(&q);
            },
            {children}
        }
    }
}

/// "Meintest du: …"
#[component]
fn Korrektur() -> Element {
    let d = SUCHE.read();
    let liste: Vec<String> = d.korrekturen.iter().filter_map(|k| k.as_str().map(String::from)).collect();
    if !d.fertig || liste.is_empty() || d.frage != ZUSTAND.read().q {
        return rsx! {};
    }
    rsx! {
        p { class: "korrekturen",
            "Meintest du: "
            for (n, k) in liste.into_iter().enumerate() {
                span { key: "{k}",
                    if n > 0 { " oder " }
                    FrageLink { q: k.clone(), b { i { "{k}" } } }
                }
            }
        }
    }
}

/// Direktantworten und Infoboxen der Instanz (wenn kein Modul die Seite hat).
#[component]
fn Karten(seite: Seite) -> Element {
    if !(seite.karten)() {
        return rsx! {};
    }
    let d = SUCHE.read();
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let antworten: Vec<&Value> = d.antworten.iter().filter(|a| !s(a, "answer").is_empty() || !s(a, "url").is_empty()).collect();
    let hinweise: Vec<&Value> = d
        .hinweise
        .iter()
        .filter(|i| !s(i, "img_src").is_empty() || i.get("urls").is_some_and(|u| !u.is_null()) || !s(i, "content").is_empty())
        .collect();
    rsx! {
        for (n, a) in antworten.into_iter().enumerate() {
            div { key: "a{n}", class: "karte sx-karte",
                div { class: "antwort", {s(a, "answer")} }
                if !s(a, "url").is_empty() {
                    p {
                        a { href: s(a, "url"), target: "_blank", rel: "noreferrer",
                            if s(a, "title").is_empty() { {s(a, "url")} } else { {s(a, "title")} }
                        }
                    }
                }
                if !s(a, "content").is_empty() {
                    p { {s(a, "content")} }
                }
            }
        }
        for (n, i) in hinweise.into_iter().enumerate() {
            div { key: "h{n}", class: "karte sx-karte",
                h2 { if s(i, "name").is_empty() { {s(i, "title")} } else { {s(i, "name")} } }
                if !s(i, "img_src").is_empty() {
                    img { class: "sx-bild", src: s(i, "img_src"), alt: "", loading: "lazy" }
                }
                p { {s(i, "content").chars().take(400).collect::<String>()} }
                for (m, u) in i.get("urls").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().take(4).enumerate() {
                    p { key: "{m}",
                        a { href: s(&u, "url"), target: "_blank", rel: "noreferrer",
                            if s(&u, "title").is_empty() { {s(&u, "url")} } else { {s(&u, "title")} }
                        }
                    }
                }
            }
        }
    }
}

/// "Aehnliche Suchanfragen" unter den Treffern - die neuen Woerter fett.
#[component]
fn Aehnlich(seite: Seite) -> Element {
    let liste = seite.aehnlich.read();
    if liste.is_empty() {
        return rsx! {};
    }
    let frage = SUCHE.read().frage.to_lowercase();
    let woerter: HashSet<&str> = frage.split_whitespace().collect();
    rsx! {
        h2 { {t("aehnlich")} }
        div { class: "aehnlich-liste",
            for v in liste.iter() {
                FrageLink { key: "{v}", q: v.clone(),
                    Lupe {}
                    span {
                        for (i, (wort, leer)) in woerter_mit_abstand(v).into_iter().enumerate() {
                            if leer || woerter.contains(wort.to_lowercase().as_str()) {
                                span { key: "{i}", "{wort}" }
                            } else {
                                b { key: "{i}", "{wort}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Zerlegt Text in Woerter und Abstaende (Abstand = true).
fn woerter_mit_abstand(text: &str) -> Vec<(String, bool)> {
    let mut aus: Vec<(String, bool)> = Vec::new();
    for c in text.chars() {
        let leer = c.is_whitespace();
        match aus.last_mut() {
            Some((s, l)) if *l == leer => s.push(c),
            _ => aus.push((c.to_string(), leer)),
        }
    }
    aus
}

// ---- Bildvorschau rechts und Lightbox ----------------------------------------------------

#[component]
fn VorschauPanel(seite: Seite, t: Treffer) -> Element {
    let mut gross = use_signal(|| None::<(String, String)>);
    let mut geteilt = use_signal(|| false);
    // Erst das Vorschaubild, dann das grosse Bild - fehlt es, bleibt das kleine.
    let setzen = use_hook(|| {
        im_scope(move |(bild, b, h): (String, u32, u32)| {
            if let Ok(mut g) = gross.try_write() {
                *g = Some((bild, format!("{b} × {h}")));
            }
        })
    });
    use_effect(use_reactive((&t.bild,), move |(bild,)| {
        geteilt.set(false);
        let Ok(img) = web_sys::HtmlImageElement::new() else { return };
        let (s, i2, b) = (setzen.clone(), img.clone(), bild.clone());
        let c = Closure::once_into_js(move || s((b, i2.natural_width(), i2.natural_height())));
        img.set_onload(Some(c.unchecked_ref()));
        img.set_src(&bild);
    }));

    let suche = SUCHE.read();
    let bilder = bilder_von(&suche);
    let nr = bilder.iter().position(|x| x.bild == t.bild);
    // Weitere Bilder: erst von derselben Seite, dann die Nachbarn im Raster.
    let weitere: Vec<(usize, Treffer)> = bilder
        .iter()
        .enumerate()
        .filter(|(_, x)| x.bild != t.bild && x.host == t.host)
        .chain(bilder.iter().enumerate().filter(|(_, x)| x.bild != t.bild && x.host != t.host))
        .take(6)
        .map(|(i, x)| (i, x.clone()))
        .collect();
    let (src, mass) = match gross.read().as_ref() {
        Some((b, m)) if *b == t.bild => (t.bild.clone(), m.clone()),
        _ => (if t.vorschaubild.is_empty() { t.bild.clone() } else { t.vorschaubild.clone() }, String::new()),
    };
    let stil = format!("--v:{:.3}", verhaeltnis(&t));
    let erstes = nr.is_none_or(|n| n == 0);
    let letztes = nr.is_none_or(|n| n + 1 >= bilder.len());
    let (titel, url) = (t.titel.clone(), t.url.clone());

    rsx! {
        div { class: "vs-kopf",
            span { class: "vs-quelle",
                FaviconKreis { host: t.host.clone() }
                "{t.host}"
            }
            button { r#type: "button", "aria-label": "Vorheriges Bild", disabled: erstes, onclick: move |_| seite.blaettern(-1), "‹" }
            button { r#type: "button", "aria-label": "Nächstes Bild", disabled: letztes, onclick: move |_| seite.blaettern(1), "›" }
            button {
                r#type: "button",
                "aria-label": "Schließen",
                onclick: move |_| {
                    let mut v = seite.vorschau;
                    v.set(None);
                },
                "×"
            }
        }
        a { class: "vs-bild", href: "{t.url}", target: "_blank", rel: "noreferrer", style: "{stil}",
            img { src: "{src}", alt: "{t.titel}" }
        }
        div { class: "vs-titel", "{t.titel}" }
        div { class: "vs-mass", "{mass}" }
        div { class: "vs-knoepfe",
            a { class: "vs-auf", href: "{t.url}", target: "_blank", rel: "noreferrer", "Aufrufen ›" }
            button {
                r#type: "button",
                onclick: move |_| teilen(titel.clone(), url.clone(), geteilt),
                if geteilt() { "Link kopiert" } else { "Teilen" }
            }
            a { href: "{t.bild}", target: "_blank", rel: "noreferrer", "Bild öffnen" }
        }
        if !weitere.is_empty() {
            div { class: "vs-weitere-titel", "Weitere Bilder" }
            div { class: "vs-weitere",
                for (i, x) in weitere {
                    a {
                        key: "{i}",
                        class: "bild",
                        href: "{x.url}",
                        "data-nr": "{i}",
                        onclick: {
                            let x = x.clone();
                            move |e: MouseEvent| {
                                e.prevent_default();
                                let mut v = seite.vorschau;
                                v.set(Some(x.clone()));
                            }
                        },
                        span { class: "rahmen",
                            Bild { src: if x.vorschaubild.is_empty() { x.bild.clone() } else { x.vorschaubild.clone() }, alt: x.titel.clone(), lazy: true }
                        }
                    }
                }
            }
        }
    }
}

/// Teilen ueber das System (wenn es das gibt), sonst den Link kopieren.
fn teilen(titel: String, url: String, mut geteilt: Signal<bool>) {
    let Some(nav) = web_sys::window().map(|w| w.navigator()) else { return };
    if js_sys::Reflect::has(&nav, &"share".into()).unwrap_or(false) {
        let daten = web_sys::ShareData::new();
        daten.set_title(&titel);
        daten.set_url(&url);
        let p = nav.share_with_data(&daten);
        spawn(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(p).await;
        });
    } else {
        spawn(async move {
            if kopieren(&url).await {
                geteilt.set(true);
            }
        });
    }
}

#[component]
fn Lightbox(seite: Seite) -> Element {
    let mut lb = seite.lightbox;
    let Some((video, url)) = lb.read().clone() else { return rsx! {} };
    rsx! {
        div {
            class: "lightbox",
            id: "lightbox",
            onclick: move |e: MouseEvent| {
                let auf_hintergrund = e
                    .data()
                    .downcast::<web_sys::MouseEvent>()
                    .and_then(|ev| ev.target())
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                    .is_some_and(|el| el.id() == "lightbox");
                if auf_hintergrund {
                    lb.set(None);
                }
            },
            button {
                class: "lightbox-zu",
                id: "lightbox-zu",
                "aria-label": "Schließen",
                onmounted: move |e| async move {
                    let _ = e.set_focus(true).await;
                },
                onclick: move |_| lb.set(None),
                "×"
            }
            div { class: "lightbox-inhalt", id: "lightbox-inhalt",
                iframe { src: "{video}", allowfullscreen: true, title: "Video" }
                div { class: "unten",
                    a { href: "{url}", target: "_blank", rel: "noreferrer", "Bei der Quelle öffnen" }
                }
            }
        }
    }
}
