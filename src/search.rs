// nrzSearch - die Suche.
//
// Die Suchmaschinen werden einzeln und gleichzeitig angefragt; ihre Treffer
// werden zu einer Rangliste zusammengefuehrt wie bei einer Metasuchmaschine:
// jeder Treffer bekommt Punkte nach Platz und Gewicht der Suchmaschine, und
// was mehrere Suchmaschinen weit oben haben, steht oben. Die erste Liste
// geht an den Browser, sobald eine der Kern-Suchmaschinen geantwortet hat;
// bis alle Kern-Suchmaschinen da sind, wird neu sortiert - danach steht die
// Reihenfolge fest, damit beim Lesen nichts springt.
//
// Es wird nichts gespeichert: kein Protokoll, keine Statistik, nur ein
// kurzer Cache im Arbeitsspeicher.

use crate::filter::{frage_gesperrt, ist_porn};
use crate::util::{bild_url, datum_von, eine_zeile, enc, host_und_pfad, liste, s};
use crate::AppState;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};

/// Die Kategorien, die das GUI anbietet.
const KATEGORIEN: &[&str] =
    &["general", "images", "videos", "news", "science", "files", "it", "food", "social media", "music", "gaming", "map"];

/// Wie viele Suchmaschinen gleichzeitig gefragt werden.
const GLEICHZEITIG: usize = 6;

/// Die Kern-Suchmaschinen: auf sie wird gewartet, bevor die Reihenfolge
/// feststeht (hoechstens bis FEST_NACH).
const KERN: &[&str] = &["duckduckgo", "brave", "bing", "google", "startpage", "qwant"];
/// Die erste Liste kommt spaetestens nach ERSTE_NACH, die Reihenfolge steht
/// spaetestens nach FEST_NACH fest.
const ERSTE_NACH: Duration = Duration::from_millis(1000);
const FEST_NACH: Duration = Duration::from_millis(2500);

/// Wie sehr eine Suchmaschine bei der Rangfolge zaehlt - nach Messungen an
/// der eigenen Instanz (deutsche Fragen): DuckDuckGo und Brave am besten,
/// Bing knapp dahinter. Yahoo und PrivacyWall liefern Bing-Treffer und
/// zaehlen weniger, sonst stimmte Bing dreifach ab; kleine Indizes wenig.
fn gewicht(engine: &str) -> f64 {
    match engine {
        "duckduckgo" | "brave" | "google" | "startpage" => 1.3,
        "bing" | "qwant" | "wikipedia" => 1.1,
        "yahoo" | "mojeek" | "presearch" => 0.7,
        "yandex" => 0.6,
        "privacywall" | "gmx" | "duckduckgo web" => 0.5,
        "yep" => 0.4,
        _ => 0.35,
    }
}

/// Fuellwoerter, die nichts ueber den Inhalt sagen (fuer die Passung).
const FUELLWOERTER: &[&str] = &[
    "der", "die", "das", "den", "dem", "des", "ein", "eine", "einen", "einem", "einer", "und", "oder", "aber", "mit",
    "von", "vom", "zum", "zur", "für", "fuer", "auf", "aus", "bei", "ist", "sind", "war", "wie", "was", "wer", "wann",
    "warum", "wieso", "welche", "welcher", "welches", "kann", "man", "ich", "nicht", "auch", "noch", "nach", "über",
    "the", "and", "for", "with", "what", "how", "who", "when", "why", "which", "does", "are", "was", "can", "you",
    "les", "des", "une", "est", "pour", "los", "las", "una", "por", "con", "para", "che", "per", "del", "della",
];

/// Die Suchwoerter (klein, ab drei Zeichen, ohne Fuellwoerter) - fuer die Pruefung, ob ein
/// Treffer ueberhaupt zur Frage passt. Gekuerzt auf fuenf Zeichen (einfacher Wortstamm).
fn suchwoerter(frage: &str) -> Vec<String> {
    let alle: Vec<String> = frage
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(String::from)
        .collect();
    let inhalt: Vec<&String> = alle.iter().filter(|w| !FUELLWOERTER.contains(&w.as_str())).collect();
    // Nur Fuellwoerter ("the who")? Dann doch alle.
    let liste = if inhalt.is_empty() { alle.iter().collect() } else { inhalt };
    let mut aus: Vec<String> = Vec::new();
    for w in liste {
        let stamm: String = w.chars().take(5).collect();
        if !aus.contains(&stamm) {
            aus.push(stamm);
        }
    }
    aus
}

/// Die Schrift eines Zeichens - grob, nur fuer den Vergleich mit der Sprache.
#[derive(PartialEq, Clone, Copy)]
enum Schrift {
    Latein,
    Kyrillisch,
    Arabisch,
    Cjk,
    Hangul,
    Devanagari,
    Thai,
    Andere,
}

fn schrift(c: char) -> Option<Schrift> {
    if !c.is_alphabetic() {
        return None;
    }
    Some(match c as u32 {
        0x0041..=0x024F | 0x1E00..=0x1EFF => Schrift::Latein,
        0x0400..=0x052F => Schrift::Kyrillisch,
        0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Schrift::Arabisch,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => Schrift::Cjk,
        0xAC00..=0xD7AF | 0x1100..=0x11FF => Schrift::Hangul,
        0x0900..=0x097F => Schrift::Devanagari,
        0x0E00..=0x0E7F => Schrift::Thai,
        _ => Schrift::Andere,
    })
}

/// Die Schrift, in der eine Sprache geschrieben wird (None = keine Pruefung).
fn schrift_der_sprache(sprache: &str) -> Option<Schrift> {
    Some(match sprache {
        "de" | "en" | "es" | "fr" | "pt" | "it" | "nl" | "pl" | "tr" | "id" | "vi" => Schrift::Latein,
        "ru" | "uk" => Schrift::Kyrillisch,
        "ar" | "ur" | "fa" => Schrift::Arabisch,
        "zh" | "ja" => Schrift::Cjk,
        "ko" => Schrift::Hangul,
        "hi" | "mr" => Schrift::Devanagari,
        "th" => Schrift::Thai,
        _ => return None,
    })
}

/// Anteil der Buchstaben in `text`, die NICHT in der Schrift `soll` stehen (0..1).
fn fremdanteil(text: &str, soll: Schrift) -> f64 {
    let (mut alle, mut fremd) = (0u32, 0u32);
    for c in text.chars() {
        if let Some(sch) = schrift(c) {
            alle += 1;
            if sch != soll {
                fremd += 1;
            }
        }
    }
    if alle < 4 { 0.0 } else { f64::from(fremd) / f64::from(alle) }
}

/// TLDs, unter denen besonders viel Spam, Betrug und Adult-Inhalt liegt.
const SPAM_TLDS: &[&str] = &["xyz", "top", "icu", "buzz", "click", "monster", "rest", "cfd", "sbs", "lol", "quest", "bond", "cyou", "work", "loan", "zip", "mov"];

/// Unsere Vorauswahl je Reiter, in der Reihenfolge, in der gefragt wird.
fn standard_engines(kategorie: &str) -> &'static [&'static str] {
    match kategorie {
        // Nur die Suchmaschinen, die auf der Instanz gute Treffer liefern -
        // gesperrte (Google, Qwant) und schwache (Mwmbl, Seznam, ...) nicht.
        // Weitere lassen sich unter "Tools" zuschalten.
        // Yandex fliegt raus: schwache sichere Suche (Adult-Treffer auch bei harmlosen
        // Fragen) und oft fremdsprachiger Spam.
        "general" => &["duckduckgo", "brave", "bing", "yahoo", "privacywall", "gmx", "mojeek"],
        "images" => &[
            "bing images", "brave.images", "duckduckgo images", "google images", "google cse images",
            "qwant images", "flickr", "unsplash", "pexels", "pixabay images", "wikicommons.images",
        ],
        "videos" => &[
            "bing videos", "brave.videos", "duckduckgo videos", "google videos", "qwant videos", "youtube",
            "vimeo", "dailymotion", "pixabay videos", "wikicommons.videos",
        ],
        "news" => &[
            "bing news", "brave.news", "duckduckgo news", "google news", "qwant news", "reuters", "wikinews",
            "tagesschau", "ansa", "il post", "naver news",
        ],
        "science" => &[
            "arxiv", "pubmed", "europepmc", "google scholar", "semantic scholar", "crossref", "openalex",
            "openairepublications", "openairedatasets", "pdbe",
        ],
        "files" => &[
            "1337x", "annas archive", "btdigg", "findfiles", "library genesis", "nyaa", "tokyotoshokan", "bt4g",
            "kickass", "piratebay", "solidtorrents", "wikicommons.files",
        ],
        "it" => &[
            "github", "gitlab", "codeberg", "stackoverflow", "superuser", "mdn", "hackernews", "npm", "pypi",
            "crates.io", "docker hub", "lib.rs", "arch linux wiki", "nixos wiki", "microsoft learn", "askubuntu",
            "gentoo", "hoogle", "mankier",
        ],
        "music" => &[
            "bandcamp", "soundcloud", "genius", "deezer", "yandex music", "mixcloud", "radio browser",
            "findfiles music", "wikicommons.audio", "youtube",
        ],
        _ => &[],
    }
}

/// Eine Suchmaschine der Instanz.
pub struct Engine {
    pub name: String,
    pub kurz: String,
    pub kategorien: Vec<String>,
    pub aktiv: bool,
}

/// Das Verzeichnis der Instanz (/config) - zehn Minuten gemerkt.
#[derive(Default)]
pub struct EngineVerzeichnis(Mutex<Option<(Arc<Vec<Engine>>, Instant)>>);

impl EngineVerzeichnis {
    pub async fn holen(&self, client: &reqwest::Client, sxng: &str) -> Arc<Vec<Engine>> {
        let mut cache = self.0.lock().await;
        if let Some((liste, zeit)) = cache.as_ref() {
            if zeit.elapsed() < Duration::from_secs(600) {
                return liste.clone();
            }
        }
        let roh = crate::util::hole(client.get(format!("{sxng}/config"))).await.unwrap_or_default();
        let engines: Vec<Engine> = liste(&roh, "engines")
            .iter()
            .filter(|e| !s(e, "name").is_empty())
            .map(|e| Engine {
                name: s(e, "name").to_string(),
                kurz: s(e, "shortcut").to_string(),
                kategorien: liste(e, "categories").iter().filter_map(|k| k.as_str().map(String::from)).collect(),
                aktiv: e.get("enabled").and_then(Value::as_bool).unwrap_or(false),
            })
            .collect();
        let engines = Arc::new(engines);
        if !engines.is_empty() {
            *cache = Some((engines.clone(), Instant::now()));
        }
        engines
    }
}

/// Die Engine-Auswahl: die des GUI, sonst unsere Vorauswahl, danach die
/// uebrigen aktiven Engines der Instanz in diesem Reiter - hoechstens 14.
fn engine_liste(engines: &[Engine], p: &SucheParams) -> Vec<String> {
    let wahl: Vec<String> = p.motoren.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
    if !wahl.is_empty() {
        return wahl;
    }
    let standard = standard_engines(&p.kategorie);
    // Im Reiter "Alle" nur die Auswahl - die uebrigen sind zu schwach.
    if p.kategorie == "general" {
        return standard.iter().map(|s| s.to_string()).collect();
    }
    // Die Bild-Datenbanken gehoeren zum Bilder-Reiter.
    let passt = |k: &String| *k == p.kategorie || (p.kategorie == "images" && k == "stock images");
    standard
        .iter()
        .map(|s| s.to_string())
        .chain(
            engines
                .iter()
                .filter(|e| e.aktiv && e.kategorien.iter().any(passt) && !standard.contains(&e.name.as_str()))
                .map(|e| e.name.clone()),
        )
        .take(14)
        .collect()
}

/// Die Parameter einer Suche.
pub struct SucheParams {
    pub q: String,
    pub kategorie: String,
    seite: i64,
    sprache: String,
    zeitraum: String,
    safesearch: String,
    motoren: String,
}

impl SucheParams {
    pub fn neu(mut m: HashMap<String, String>) -> Self {
        let mut hole = |k: &str| m.remove(k).unwrap_or_default();
        let (kategorie, sprache, zeitraum, safesearch) = (hole("kategorie"), hole("sprache"), hole("zeitraum"), hole("safesearch"));
        Self {
            q: hole("q"),
            kategorie: if KATEGORIEN.contains(&kategorie.as_str()) { kategorie } else { "general".into() },
            seite: hole("page").parse().unwrap_or(1).clamp(1, 50),
            // Ohne Angabe Deutsch - wie Google fuer jemanden in Deutschland.
            sprache: if !sprache.is_empty() && sprache.len() <= 8 && sprache.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
                sprache
            } else {
                "de".into()
            },
            zeitraum: if ["day", "week", "month", "year"].contains(&zeitraum.as_str()) { zeitraum } else { String::new() },
            // Immer streng: pornografische Inhalte will hier niemand sehen (siehe filter.rs).
            safesearch: { let _ = safesearch; "2".into() },
            motoren: hole("motoren"),
        }
    }

    /// Der Schluessel fuer den Cache (Frage klein und ohne doppelte Leerzeichen).
    fn schluessel(&self) -> String {
        let q = self.q.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        format!("{q}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}", self.kategorie, self.seite, self.sprache, self.zeitraum, self.motoren)
    }

    /// Der Anfang der Adresse fuer die Instanz (ohne die Suchmaschine).
    fn url(&self, sxng: &str) -> String {
        let mut url = format!(
            "{sxng}/search?format=json&q={}&pageno={}&language={}&safesearch={}",
            enc(&self.q),
            self.seite,
            enc(region(&self.sprache)),
            self.safesearch
        );
        if !self.zeitraum.is_empty() {
            url += &format!("&time_range={}", self.zeitraum);
        }
        url + "&engines="
    }
}

/// Die 25 meistgesprochenen Sprachen - mit der Region, mit der die Instanz
/// am passendsten sucht ("de" -> "de-DE").
pub const SPRACHEN: [(&str, &str); 25] = [
    ("en", "en-US"), ("zh", "zh-CN"), ("hi", "hi-IN"), ("es", "es-ES"), ("fr", "fr-FR"), ("ar", "ar-SA"), ("bn", "bn-BD"),
    ("pt", "pt-BR"), ("ru", "ru-RU"), ("ur", "ur-PK"), ("id", "id-ID"), ("de", "de-DE"), ("ja", "ja-JP"), ("mr", "mr-IN"),
    ("te", "te-IN"), ("tr", "tr-TR"), ("ta", "ta-IN"), ("vi", "vi-VN"), ("ko", "ko-KR"), ("it", "it-IT"), ("fa", "fa-IR"),
    ("pl", "pl-PL"), ("uk", "uk-UA"), ("nl", "nl-NL"), ("th", "th-TH"),
];

fn region(sprache: &str) -> &str {
    SPRACHEN.iter().find(|(s, _)| *s == sprache).map_or(sprache, |(_, r)| r)
}

/// Welche Sprache zeigt eine Adresse an? "en.wikipedia.org" -> en,
/// "example.com/fr/…" -> fr, "example.com/pt-br/…" -> pt. Sonst None.
fn adress_sprache(url: &str) -> Option<&'static str> {
    let u = reqwest::Url::parse(url).ok()?;
    let host = u.host_str()?.to_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let bekannt = |kandidat: &str| {
        let basis = kandidat.split(['-', '_']).next().unwrap_or("");
        SPRACHEN.iter().map(|(s, _)| *s).chain(["sv", "cs", "da", "fi", "no", "el", "he", "hu", "ro"]).find(|s| *s == basis)
    };
    // Subdomain: "en.wikipedia.org", "fr.m.wikipedia.org" (nicht "de.com").
    if labels.len() >= 3 {
        if let Some(s) = bekannt(labels[0]) {
            return Some(s);
        }
    }
    // Erster Pfadteil: "/en/", "/pt-br/" - "it" nicht (das ist oft Informatik).
    let teil = u.path_segments()?.next()?.to_lowercase();
    let form = teil.len() == 2 || (teil.len() == 5 && teil.as_bytes()[2] == b'-');
    if form && !teil.starts_with("it") {
        return bekannt(&teil);
    }
    None
}

/// Parameter, die nur der Verfolgung dienen - sie fliegen aus der Adresse.
fn verfolgung(name: &str) -> bool {
    name.starts_with("utm_") || ["msockid", "fbclid", "gclid", "ocid", "mc_cid", "mc_eid", "_hsenc", "_hsmi", "yclid"].contains(&name)
}

/// Die Adresse ohne Verfolgungs-Parameter und Anker - und ein Schluessel,
/// unter dem gleiche Seiten zusammenfallen (mit/ohne www., /, http/https).
fn adresse(url: &str) -> (String, String) {
    let Ok(mut u) = reqwest::Url::parse(url) else { return (url.to_string(), url.to_string()) };
    u.set_fragment(None);
    let mut paare: Vec<(String, String)> = u.query_pairs().filter(|(k, _)| !verfolgung(k)).map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    if paare.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut().clear().extend_pairs(&paare);
    }
    paare.sort();
    // Mobil- und Desktop-Fassung sind dieselbe Seite ("de.m.wikipedia.org", "m.example.com").
    let host = u.host_str().unwrap_or("").trim_start_matches("www.").trim_start_matches("m.").replace(".m.", ".").to_lowercase();
    let anfrage: Vec<String> = paare.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let schluessel = format!("{host}{}?{}", u.path().trim_end_matches('/'), anfrage.join("&"));
    (u.to_string(), schluessel)
}

/// Die Rangliste: Treffer aller Suchmaschinen, zusammengefuehrt.
#[derive(Default)]
struct Rangliste {
    /// Die Suchwoerter (gekuerzte Staemme, siehe suchwoerter).
    woerter: Vec<String>,
    /// Die gewaehlte Sprache ("de", "all" ...) - fremdsprachige Fassungen
    /// mehrsprachiger Seiten ("en.wikipedia.org", "/en/") fliegen raus.
    sprache: String,
    eintraege: Vec<(Value, f64, Vec<String>)>,
    index: HashMap<String, usize>,
    /// So viele Eintraege vorn stehen fest (die Reihenfolge, die der
    /// Browser schon zeigt).
    fest: usize,
    gefiltert: usize,
    /// Die ganze Frage, normiert (fuer den Bonus, wenn sie woertlich im Titel steht).
    frage: String,
    /// Die Inhaltswoerter in voller Laenge (fuer Navigations-Fragen: "spiegel" -> spiegel.de).
    ganze_woerter: Vec<String>,
    /// Die Schrift der gewaehlten Sprache - wenn die Frage selbst nichts anderes enthaelt.
    schrift: Option<Schrift>,
    /// Bilder/Videos: Filter strenger.
    streng: bool,
}

impl Rangliste {
    fn neu(frage: &str, sprache: &str) -> Self {
        let sprache = sprache.split('-').next().unwrap_or("").to_lowercase();
        let soll = schrift_der_sprache(&sprache);
        // Steht in der Frage selbst eine andere Schrift ("东京 wetter"), wird nicht geprueft.
        let schrift = soll.filter(|s| fremdanteil(frage, *s) == 0.0 || frage.chars().filter(|c| c.is_alphabetic()).count() < 4);
        let norm = crate::util::normieren(frage);
        Self {
            woerter: suchwoerter(frage),
            ganze_woerter: norm.split(' ').filter(|w| w.chars().count() >= 4 && !FUELLWOERTER.contains(w)).map(String::from).collect(),
            frage: norm,
            sprache,
            schrift,
            ..Default::default()
        }
    }

    fn mit_kategorie(mut self, kategorie: &str) -> Self {
        self.streng = matches!(kategorie, "images" | "videos");
        self
    }

    /// Ist das die Fassung einer Seite in einer anderen als der gewaehlten Sprache?
    fn fremd(&self, url: &str) -> bool {
        !self.sprache.is_empty() && self.sprache != "all" && adress_sprache(url).is_some_and(|s| s != self.sprache)
    }

    /// Passt der Treffer zur Frage? Je Suchwort: im Titel 1, in der Adresse 0,8, nur im Text
    /// 0,6. Ohne einen Treffer zaehlt er nur noch ein Viertel; steht die ganze Frage woertlich
    /// im Titel, ein Viertel mehr.
    fn passung(&self, t: &Value) -> f64 {
        if self.woerter.is_empty() {
            return 1.0;
        }
        let titel = s(t, "titel").to_lowercase();
        let adresse = s(t, "url").to_lowercase();
        let text = s(t, "text").to_lowercase();
        let summe: f64 = self
            .woerter
            .iter()
            .map(|w| {
                if titel.contains(w.as_str()) {
                    1.0
                } else if adresse.contains(w.as_str()) {
                    0.8
                } else if text.contains(w.as_str()) {
                    0.6
                } else {
                    0.0
                }
            })
            .sum();
        let mut wert = 0.25 + 0.75 * summe / self.woerter.len() as f64;
        if self.frage.contains(' ') && crate::util::normieren(&titel).contains(&self.frage) {
            wert *= 1.25;
        }
        wert
    }

    /// Ein Faktor fuer die Qualitaet der Quelle: offizielle Seite bei Navigations-Fragen hoch,
    /// Wikipedia etwas hoch, Spam-TLDs und Treffer in fremder Schrift tief.
    fn quelle(&self, t: &Value) -> f64 {
        let host = s(t, "host");
        let mut f = 1.0;
        let teile: Vec<&str> = host.split('.').collect();
        let name = if teile.len() >= 2 { teile[teile.len() - 2] } else { host };
        let tld = teile.last().copied().unwrap_or("");
        // "spiegel" -> spiegel.de, "deutsche bahn" -> bahn.de: die Startseite ganz nach oben.
        if self.ganze_woerter.iter().any(|w| w == name || w.replace(' ', "") == name) {
            let tiefe = reqwest::Url::parse(s(t, "url")).map(|u| u.path_segments().map_or(0, |p| p.filter(|x| !x.is_empty()).count())).unwrap_or(9);
            f *= if tiefe <= 1 { 1.8 } else { 1.15 };
        }
        if host.ends_with("wikipedia.org") {
            f *= 1.15;
        }
        if SPAM_TLDS.contains(&tld) {
            f *= 0.4;
        }
        if let Some(soll) = self.schrift {
            let anteil = fremdanteil(&format!("{} {}", s(t, "titel"), s(t, "text")), soll);
            if anteil > 0.5 {
                f *= 0.05;
            } else if anteil > 0.25 {
                f *= 0.4;
            }
        }
        f
    }

    /// Die Treffer einer Suchmaschine aufnehmen: Punkte nach Gewicht und
    /// Platz (1/Platz), gleiche Seiten fallen zusammen.
    fn aufnehmen(&mut self, engine: &str, treffer: &[Value], kategorie: &str, sxng: &str) {
        let w = gewicht(engine);
        for (i, r) in treffer.iter().enumerate() {
            // Der Porn-Filter: was nicht in die Liste gehoert, bleibt draussen.
            if ist_porn(r, self.streng) {
                self.gefiltert += 1;
                continue;
            }
            if s(r, "url").is_empty() || self.fremd(s(r, "url")) {
                continue;
            }
            let (url, schluessel) = adresse(s(r, "url"));
            let punkte = w / (i as f64 + 1.0);
            match self.index.get(&schluessel) {
                Some(&n) => {
                    let (t, score, engines) = &mut self.eintraege[n];
                    *score += punkte;
                    if !engines.iter().any(|e| e == engine) {
                        engines.push(engine.to_string());
                    }
                    // Der laengere Text gewinnt (manche liefern nur Schnipsel).
                    let text = eine_zeile(s(r, "content"));
                    if text.len() > s(t, "text").len() {
                        t["text"] = json!(text);
                    }
                    // Datum und Bild fehlen bei manchen (Bing News) - die anderen ergaenzen.
                    let neu = forme_treffer(r, kategorie, 0, sxng);
                    for feld in ["datum", "bild"] {
                        if t[feld].is_null() || t[feld] == "" {
                            t[feld] = neu[feld].clone();
                        }
                    }
                }
                None => {
                    let mut t = forme_treffer(r, kategorie, 0, sxng);
                    t["url"] = json!(url);
                    self.index.insert(schluessel, self.eintraege.len());
                    self.eintraege.push((t, punkte, vec![engine.to_string()]));
                }
            }
        }
    }

    /// Die Punkte zum Sortieren: wer bei mehreren Suchmaschinen steht, zaehlt
    /// mehr, wer nicht zur Frage passt, weniger.
    fn wert(&self, (t, score, engines): &(Value, f64, Vec<String>)) -> f64 {
        score * (1.0 + 0.25 * (engines.len() as f64 - 1.0)) * self.passung(t) * self.quelle(t)
    }

    /// Die Liste in Rangfolge - die festen Eintraege bleiben, wo sie sind.
    /// Wie bei Google stehen von einer Website hoechstens zwei Treffer
    /// direkt hintereinander oben, weitere ruecken nach unten.
    fn liste(&mut self) -> Vec<Value> {
        let fest = self.fest.min(self.eintraege.len());
        let mut werte: Vec<(f64, usize)> = (fest..self.eintraege.len()).map(|i| (self.wert(&self.eintraege[i]), i)).collect();
        werte.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut je_host: HashMap<String, i32> = HashMap::new();
        for i in 0..fest {
            *je_host.entry(s(&self.eintraege[i].0, "host").to_string()).or_default() += 1;
        }
        for (wert, i) in &mut werte {
            let zahl = je_host.entry(s(&self.eintraege[*i].0, "host").to_string()).or_default();
            *zahl += 1;
            if *zahl > 2 {
                *wert *= 0.5_f64.powi(*zahl - 2);
            }
        }
        werte.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut rest: Vec<Option<(Value, f64, Vec<String>)>> = self.eintraege.drain(fest..).map(Some).collect();
        let neu: Vec<_> = werte.iter().map(|(_, i)| rest[i - fest].take().expect("jeder Eintrag einmal")).collect();
        self.eintraege.extend(neu);
        self.index = self.eintraege.iter().enumerate().map(|(i, e)| (adresse(s(&e.0, "url")).1, i)).collect();
        self.eintraege
            .iter()
            .enumerate()
            .map(|(i, (t, score, engines))| {
                let mut t = t.clone();
                t["position"] = json!(i);
                t["engines"] = json!(engines);
                t["bewertung"] = json!((score * 1000.0).round() / 1000.0);
                t
            })
            .collect()
    }

    /// Ab jetzt steht die Reihenfolge der gezeigten Treffer fest.
    fn einfrieren(&mut self) {
        self.fest = self.eintraege.len();
    }
}

/// Einen Treffer in die Form bringen, die das GUI braucht.
fn forme_treffer(r: &Value, kategorie: &str, position: usize, sxng: &str) -> Value {
    let url = s(r, "url");
    let (host, pfad) = host_und_pfad(url);
    let datum = Some(s(r, "publishedDate")).filter(|d| !d.is_empty()).unwrap_or(s(r, "pubdate"));
    json!({
        "titel": r.get("title").and_then(Value::as_str).unwrap_or(url),
        "url": url,
        "host": host,
        "pfad": pfad,
        "text": eine_zeile(s(r, "content")),
        "kategorie": kategorie,
        "bild": bild_url(s(r, "img_src"), sxng),
        // Bilder: das kleine Vorschaubild der Suchmaschine (thumbnail_src) - viel schneller
        // geladen als das Original.
        "vorschaubild": bild_url(Some(s(r, "thumbnail_src")).filter(|t| !t.is_empty()).unwrap_or(s(r, "thumbnail")), sxng),
        "groesse": s(r, "resolution"),
        "autor": s(r, "author"),
        "dauer": s(r, "length"),
        "datum": datum_von(datum),
        "video": s(r, "iframe_src"),
        "engines": r.get("engines").cloned().unwrap_or(json!([])),
        "position": position,
        "bewertung": r.get("score").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

/// Die Suchmaschinen, die nicht geantwortet haben (mit Grund).
fn kaputte(roh: &Value) -> impl Iterator<Item = Value> + '_ {
    liste(roh, "unresponsive_engines").iter().filter_map(Value::as_array).map(|a| {
        json!({
            "engine": a.first().and_then(Value::as_str).unwrap_or(""),
            "grund": a.get(1).and_then(Value::as_str).unwrap_or("").chars().take(200).collect::<String>(),
        })
    })
}

/// Eine Suchmaschine fragen: die Dauer und die Antwort (None = Fehler).
async fn fragen(st: &AppState, basis: &str, engine: &str) -> (String, i64, Option<Value>) {
    let start = Instant::now();
    let anfrage = st.client.get(format!("{basis}{}", enc(engine))).header("accept-language", "de").timeout(st.engine_timeout);
    let roh = crate::util::hole(anfrage).await;
    let ms = if roh.is_some() { start.elapsed().as_millis() as i64 } else { 0 };
    (engine.to_string(), ms, roh)
}

/// Die Suche starten. Sie liefert einen Kanal, ueber den die Ereignisse
/// ("treffer", "fertig") laufen - schon als JSON-Text, so geht ein Cache-Treffer ohne
/// erneutes Umwandeln raus.
pub fn suche_starten(st: Arc<AppState>, p: SucheParams) -> mpsc::Receiver<(&'static str, Arc<str>)> {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        let start = Instant::now();
        let sxng = &st.instanz.bereit(Duration::from_secs(25)).await;
        let senden = |name: &'static str, wert: Value| {
            let tx = tx.clone();
            async move {
                let _ = tx.send((name, Arc::from(wert.to_string()))).await;
            }
        };

        // Eindeutig pornografische Fragen: keine Treffer, nur ein Hinweis.
        if frage_gesperrt(&p.q) {
            senden("fertig", json!({
                "frage": p.q, "seite": p.seite, "dauerMs": 0, "firstMs": 0, "anzahl": 0, "mehr": false,
                "korrekturen": [], "vorschlaege": [], "kaputte": [], "motorenAnzahl": 0, "instanz": sxng,
                "ausgefiltert": 0, "cache": false, "jugendschutz": true,
                "hinweis": "Für diese Suche zeigt nrzSearch keine Treffer (Jugendschutz-Filter).",
            })).await;
            return;
        }

        // Der Cache: dieselbe Frage noch einmal geht sofort (fertige JSON-Texte).
        let schluessel = p.schluessel();
        if let Some(gemerkt) = st.cache.lesen(&schluessel) {
            let _ = tx.send(("treffer", gemerkt.treffer.clone())).await;
            let _ = tx.send(("fertig", gemerkt.fertig.clone())).await;
            return;
        }

        let engines = st.verzeichnis.holen(&st.client, sxng).await;
        let namen = engine_liste(&engines, &p);
        let basis = p.url(sxng);

        let mut rang = Rangliste::neu(&p.q, &p.sprache).mit_kategorie(&p.kategorie);
        let (mut antworten, mut korrekturen, mut hinweise, mut vorschlaege, mut kaputt) = (vec![], vec![], vec![], vec![], vec![]);
        let mut erste_ms = 0;
        let anzahl_motoren = namen.len();
        let kern_gesamt = namen.iter().filter(|n| KERN.contains(&n.as_str())).count();
        let mut kern_da = 0;
        let (mut gesendet, mut eingefroren, mut frist_vorbei) = (false, false, false);
        let erste_frist = tokio::time::Instant::now() + ERSTE_NACH;
        let mut alle: Vec<Value> = Vec::new();

        // Die Kern-Suchmaschinen zuerst - sie laufen gleichzeitig los.
        let mut namen = namen;
        namen.sort_by_key(|n| !KERN.contains(&n.as_str()));
        let mut strom = futures_util::stream::iter(namen)
            .map(|e| {
                let (st, basis) = (st.clone(), basis.clone());
                async move { fragen(&st, &basis, &e).await }
            })
            .buffer_unordered(GLEICHZEITIG);
        loop {
            let (engine, dauer_ms, roh) = tokio::select! {
                antwort = strom.next() => match antwort {
                    Some(a) => a,
                    None => break,
                },
                // Spaetestens nach ERSTE_NACH kommt die erste Liste, auch
                // wenn noch keine Kern-Suchmaschine geantwortet hat.
                _ = tokio::time::sleep_until(erste_frist), if !gesendet && !frist_vorbei => {
                    frist_vorbei = true;
                    if !rang.eintraege.is_empty() {
                        alle = rang.liste();
                        erste_ms = start.elapsed().as_millis() as i64;
                        senden("treffer", json!({"treffer": alle, "ersetzen": true, "anzahl": alle.len()})).await;
                        gesendet = true;
                    }
                    continue;
                }
            };
            // Zu spaet fuer die Reihenfolge: was der Browser zeigt, bleibt stehen.
            if gesendet && !eingefroren && start.elapsed() >= FEST_NACH {
                rang.einfrieren();
                eingefroren = true;
            }
            let roh = roh.unwrap_or_default();
            rang.aufnehmen(&engine, liste(&roh, "results"), &p.kategorie, sxng);
            if KERN.contains(&engine.as_str()) {
                kern_da += 1;
            }
            let bereit = gesendet || kern_da > 0 || kern_gesamt == 0 || frist_vorbei;
            if bereit && !rang.eintraege.is_empty() {
                alle = rang.liste();
                if erste_ms == 0 {
                    erste_ms = start.elapsed().as_millis() as i64;
                }
                senden("treffer", json!({"treffer": alle, "ersetzen": true, "engine": engine, "dauerMs": dauer_ms, "anzahl": alle.len()})).await;
                gesendet = true;
            }
            if gesendet && !eingefroren && kern_da >= kern_gesamt {
                rang.einfrieren();
                eingefroren = true;
            }
            // Antworten, Korrekturen, Infoboxen und Vorschlaege nur einmal nehmen.
            for (ziel, k, max) in [(&mut antworten, "answers", usize::MAX), (&mut korrekturen, "corrections", usize::MAX), (&mut hinweise, "infoboxes", 3), (&mut vorschlaege, "suggestions", 6)] {
                if ziel.is_empty() {
                    // Auch Vorschlaege, Antworten und Infoboxen durch den Jugendschutz.
                    *ziel = liste(&roh, k).iter().filter(|v| crate::filter::punkte(&v.to_string()) < 2).take(max).cloned().collect();
                }
            }
            kaputt.extend(kaputte(&roh));
        }
        drop(strom);

        let dauer_ms = start.elapsed().as_millis() as i64;
        let erste_ms = if erste_ms == 0 { dauer_ms } else { erste_ms };
        let gefiltert = rang.gefiltert;
        let anzahl = alle.len();
        let fertig = json!({
            "frage": p.q, "seite": p.seite, "dauerMs": dauer_ms, "firstMs": erste_ms, "anzahl": anzahl, "mehr": anzahl > 0,
            "antworten": antworten, "hinweise": hinweise, "korrekturen": korrekturen, "vorschlaege": vorschlaege,
            "kaputte": kaputt, "motorenAnzahl": anzahl_motoren, "instanz": sxng, "ausgefiltert": gefiltert, "cache": false,
        });
        let fertig_text: Arc<str> = Arc::from(fertig.to_string());
        let _ = tx.send(("fertig", fertig_text.clone())).await;

        // Erst nach dem Ende fuer den Browser: in den Cache (nur mit Treffern - ein Ausfall der
        // Instanz soll nicht zehn Minuten lang haengen bleiben).
        if anzahl > 0 {
            let mut cache_fertig = fertig;
            cache_fertig["cache"] = json!(true);
            let treffer = json!({"treffer": alle, "ersetzen": true, "anzahl": anzahl, "cache": true}).to_string();
            st.cache.schreiben(schluessel, Gemerkt { treffer: Arc::from(treffer), fertig: Arc::from(cache_fertig.to_string()) });
        }
    });
    rx
}

/// Dieselbe Suche in einem Rutsch: die erste Suchmaschine, die antwortet.
pub async fn suche_einmal(st: &AppState, p: SucheParams) -> Option<Value> {
    if frage_gesperrt(&p.q) {
        return Some(json!({"frage": p.q, "seite": p.seite, "dauerMs": 0, "mehr": false, "treffer": [], "antworten": [],
            "korrekturen": [], "hinweise": [], "vorschlaege": [], "kaputte": [], "motorenAnzahl": 0, "instanz": "",
            "ausgefiltert": 0, "jugendschutz": true}));
    }
    let schluessel = format!("einmal|{}", p.schluessel());
    if let Some(v) = st.cache.einmal_lesen(&schluessel) {
        return Some((*v).clone());
    }
    let antwort = suche_einmal_frisch(st, p).await?;
    if !liste(&antwort, "treffer").is_empty() {
        st.cache.einmal_schreiben(schluessel, Arc::new(antwort.clone()));
    }
    Some(antwort)
}

async fn suche_einmal_frisch(st: &AppState, p: SucheParams) -> Option<Value> {
    let sxng = st.instanz.bereit(Duration::from_secs(25)).await;
    let engines = st.verzeichnis.holen(&st.client, &sxng).await;
    let namen = engine_liste(&engines, &p);
    let basis = p.url(&sxng);
    // Die ersten drei Suchmaschinen gleichzeitig, zusammengefuehrt.
    let antworten: Vec<(String, i64, Option<Value>)> =
        futures_util::future::join_all(namen.iter().take(3).map(|e| fragen(st, &basis, e))).await;
    let mut rang = Rangliste::neu(&p.q, &p.sprache).mit_kategorie(&p.kategorie);
    let mut dauer_ms = 0;
    let mut benutzt = Vec::new();
    for (engine, ms, roh) in &antworten {
        if let Some(roh) = roh {
            rang.aufnehmen(engine, liste(roh, "results"), &p.kategorie, &sxng);
            dauer_ms = dauer_ms.max(*ms);
            benutzt.push(engine.clone());
        }
    }
    let roh = antworten.iter().find_map(|(_, _, r)| r.clone())?;
    let treffer = rang.liste();
    let gefiltert = rang.gefiltert;

    let erste = |k: &str, n: usize| liste(&roh, k).iter().take(n).cloned().collect::<Vec<_>>();
    Some(json!({
        "frage": roh.get("query").and_then(Value::as_str).unwrap_or(&p.q),
        "seite": p.seite,
        "dauerMs": dauer_ms,
        "mehr": !treffer.is_empty(),
        "treffer": treffer,
        "antworten": erste("answers", usize::MAX),
        "korrekturen": erste("corrections", usize::MAX),
        "hinweise": erste("infoboxes", 3),
        "vorschlaege": erste("suggestions", 6),
        "kaputte": kaputte(&roh).collect::<Vec<_>>(),
        "motorenAnzahl": namen.len(),
        "instanz": benutzt.join(","),
        "ausgefiltert": gefiltert,
    }))
}

/// Eine gemerkte Suche: die beiden Ereignisse als fertiger JSON-Text.
#[derive(Clone)]
pub struct Gemerkt {
    treffer: Arc<str>,
    fertig: Arc<str>,
}

/// Ein kleiner LRU-Speicher mit Ablaufzeit (nur im Arbeitsspeicher). Beim Ueberlaufen fliegen
/// die aeltesten Eintraege raus - nicht alles auf einmal.
pub struct Lru<V> {
    karte: HashMap<String, (V, Instant, u64)>,
    zaehler: u64,
    max: usize,
    dauer: Duration,
}

impl<V: Clone> Lru<V> {
    pub fn neu(max: usize, dauer: Duration) -> Self {
        Self { karte: HashMap::with_capacity(max.min(1024)), zaehler: 0, max, dauer }
    }

    pub fn lesen(&mut self, k: &str) -> Option<V> {
        self.zaehler += 1;
        let jetzt = self.zaehler;
        match self.karte.get_mut(k) {
            Some((v, zeit, benutzt)) if zeit.elapsed() < self.dauer => {
                *benutzt = jetzt;
                Some(v.clone())
            }
            Some(_) => {
                self.karte.remove(k);
                None
            }
            None => None,
        }
    }

    pub fn schreiben(&mut self, k: String, v: V) {
        self.zaehler += 1;
        if self.karte.len() >= self.max && !self.karte.contains_key(&k) {
            let dauer = self.dauer;
            self.karte.retain(|_, (_, zeit, _)| zeit.elapsed() < dauer);
            if self.karte.len() >= self.max {
                // Das am laengsten nicht benutzte Achtel raus.
                let mut alter: Vec<u64> = self.karte.values().map(|(_, _, b)| *b).collect();
                alter.sort_unstable();
                let grenze = alter[(alter.len() / 8).max(1).min(alter.len() - 1)];
                self.karte.retain(|_, (_, _, b)| *b > grenze);
            }
        }
        self.karte.insert(k, (v, Instant::now(), self.zaehler));
    }
}

/// Der Cache der Suchen: zehn Minuten, nur im Arbeitsspeicher, hoechstens 2000 Antworten
/// (Strom) und 2000 Suchen in einem Rutsch (fuer Dieter).
pub struct Cache {
    strom: std::sync::Mutex<Lru<Gemerkt>>,
    einmal: std::sync::Mutex<Lru<Arc<Value>>>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            strom: std::sync::Mutex::new(Lru::neu(2000, Duration::from_secs(600))),
            einmal: std::sync::Mutex::new(Lru::neu(2000, Duration::from_secs(600))),
        }
    }
}

impl Cache {
    fn lesen(&self, schluessel: &str) -> Option<Gemerkt> {
        self.strom.lock().ok()?.lesen(schluessel)
    }

    fn schreiben(&self, schluessel: String, antwort: Gemerkt) {
        if let Ok(mut c) = self.strom.lock() {
            c.schreiben(schluessel, antwort);
        }
    }

    fn einmal_lesen(&self, schluessel: &str) -> Option<Arc<Value>> {
        self.einmal.lock().ok()?.lesen(schluessel)
    }

    fn einmal_schreiben(&self, schluessel: String, antwort: Arc<Value>) {
        if let Ok(mut c) = self.einmal.lock() {
            c.schreiben(schluessel, antwort);
        }
    }
}
