//! Modul "kurs" - Portierung von public/module/kurs.js.
//!
//! Fragen wie "1 btc in eur", "100 eur in usd" oder "apple kurs" bekommen eine Kurskarte wie bei
//! Google Finance: grosser Kurs, Aenderung, Verlauf mit Fadenkreuz, Zeitraum-Reiter und ein
//! Waehrungsrechner. Die Kurse kommen vom Server (/api/kurs). Die Karte erscheint nur, wenn
//! wirklich ein Kurs da ist.

// Solange der Kern module::starten noch nicht aufruft, gilt hier alles als unbenutzt.
#![allow(dead_code)]

use super::ModulCtx;
use crate::werkzeug::{self, zahl};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::JsValue;

// ---- Tabellen ----------------------------------------------------------------------------

const KRYPTO: &[(&str, &str)] = &[
    ("btc", "BTC"), ("xbt", "BTC"), ("bitcoin", "BTC"), ("eth", "ETH"), ("ethereum", "ETH"), ("ltc", "LTC"), ("litecoin", "LTC"),
    ("xrp", "XRP"), ("ripple", "XRP"), ("sol", "SOL"), ("solana", "SOL"), ("doge", "DOGE"), ("dogecoin", "DOGE"), ("ada", "ADA"),
    ("cardano", "ADA"), ("bnb", "BNB"), ("dot", "DOT"), ("polkadot", "DOT"), ("avax", "AVAX"), ("link", "LINK"), ("trx", "TRX"),
];
const AKTIEN: &[(&str, &str)] = &[
    ("apple", "AAPL"), ("microsoft", "MSFT"), ("msft", "MSFT"), ("tesla", "TSLA"), ("nvidia", "NVDA"), ("amazon", "AMZN"), ("meta", "META"),
    ("facebook", "META"), ("netflix", "NFLX"), ("google", "GOOGL"), ("alphabet", "GOOGL"), ("intel", "INTC"), ("amd", "AMD"), ("ibm", "IBM"),
    ("oracle", "ORCL"), ("paypal", "PYPL"), ("coinbase", "COIN"), ("palantir", "PLTR"), ("palantir technologies", "PLTR"),
    ("snowflake", "SNOW"), ("datadog", "DDOG"), ("crowdstrike", "CRWD"), ("airbnb", "ABNB"), ("uber", "UBER"), ("salesforce", "CRM"),
    ("adobe", "ADBE"), ("qualcomm", "QCOM"), ("broadcom", "AVGO"), ("siemens", "SIE.DE"), ("bmw", "BMW.DE"), ("mercedes", "MBG.DE"),
    ("deutsche telekom", "DTE.DE"), ("deutsche b\u{f6}rse", "DB1.DE"), ("allianz", "ALV.DE"), ("lvmh", "MC.PA"),
    ("total energies", "TTE.PA"), ("sap", "SAP.DE"), ("adidas", "ADS.DE"), ("volkswagen", "VOW3.DE"), ("deutschebank", "DBK.DE"),
];
/// Indizes (mit "^") und Rohstoffe (mit "=F") - die Symbole von Yahoo.
const MARKTDATEN: &[(&str, &str)] = &[
    ("dax", "^GDAXI"), ("dax 40", "^GDAXI"), ("mdax", "^MDAXI"), ("sdax", "^SDAXI"), ("ftse", "^FTSE"), ("ftse 100", "^FTSE"),
    ("ftse all world", "AW01.FGI"), ("ftse all-world", "AW01.FGI"), ("ftse welt", "AW01.FGI"), ("ftse world", "AW01.FGI"),
    ("dow jones", "^DJI"), ("dow", "^DJI"), ("sp 500", "^GSPC"), ("s&p 500", "^GSPC"), ("sp500", "^GSPC"), ("s&p500", "^GSPC"),
    ("nasdaq", "^IXIC"), ("nasdaq 100", "^NDX"), ("nasdaq composite", "^IXIC"), ("euro stoxx 50", "^STOXX50E"),
    ("eurostoxx 50", "^STOXX50E"), ("stoxx 50", "^STOXX50E"), ("euro stoxx", "^STOXX50E"), ("nikkei", "^N225"),
    ("nikkei 225", "^N225"), ("hang seng", "^HSI"), ("cac 40", "^FCHI"), ("cac40", "^FCHI"), ("smi", "^SSMI"), ("ibex", "^IBEX"),
    ("vix", "^VIX"), ("volatilitaetsindex", "^VIX"), ("msci world", "^990100-USD-STRD"), ("msci welt", "^990100-USD-STRD"),
    ("russell 2000", "^RUT"), ("sensex", "^BSESN"), ("shanghai", "^SSEC"),
    ("gold", "GC=F"), ("goldpreis", "GC=F"), ("gold price", "GC=F"), ("silber", "SI=F"), ("silberpreis", "SI=F"), ("kupfer", "HG=F"),
    ("kupferpreis", "HG=F"), ("platin", "PL=F"), ("palladium", "PA=F"), ("rohoel", "CL=F"), ("erdoel", "CL=F"), ("roh\u{f6}l", "CL=F"),
    ("erd\u{f6}l", "CL=F"), ("wti", "CL=F"), ("brent", "BZ=F"), ("erdgas", "NG=F"), ("weizen", "ZW=F"), ("mais", "ZC=F"), ("soja", "ZS=F"),
    ("kaffee", "KC=F"), ("kakao", "CC=F"), ("zucker", "SB=F"), ("baumwolle", "CT=F"), ("aluminium", "ALI=F"), ("heizoel", "HO=F"), ("benzin", "RB=F"),
];
const FIAT: &[&str] = &[
    "USD", "EUR", "GBP", "JPY", "CHF", "AUD", "CAD", "CNY", "SEK", "NOK", "DKK", "PLN", "INR", "KRW",
    "BRL", "NZD", "MXN", "ZAR", "SGD", "HKD", "TRY", "AED", "ILS", "THB", "CZK", "HUF", "RON", "BGN", "ISK", "CLP",
    "COP", "ARS", "IDR", "MYR", "PHP", "VND", "TWD",
];
const WAEHRUNG_NAMEN: &[(&str, &str)] = &[
    ("euro", "EUR"), ("dollar", "USD"), ("pfund", "GBP"), ("yen", "JPY"), ("yuan", "CNY"), ("renminbi", "CNY"), ("franken", "CHF"), ("rand", "ZAR"), ("real", "BRL"),
];
/// Woerter, die nichts mit dem Kurs selbst zu tun haben.
const KURS_STOPP: &[&str] = &[
    "kurs", "kurse", "kursentwicklung", "preis", "preise", "preisentwicklung", "price",
    "heute", "jetzt", "aktuell", "aktuelle", "aktueller", "von", "der", "die", "das", "den", "dem", "und", "im",
    "am", "f\u{fc}r", "fuer", "ist", "wie", "viel", "bitte", "kurz", "aktie", "aktien", "aktienkurs", "woert", "cours", "course",
];
const KRYPTO_KUERZEL: &[&str] = &["BTC", "ETH", "LTC", "XRP", "SOL", "DOGE", "ADA", "BNB", "DOT", "AVAX", "LINK", "TRX"];
const KURS_BEREICHE: &[(&str, &str)] = &[("1d", "1 T."), ("5d", "5 T."), ("1mo", "1 M."), ("6mo", "6 M."), ("ytd", "YTD"), ("1y", "1 J."), ("5y", "5 J."), ("max", "Max.")];

fn suchen(tabelle: &[(&str, &'static str)], wort: &str) -> Option<&'static str> {
    tabelle.iter().find(|(k, _)| *k == wort).map(|(_, v)| *v)
}

/// Alle Waehrungen des Rechners (Krypto und Fiat).
fn kurs_waehrungen() -> impl Iterator<Item = &'static str> {
    KRYPTO_KUERZEL.iter().chain(FIAT.iter()).copied()
}

// ---- Kursanfrage -------------------------------------------------------------------------

/// Eine Kursanfrage aus der Suchfrage (wie `kursAnfrage` in kurs.js).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KursAnfrage {
    pub symbol: String,
    pub betrag: f64,
    pub umgekehrt: bool,
    /// Nur der Firmenname ("apple") - dann zeigt die Info-Karte den Kurs.
    pub nur_name: bool,
}

/// Wechselkurs wie "EURUSD=X"?
fn ist_wechselkurs(symbol: &str) -> bool {
    symbol.len() == 8 && symbol.ends_with("=X") && symbol[..6].bytes().all(|b| b.is_ascii_uppercase())
}

/// Waehrungspaare starten mit einem Monat (die EZB liefert nur Tageskurse), sonst ein Tag.
fn standard_bereich(symbol: &str) -> &'static str {
    if ist_wechselkurs(symbol) { "1mo" } else { "1d" }
}

/// Der Zeitraum aus dem Suchfilter bestimmt den Startverlauf.
fn bereich_aus_zeitraum(symbol: &str, zeitraum: &str) -> &'static str {
    match zeitraum {
        "day" => "1d",
        "week" => "5d",
        "month" => "1mo",
        "year" => "1y",
        _ => standard_bereich(symbol),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Art {
    Markt,
    Krypto,
    Aktie,
    Fiat,
}

#[derive(Clone, Copy)]
struct Teil {
    sym: &'static str,
    art: Art,
}

/// Ein Wort nachschlagen: Marktdaten, Krypto, Aktie oder Waehrung.
fn kurs_nachschlagen(wort: &str) -> Option<Teil> {
    if let Some(sym) = suchen(MARKTDATEN, wort) {
        return Some(Teil { sym, art: Art::Markt });
    }
    if let Some(sym) = suchen(KRYPTO, wort) {
        return Some(Teil { sym, art: Art::Krypto });
    }
    if let Some(sym) = suchen(AKTIEN, wort) {
        return Some(Teil { sym, art: Art::Aktie });
    }
    let f = match suchen(WAEHRUNG_NAMEN, wort) {
        Some(f) => f.to_string(),
        None if wort.len() == 3 && wort.bytes().all(|b| b.is_ascii_lowercase()) => wort.to_ascii_uppercase(),
        None => String::new(),
    };
    FIAT.iter().find(|w| **w == f).map(|sym| Teil { sym, art: Art::Fiat })
}

/// Aus zwei Teilen das Kurs-Symbol bauen (oder None).
fn kurs_paar(von: Teil, nach: Teil, betrag: f64) -> Option<KursAnfrage> {
    use Art::*;
    let symbol = match (von.art, nach.art) {
        (Krypto, Fiat) => format!("{}-{}", von.sym, nach.sym),
        (Fiat, Krypto) => format!("{}-{}", nach.sym, von.sym),
        (Fiat, Fiat) => format!("{}{}=X", von.sym, nach.sym),
        (Krypto, Krypto) => format!("{}-{}=X", von.sym, nach.sym),
        (Aktie, Fiat) => von.sym.to_string(),
        // "100 gold in eur" - der Rohstoff hat seinen eigenen Kurs, also eine Einheit.
        (Markt, Fiat) => von.sym.to_string(),
        (Fiat, Markt) => nach.sym.to_string(),
        _ => return None,
    };
    let markt = von.art == Markt || nach.art == Markt;
    // "eur in eth" wird als ETH-EUR gehandelt (Kurs pro ETH) - merken.
    let normal = symbol.starts_with(&format!("{}-", von.sym)) || symbol == format!("{}{}=X", von.sym, nach.sym) || symbol == von.sym;
    Some(KursAnfrage { symbol, betrag: if markt { 1.0 } else { betrag }, umgekehrt: !normal, nur_name: false })
}

/// Der Betrag am Anfang der Frage ("100,5 eur in usd" -> 100.5).
fn betrag_vorne(roh: &str) -> Option<f64> {
    let ganz: String = roh.chars().take_while(char::is_ascii_digit).collect();
    if ganz.is_empty() {
        return None;
    }
    let rest = &roh[ganz.len()..];
    let mut text = ganz;
    let mut zeichen = rest.chars();
    if let Some('.' | ',') = zeichen.next() {
        let nach: String = zeichen.take_while(char::is_ascii_digit).collect();
        if !nach.is_empty() {
            text.push('.');
            text.push_str(&nach);
        }
    }
    text.parse().ok()
}

/// "1 btc in eur" -> BTC-EUR, "apple kurs" -> AAPL, "gold" -> GC=F, sonst None.
pub fn kurs_anfrage(frage: &str) -> Option<KursAnfrage> {
    let roh = frage.trim();
    let klein = roh.to_lowercase();
    let woerter: Vec<&str> = klein.split(|c: char| c.is_whitespace() || ",!?;:".contains(c)).filter(|w| !w.is_empty()).collect();
    if woerter.is_empty() {
        return None;
    }
    let betrag = betrag_vorne(roh).unwrap_or(1.0);

    // Muster 1: "X in Y"
    if let Some(i) = woerter.iter().position(|w| *w == "in") {
        if i > 0 && i + 1 < woerter.len() {
            if let (Some(von), Some(nach)) = (kurs_nachschlagen(woerter[i - 1]), kurs_nachschlagen(woerter[i + 1])) {
                if von.sym != nach.sym {
                    return kurs_paar(von, nach, betrag);
                }
            }
        }
    }
    // Muster 2: Mehrwort-Namen ("ftse all world", "dow jones") - laengere zuerst.
    for n in (1..=3).rev() {
        for j in 0..woerter.len().saturating_sub(n - 1) {
            if j + n > woerter.len() {
                break;
            }
            if let Some(markt) = suchen(MARKTDATEN, &woerter[j..j + n].join(" ")) {
                return Some(KursAnfrage { symbol: markt.into(), betrag: 1.0, umgekehrt: false, nur_name: false });
            }
        }
    }
    // Muster 3: "bitcoin kurs", "apple aktie", "kurs von tesla". Steht nur der Firmenname da
    // ("apple"), ist es "nur_name" - dann zeigt die Info-Karte den Kurs rechts.
    let stopp = |w: &str| KURS_STOPP.contains(&w);
    let nach_kurs_gefragt = woerter.iter().any(|w| stopp(w));
    for w in &woerter {
        if stopp(w) {
            continue;
        }
        let Some(t) = kurs_nachschlagen(w) else { continue };
        return match t.art {
            Art::Krypto => kurs_paar(t, Teil { sym: "EUR", art: Art::Fiat }, betrag),
            Art::Aktie | Art::Markt => Some(KursAnfrage {
                symbol: t.sym.into(),
                betrag: 1.0,
                umgekehrt: false,
                nur_name: t.art == Art::Aktie && !nach_kurs_gefragt,
            }),
            // "euro kurs" allein ist zu mehrdeutig
            Art::Fiat => None,
        };
    }
    None
}

/// Aus "BTC-EUR" das Waehrungspaar fuer den Rechner machen (bei Aktien None).
/// Rohstoffe wie XAUUSD=X sehen zwar aus wie ein Paar, haben aber keinen Rechner.
fn paar_von_symbol(symbol: &str) -> Option<(String, String)> {
    if ist_wechselkurs(symbol) {
        let (a, b) = (&symbol[..3], &symbol[3..6]);
        return (FIAT.contains(&a) && FIAT.contains(&b)).then(|| (a.to_string(), b.to_string()));
    }
    let kern = symbol.strip_suffix("=X").unwrap_or(symbol);
    let (a, b) = kern.split_once('-')?;
    let gut = |t: &str| (1..=7).contains(&t.len()) && t.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    (gut(a) && gut(b)).then(|| (a.to_string(), b.to_string()))
}

/// Nachkommastellen fuer einen Kurs (Wechselkurse 4, sonst je nach Groesse).
pub fn kurs_stellen(n: f64, symbol: &str) -> usize {
    let a = n.abs();
    if symbol.ends_with("=X") { if a >= 0.01 { 4 } else { 6 } } else if a >= 1.0 { 2 } else if a >= 0.01 { 4 } else { 6 }
}

/// Kursdaten von /api/kurs (None bei Fehler).
pub async fn kurs_holen(symbol: &str, bereich: &str) -> Option<Value> {
    crate::api::get_oder_nichts("/api/kurs", &[("symbol", symbol), ("bereich", bereich)]).await
}

/// Eine Zahl aus einem Eingabefeld wie `parseFloat` (Leerzeichen raus, Komma als Punkt). NaN wenn leer.
fn zahl_aus_feld(text: &str) -> f64 {
    let roh: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let roh = roh.replacen(',', ".", 1);
    let vorne: String = roh.chars().take_while(|c| c.is_ascii_digit() || "+-.eE".contains(*c)).collect();
    (1..=vorne.len()).rev().find_map(|n| vorne[..n].parse::<f64>().ok()).unwrap_or(f64::NAN)
}

// ---- Zeit --------------------------------------------------------------------------------

/// Ein Zeitpunkt (ms) mit Intl.DateTimeFormat("de-DE", optionen).
fn zeit_text(ms: f64, optionen: &[(&str, &str)]) -> String {
    let orte = js_sys::Array::of1(&JsValue::from_str("de-DE"));
    let opt = js_sys::Object::new();
    for (k, v) in optionen {
        let _ = js_sys::Reflect::set(&opt, &JsValue::from_str(k), &JsValue::from_str(v));
    }
    let fmt = js_sys::Intl::DateTimeFormat::new(&orte, &opt);
    fmt.format()
        .call1(&JsValue::NULL, &js_sys::Date::new(&JsValue::from_f64(ms)))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

// ---- Chart -------------------------------------------------------------------------------

/// Die Lage der Verlaufspunkte im SVG (640 x 160).
const CHART_B: f64 = 640.0;
const CHART_H: f64 = 160.0;

struct Geometrie {
    n: usize,
    min: f64,
    spanne: f64,
}

impl Geometrie {
    fn neu(verlauf: &[(f64, f64)]) -> Self {
        let min = verlauf.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max = verlauf.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        let spanne = if max - min == 0.0 { 1.0 } else { max - min };
        Self { n: verlauf.len(), min, spanne }
    }
    fn x(&self, i: usize) -> f64 {
        i as f64 / (self.n - 1) as f64 * CHART_B
    }
    fn y(&self, wert: f64) -> f64 {
        CHART_H - 14.0 - (wert - self.min) / self.spanne * (CHART_H - 28.0)
    }
    fn drin(&self, wert: f64) -> bool {
        wert >= self.min && wert <= self.min + self.spanne
    }
}

/// Der Verlauf als Liste (Zeitstempel in s, Wert).
fn verlauf_von(daten: &Value) -> Vec<(f64, f64)> {
    daten["verlauf"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
        .collect()
}

/// Mehr als drei Tage Verlauf: Tagesachsen statt Uhrzeiten.
fn ist_taeglich(verlauf: &[(f64, f64)]) -> bool {
    verlauf.last().map(|l| l.0).unwrap_or(0.0) - verlauf.first().map(|f| f.0).unwrap_or(0.0) > 3.0 * 86400.0
}

/// Die Zeit eines Verlaufspunkts fuer Tooltip und Kopf.
fn hover_zeit(stempel: f64, taeglich: bool) -> String {
    if taeglich {
        zeit_text(stempel * 1000.0, &[("day", "numeric"), ("month", "short"), ("year", "numeric")])
    } else {
        format!("{} Uhr", zeit_text(stempel * 1000.0, &[("day", "numeric"), ("month", "short"), ("hour", "2-digit"), ("minute", "2-digit")]))
    }
}

/// Die Aenderung gegenueber dem Vergleichswert: (hoch, "+1,23 (0,45 %)").
fn kurs_delta(preis: f64, basis: f64, stellen: usize) -> (bool, String) {
    let delta = preis - basis;
    let hoch = delta >= 0.0;
    let prozent = if basis != 0.0 { (preis / basis - 1.0) * 100.0 } else { 0.0 };
    (hoch, format!("{}{} ({} %)", if hoch { "+" } else { "" }, zahl(delta, stellen), zahl(prozent, 2)))
}

/// Der Vergleichswert fuer den Hover (Vortagsschluss oder erster Punkt).
fn hover_basis(daten: &Value, verlauf: &[(f64, f64)]) -> f64 {
    daten["vorher"].as_f64().unwrap_or_else(|| verlauf.first().map(|p| p.1).unwrap_or(0.0))
}

/// Die Client-Position (x) unter dem Zeiger in einen Verlaufsindex umrechnen.
fn index_bei(box_el: &Option<web_sys::Element>, client_x: f64, n: usize) -> Option<usize> {
    let rect = box_el.as_ref()?.get_bounding_client_rect();
    if rect.width() <= 0.0 || n < 2 {
        return None;
    }
    let anteil = ((client_x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    Some((anteil * (n - 1) as f64).round() as usize)
}

/// Verlauf als SVG-Linie mit Farbverlauf, Vortagsschluss-Linie und Fadenkreuz. Beim Ueberfahren
/// zeigt ein Tooltip Zeit, Wert und Aenderung. Mit `hover` folgt auch der Kopf der Karte.
#[component]
pub fn KursChart(daten: Value, #[props(default)] hover: Option<Signal<Option<usize>>>) -> Element {
    let intern = use_signal(|| None::<usize>);
    let mut hover = hover.unwrap_or(intern);
    let mut box_el = use_signal(|| None::<web_sys::Element>);
    let verlauf = verlauf_von(&daten);
    if verlauf.len() < 2 {
        return rsx! {};
    }
    let n = verlauf.len();
    let g = Geometrie::neu(&verlauf);
    let punkte = verlauf.iter().enumerate().map(|(i, p)| format!("{:.1},{:.1}", g.x(i), g.y(p.1))).collect::<Vec<_>>().join(" ");
    let flaeche = format!("0,{CHART_H} {punkte} {CHART_B},{CHART_H}");
    let vorher = daten["vorher"].as_f64().filter(|v| g.drin(*v)).map(|v| format!("{:.1}", g.y(v)));
    let taeglich = ist_taeglich(&verlauf);
    let achse: &[(&str, &str)] = if taeglich { &[("day", "2-digit"), ("month", "2-digit")] } else { &[("hour", "2-digit"), ("minute", "2-digit")] };
    let zeiten: Vec<String> = [0, n / 3, 2 * n / 3, n - 1].iter().map(|&i| zeit_text(verlauf[i].0 * 1000.0, achse)).collect();

    // Fadenkreuz und Tooltip
    let stellen = kurs_stellen(daten["kurs"].as_f64().unwrap_or(0.0), daten["symbol"].as_str().unwrap_or(""));
    let basis = hover_basis(&daten, &verlauf);
    let waehrung = daten["waehrung"].as_str().unwrap_or("").to_string();
    let aktiv = hover().filter(|i| *i < n);
    let (hx, hy) = aktiv.map(|i| (g.x(i), g.y(verlauf[i].1))).unwrap_or((0.0, 0.0));
    let tipp = aktiv.map(|i| {
        let (stempel, preis) = verlauf[i];
        let (hoch, text) = kurs_delta(preis, basis, stellen);
        let links = hx / CHART_B * 100.0;
        let mut klasse = String::from("kurs-hover-tipp");
        if hy / CHART_H * 100.0 < 34.0 {
            klasse.push_str(" unten");
        }
        if links < 16.0 {
            klasse.push_str(" links");
        }
        if links > 84.0 {
            klasse.push_str(" rechts");
        }
        (klasse, links, format!("{} {}", zahl(preis, stellen), waehrung), hoch, text, hover_zeit(stempel, taeglich))
    });

    rsx! {
        div {
            class: "kurs-chart-bereich",
            "aria-label": "Interaktiver Kursverlauf",
            onmounted: move |e| box_el.set(e.data().downcast::<web_sys::Element>().cloned()),
            onmousemove: move |e| {
                if let Some(i) = index_bei(&box_el.peek(), e.client_coordinates().x, n) {
                    hover.set(Some(i));
                }
            },
            ontouchstart: move |e| {
                if let Some(t) = e.touches().first() {
                    if let Some(i) = index_bei(&box_el.peek(), t.client_coordinates().x, n) {
                        hover.set(Some(i));
                    }
                }
            },
            ontouchmove: move |e| {
                if let Some(t) = e.touches().first() {
                    if let Some(i) = index_bei(&box_el.peek(), t.client_coordinates().x, n) {
                        hover.set(Some(i));
                    }
                }
            },
            onmouseleave: move |_| hover.set(None),
            ontouchend: move |_| hover.set(None),
            ontouchcancel: move |_| hover.set(None),
            svg {
                class: "kurs-chart",
                view_box: "0 0 {CHART_B} {CHART_H}",
                preserve_aspect_ratio: "none",
                "aria-hidden": "true",
                defs {
                    linearGradient { id: "kurs-grad", x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%", stop_color: "var(--chart-farbe)", stop_opacity: "0.25" }
                        stop { offset: "100%", stop_color: "var(--chart-farbe)", stop_opacity: "0.0" }
                    }
                }
                line { class: "gitter", x1: "0", y1: "53", x2: "{CHART_B}", y2: "53" }
                line { class: "gitter", x1: "0", y1: "106", x2: "{CHART_B}", y2: "106" }
                if let Some(y) = vorher {
                    line { class: "kurs-vorher", x1: "0", y1: "{y}", x2: "{CHART_B}", y2: "{y}" }
                }
                polygon { class: "kurs-flaeche", points: "{flaeche}" }
                polyline { class: "kurs-linie", points: "{punkte}" }
                line {
                    class: "kurs-hover-vlinie",
                    x1: "{hx:.1}", y1: "0", x2: "{hx:.1}", y2: "{CHART_H}",
                    opacity: if aktiv.is_some() { "0.75" } else { "0" },
                }
                circle {
                    class: "kurs-hover-dot",
                    cx: "{hx:.1}", cy: "{hy:.1}", r: "4.5",
                    opacity: if aktiv.is_some() { "1" } else { "0" },
                }
            }
            if let Some((klasse, links, preis, hoch, text, zeit)) = tipp {
                div { class: "{klasse}", style: "left: {links}%",
                    div { class: "tipp-preis", "{preis}" }
                    div { class: if hoch { "tipp-delta hoch" } else { "tipp-delta runter" }, "{text}" }
                    div { class: "tipp-zeit", "{zeit}" }
                }
            } else {
                div { class: "kurs-hover-tipp", hidden: true }
            }
        }
        div { class: "kurs-zeiten",
            for z in zeiten {
                span { "{z}" }
            }
        }
    }
}

// ---- Karte -------------------------------------------------------------------------------

/// Die gezeichnete Karte: Kursdaten und die Anfrage dazu.
#[derive(Clone, PartialEq)]
struct Karte {
    daten: Value,
    anfrage: KursAnfrage,
}

/// Der Zustand der Karte (alles Signale, also kopierbar).
#[derive(Clone, Copy)]
struct Zustand {
    karte: Signal<Option<Karte>>,
    betrag: Signal<String>,
    von: Signal<String>,
    nach: Signal<String>,
    hover: Signal<Option<usize>>,
}

impl Zustand {
    /// Die Karte neu zeichnen - mit Rechner (Startbetrag) und ohne Fadenkreuz.
    fn zeichnen(mut self, daten: Value, anfrage: KursAnfrage) {
        let start = if anfrage.betrag != 0.0 && !anfrage.umgekehrt { anfrage.betrag } else { 1.0 };
        self.betrag.set(start.to_string());
        let (von, nach) = paar_von_symbol(daten["symbol"].as_str().unwrap_or("")).unwrap_or_default();
        self.von.set(von);
        self.nach.set(nach);
        self.hover.set(None);
        self.karte.set(Some(Karte { daten, anfrage }));
    }
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let anfrage = kurs_anfrage(&ctx.frage)?;
    if anfrage.nur_name {
        return None;
    }
    serde_json::to_value(anfrage).ok()
}

/// Zeichnet das Modul (steht in `<section class="modul modul-kurs">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let z = Zustand {
        karte: use_signal(|| None),
        betrag: use_signal(String::new),
        von: use_signal(String::new),
        nach: use_signal(String::new),
        hover: use_signal(|| None),
    };
    let anfrage: Option<KursAnfrage> = serde_json::from_value(daten).ok();
    use_future(move || {
        let ctx = ctx.clone();
        let anfrage = anfrage.clone();
        async move {
            let Some(anfrage) = anfrage else { return };
            let bereich = bereich_aus_zeitraum(&anfrage.symbol, &ctx.einstellungen().zeitraum);
            let Some(daten) = kurs_holen(&anfrage.symbol, bereich).await else { return };
            if daten["kurs"].as_f64().is_some_and(|k| k != 0.0) && ctx.aktuell() {
                z.zeichnen(daten, anfrage);
            }
        }
    });

    let css = rsx! { werkzeug::ModulCss { name: "kurs".to_string(), css: CSS } };
    // Zeigt die Info-Karte schon ein Unternehmen, steht der Kurs dort.
    let karte = z.karte.read().clone();
    let Some(Karte { daten, anfrage }) = karte.filter(|_| !crate::module::info::FIRMA_SICHTBAR()) else {
        return css;
    };

    let symbol = daten["symbol"].as_str().unwrap_or("").to_string();
    let kurs = daten["kurs"].as_f64().unwrap_or(0.0);
    let stellen = kurs_stellen(kurs, &symbol);
    let waehrung = daten["waehrung"].as_str().unwrap_or("").to_string();
    let name = daten["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(&symbol).to_string();
    let zeitraum = daten["zeitraum"].as_str().filter(|s| !s.is_empty()).unwrap_or("heute").to_string();
    let quelle = daten["quelle"].as_str().unwrap_or("").to_string();
    let quelle_link = daten["link"].as_str().filter(|l| l.starts_with("https://")).map(String::from);
    let vorher = daten["vorher"].as_f64();
    let paar = paar_von_symbol(&symbol);
    let bereich = daten["bereich"].as_str().map(String::from);
    let aktiv_bereich = bereich.clone().unwrap_or_else(|| standard_bereich(&symbol).to_string());
    let stand_ms = daten["stand"].as_f64().filter(|s| *s != 0.0).map(|s| s * 1000.0).unwrap_or_else(werkzeug::jetzt_ms);
    let stand = format!("{} Uhr", zeit_text(stand_ms, &[("day", "numeric"), ("month", "short"), ("hour", "2-digit"), ("minute", "2-digit")]));

    // Beim Ueberfahren des Verlaufs folgt der Kopf: Wert, Aenderung, Zeit.
    let verlauf = verlauf_von(&daten);
    let punkt = (z.hover)().and_then(|i| verlauf.get(i).copied()).filter(|_| verlauf.len() >= 2);
    let (wert_text, delta, zeit) = match punkt {
        Some((stempel, preis)) => (
            zahl(preis, stellen),
            vorher.map(|_| kurs_delta(preis, hover_basis(&daten, &verlauf), stellen)),
            hover_zeit(stempel, ist_taeglich(&verlauf)),
        ),
        None => (zahl(kurs, stellen), vorher.map(|v| kurs_delta(kurs, v, stellen)), stand),
    };
    let runter = vorher.map(|v| kurs - v < 0.0).unwrap_or(false);
    let ergebnis = zahl({
        let b = zahl_aus_feld(&z.betrag.read());
        (if b.is_nan() { 0.0 } else { b }) * kurs
    }, stellen);

    let symbol_klick = symbol.clone();
    let anfrage_klick = anfrage.clone();
    let symbol_wechsel = symbol.clone();
    let bereich_wechsel = bereich.clone();
    let mut betrag_sig = z.betrag;
    let mut von_sig = z.von;
    let mut nach_sig = z.nach;

    // Zeitraum-Reiter: anderer Verlauf, gleiche Karte, gleicher Rechner.
    let reiter_klick = move |id: &'static str| {
        let symbol = symbol_klick.clone();
        let alt = anfrage_klick.clone();
        let bereich = bereich.clone();
        move |_: MouseEvent| {
            if bereich.as_deref() == Some(id) {
                return;
            }
            let symbol = symbol.clone();
            let alt = alt.clone();
            spawn(async move {
                let betrag = zahl_aus_feld(&betrag_sig.peek());
                let Some(neu) = kurs_holen(&symbol, id).await else { return };
                if neu["kurs"].as_f64().is_some_and(|k| k != 0.0) {
                    let betrag = if betrag > 0.0 { betrag } else if alt.betrag != 0.0 { alt.betrag } else { 1.0 };
                    z.zeichnen(neu, KursAnfrage { symbol, betrag, umgekehrt: alt.umgekehrt, nur_name: false });
                }
            });
        }
    };

    // Andere Waehrung gewaehlt: den neuen Kurs holen.
    let wechseln = move || {
        let Some(mut neu) = kurs_anfrage(&format!("{} in {}", von_sig.peek(), nach_sig.peek())) else { return };
        if neu.symbol == symbol_wechsel {
            return;
        }
        let betrag = zahl_aus_feld(&betrag_sig.peek());
        if betrag > 0.0 {
            neu.betrag = betrag;
        }
        let bereich = bereich_wechsel.clone().unwrap_or_else(|| standard_bereich(&neu.symbol).to_string());
        spawn(async move {
            let Some(neu_daten) = kurs_holen(&neu.symbol, &bereich).await else { return };
            if neu_daten["kurs"].as_f64().is_some_and(|k| k != 0.0) {
                z.zeichnen(neu_daten, neu);
            }
        });
    };
    let wechseln_von = wechseln.clone();
    let wechseln_nach = wechseln;

    rsx! {
        {css}
        div { class: if runter { "kurs runter" } else { "kurs hoch" },
            div { class: "kurs-kopf",
                div { class: "kurs-titel",
                    "Finanzen \u{203a} "
                    b { "{name}" }
                    " "
                    span { class: "kurs-tag", "{symbol}" }
                }
                div { class: "kurs-preis",
                    span { class: "kurs-zahl", "{wert_text}" }
                    span { class: "kurs-einheit", "{waehrung}" }
                }
                if let Some((hoch, text)) = delta {
                    div { class: if hoch { "kurs-stand hoch" } else { "kurs-stand runter" },
                        span { class: "kurs-pfeil", "aria-hidden": "true", if hoch { "\u{25b2}" } else { "\u{25bc}" } }
                        span { class: "kurs-delta-text", "{text}" }
                        span { class: "kurs-zeitraum", "{zeitraum}" }
                    }
                }
                div { class: "kurs-zeit",
                    span { "{zeit}" }
                    " \u{b7} Quelle: "
                    if let Some(l) = quelle_link {
                        a { href: "{l}", target: "_blank", rel: "noreferrer", "{quelle}" }
                    } else {
                        "{quelle}"
                    }
                }
            }
            div { class: "kurs-bereiche", role: "group", "aria-label": "Zeitraum",
                for (id, titel) in KURS_BEREICHE.iter().copied() {
                    button {
                        r#type: "button",
                        class: if aktiv_bereich == id { "kurs-bereich aktiv" } else { "kurs-bereich" },
                        "aria-pressed": "{aktiv_bereich == id}",
                        onclick: reiter_klick(id),
                        "{titel}"
                    }
                }
            }
            KursChart { daten: daten.clone(), hover: Some(z.hover) }
            div { class: "kurs-rechner",
                if paar.is_some() {
                    div { class: "kurs-feld",
                        input {
                            r#type: "text",
                            inputmode: "decimal",
                            id: "kurs-betrag",
                            "aria-label": "Betrag",
                            value: "{betrag_sig}",
                            oninput: move |e| betrag_sig.set(e.value()),
                        }
                        select {
                            id: "kurs-von",
                            "aria-label": "Von",
                            value: "{von_sig}",
                            onchange: move |e| {
                                von_sig.set(e.value());
                                wechseln_von();
                            },
                            for w in kurs_waehrungen() {
                                option { value: "{w}", selected: *von_sig.read() == w, "{w}" }
                            }
                        }
                    }
                    span { class: "gleich", "=" }
                    div { class: "kurs-feld",
                        input { r#type: "text", id: "kurs-ergebnis", readonly: true, "aria-label": "Ergebnis", value: "{ergebnis}" }
                        select {
                            id: "kurs-nach",
                            "aria-label": "Nach",
                            value: "{nach_sig}",
                            onchange: move |e| {
                                nach_sig.set(e.value());
                                wechseln_nach();
                            },
                            for w in kurs_waehrungen() {
                                option { value: "{w}", selected: *nach_sig.read() == w, "{w}" }
                            }
                        }
                    }
                } else {
                    span { class: "gleich", "1 {symbol} = {zahl(kurs, stellen)} {waehrung}" }
                }
            }
        }
    }
}

/// Das CSS der Kurskarte (aus kurs.js). Die erste Regel ersetzt das alte `frei`: die Karte hat
/// ihren eigenen Rahmen, die Section bekommt keinen.
pub const CSS: &str = r#"/* Die Karte hat ihren eigenen Rahmen (frueher class "frei") */
#widgets > .modul.modul-kurs.modul-kurs { border: 0; border-radius: 0; padding: 0; margin-bottom: 0; }

/* ---- Kurskarte (Google Finance Vorbild) ----------------------------------- */
/* Grosse Zahl, Aenderung, Zeitmarken, Zeitraum-Tabs, SVG mit Farbverlauf
   und interaktives Fadenkreuz / Tooltip beim Darueberfahren. */

.kurs {
  --chart-farbe: var(--gruen);
  width: 100%;
  max-width: 44rem;
  border: 1px solid var(--kante);
  background: var(--flaeche);
  border-radius: var(--r);
  padding: 1.25rem 1.4rem 1.35rem;
  margin-bottom: 2rem;
  transition: border-color .15s ease;
}
.kurs.runter {
  --chart-farbe: var(--rot);
}

.kurs-kopf {
  display: flex;
  flex-direction: column;
  gap: .25rem;
  margin-bottom: .85rem;
}

.kurs-titel {
  font-size: .88rem;
  color: var(--gedimmt);
  display: flex;
  align-items: center;
  gap: .4rem;
  flex-wrap: wrap;
}
.kurs-titel b {
  color: var(--schrift);
  font-weight: 500;
  font-size: 1rem;
}
.kurs-tag {
  font-size: .72rem;
  padding: .1rem .45rem;
  background: var(--flaeche2);
  border: 1px solid var(--kante);
  border-radius: var(--r2);
  color: var(--gedimmt);
  font-weight: 500;
  letter-spacing: .02em;
}

.kurs-preis {
  display: flex;
  align-items: baseline;
  gap: .4rem;
  margin-top: .15rem;
}
.kurs-zahl {
  font-size: 2.35rem;
  line-height: 1.05;
  letter-spacing: -.015em;
  color: var(--schrift);
  font-weight: 400;
  font-variant-numeric: tabular-nums;
}
.kurs-einheit {
  font-size: 1.1rem;
  color: var(--gedimmt);
  font-weight: 400;
}

.kurs-stand {
  display: flex;
  align-items: center;
  gap: .35rem;
  font-size: .96rem;
  font-weight: 500;
  margin-top: .1rem;
}
.kurs-stand.hoch { color: var(--gruen); }
.kurs-stand.runter { color: var(--rot); }
.kurs-pfeil { font-size: .8rem; line-height: 1; }
.kurs-zeitraum { font-weight: 400; color: var(--gedimmt); font-size: .86rem; margin-left: .2rem; }

.kurs-zeit {
  font-size: .78rem;
  color: var(--gedimmt);
  margin-top: .2rem;
}
.kurs-zeit a { color: var(--hoch); text-decoration: none; }
.kurs-zeit a:hover { text-decoration: underline; }

/* Zeitraum-Reiter wie bei Google Finance (1 T., 5 T., 1 M. ... Max.) */
.kurs-bereiche {
  display: flex;
  flex-wrap: wrap;
  gap: .3rem;
  margin: .85rem 0 .5rem;
}
.kurs-bereich {
  font: inherit;
  font-size: .78rem;
  font-weight: 500;
  padding: .32rem .72rem;
  border: 1px solid transparent;
  border-radius: 999px;
  background: transparent;
  color: var(--gedimmt);
  cursor: pointer;
  transition: all .12s ease;
}
.kurs-bereich:hover {
  color: var(--schrift);
  background: var(--flaeche2);
}
.kurs-bereich.aktiv {
  color: var(--hoch);
  background: var(--flaeche3);
  font-weight: 600;
}
.kurs-bereich:focus-visible { outline: 2px solid var(--hoch); outline-offset: 1px; }

/* Interaktiver Chart-Bereich mit Hover-Fadenkreuz */
.kurs-chart-bereich {
  position: relative;
  width: 100%;
  height: 10.5rem;
  margin: .75rem 0 .25rem;
  touch-action: none;
  cursor: crosshair;
  user-select: none;
}
.kurs-chart {
  display: block;
  width: 100%;
  height: 100%;
  overflow: visible;
}
.kurs-chart .gitter {
  stroke: var(--kante);
  stroke-width: 1;
  vector-effect: non-scaling-stroke;
  opacity: .55;
}
.kurs-chart .kurs-vorher {
  stroke: var(--gedimmt);
  stroke-width: 1;
  stroke-dasharray: 3, 3;
  vector-effect: non-scaling-stroke;
  opacity: .45;
}
.kurs-chart .kurs-linie {
  fill: none;
  stroke: var(--chart-farbe);
  stroke-width: 2;
  stroke-linejoin: round;
  stroke-linecap: round;
  vector-effect: non-scaling-stroke;
}
.kurs-chart .kurs-flaeche {
  fill: url(#kurs-grad);
  stroke: none;
}
.kurs-chart .kurs-hover-vlinie {
  stroke: var(--gedimmt);
  stroke-width: 1;
  stroke-dasharray: 3, 3;
  vector-effect: non-scaling-stroke;
  pointer-events: none;
  transition: opacity .08s ease;
}
.kurs-chart .kurs-hover-dot {
  fill: var(--chart-farbe);
  stroke: var(--bg);
  stroke-width: 2.5;
  vector-effect: non-scaling-stroke;
  pointer-events: none;
  transition: opacity .08s ease;
}

/* Schwebendes Tooltip-Badge ueber dem Hover-Punkt */
.kurs-hover-tipp {
  position: absolute;
  top: 0;
  left: 0;
  transform: translate(-50%, -115%);
  z-index: 20;
  padding: .45rem .75rem;
  background: var(--flaeche2);
  border: 1px solid var(--kante);
  border-radius: var(--r2);
  box-shadow: var(--schatten);
  pointer-events: none;
  white-space: nowrap;
  font-size: .78rem;
  line-height: 1.35;
  text-align: center;
  transition: opacity .08s ease, transform .08s ease;
}
.kurs-hover-tipp.unten { transform: translate(-50%, 25%); }
.kurs-hover-tipp.links { transform: translate(0, -115%); }
.kurs-hover-tipp.links.unten { transform: translate(0, 25%); }
.kurs-hover-tipp.rechts { transform: translate(-100%, -115%); }
.kurs-hover-tipp.rechts.unten { transform: translate(-100%, 25%); }

.kurs-hover-tipp .tipp-preis {
  font-weight: 600;
  font-size: .88rem;
  color: var(--schrift);
}
.kurs-hover-tipp .tipp-delta {
  font-size: .76rem;
  font-weight: 500;
}
.kurs-hover-tipp .tipp-delta.hoch { color: var(--gruen); }
.kurs-hover-tipp .tipp-delta.runter { color: var(--rot); }
.kurs-hover-tipp .tipp-zeit {
  font-size: .72rem;
  color: var(--gedimmt);
  margin-top: .1rem;
}

.kurs-zeiten {
  display: flex;
  justify-content: space-between;
  font-size: .75rem;
  color: var(--gedimmt);
  margin-top: .2rem;
}

/* Waehrungsrechner im Google-Stil */
.kurs-rechner {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: .75rem;
  margin-top: 1.1rem;
  padding-top: 1rem;
  border-top: 1px solid var(--kante);
}
.kurs-rechner .gleich {
  color: var(--gedimmt);
  font-size: 1.2rem;
  font-weight: 300;
}
.kurs-feld {
  display: flex;
  align-items: stretch;
  border: 1px solid var(--kante);
  border-radius: var(--r);
  background: var(--flaeche2);
  overflow: hidden;
  transition: border-color .15s ease, box-shadow .15s ease;
}
.kurs-feld:focus-within {
  border-color: var(--hoch);
  box-shadow: 0 0 0 1px var(--hoch);
}
.kurs-feld input {
  font: inherit;
  font-size: 1.05rem;
  font-weight: 500;
  width: 9rem;
  padding: .5rem .75rem;
  color: var(--schrift);
  background: transparent;
  border: 0;
}
.kurs-feld input:focus { outline: none; }
.kurs-feld input[readonly] { color: var(--schrift); background: transparent; }
.kurs-feld select {
  font: inherit;
  font-size: .88rem;
  font-weight: 500;
  padding: .5rem .65rem;
  color: var(--schrift);
  background: var(--flaeche3);
  border: 0;
  border-left: 1px solid var(--kante);
  cursor: pointer;
  outline: none;
}
"#;
