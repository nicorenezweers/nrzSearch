//! Modul "vergleich" - Portierung von public/module/vergleich.js: "gtx 1050 ti vs rtx 2080",
//! "AMD vs Intel", "GPT-5 vs Claude", "MacBook Air oder ThinkPad X1" … Dieter sucht im
//! Hintergrund selbst, liest die Seiten und erzeugt daraus einen Benchmark: nur Diagramme und
//! eine Tabelle mit Quellen, kein Text. Bei Aktien und Krypto kommen die Kurse dazu. Ein Knopf
//! kopiert einen Prompt mit allem Gesammelten fuer eine andere KI, ein anderer fuehrt in den
//! KI-Modus.
//!
//! module.json: trenner (Woerter zwischen den Seiten), vorne (Woerter davor wie "vergleich"),
//! maxWoerter (je Seite bei "oder").

use super::ModulCtx;
use crate::api::{self, Abbruch};
use crate::bib::diagramm::{ersetzen, wert_text};
use crate::bib::ki::{self, Quelle, QuellenZeile, bloecke, bloecke_zeigen};
use crate::i18n::t;
use crate::werkzeug::{KopierKnopf, ModulCss, re_treffer, zahl};
use dioxus::prelude::*;
use serde_json::{Value, json};

const CSS: &str = r#"
.modul-vergleich .vg-kopf { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px; margin-bottom: 14px; font: 500 16px "Google Sans", "Segoe UI", Arial; color: var(--gedimmt); }
.modul-vergleich .vg-kopf .ki-stern { width: 22px; height: 22px; margin: 0; }
.modul-vergleich .vg-titel { display: inline-flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; font-size: 20px; color: var(--schrift); }
.modul-vergleich .vg-vs { font-size: 13px; font-weight: 700; color: var(--hoch); text-transform: uppercase; }
.modul-vergleich .vg-kurse { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 10px; margin-bottom: 16px; }
.modul-vergleich .vg-kurs { display: flex; flex-direction: column; gap: 4px; padding: 12px 14px; border: 1px solid var(--kante); border-radius: 12px; color: var(--schrift); text-decoration: none !important; }
.modul-vergleich .vg-kurs:hover { background: var(--flaeche2); }
.modul-vergleich .vg-kurs-name { font-size: 13px; color: var(--adresse); }
.modul-vergleich .vg-kurs-name small { color: var(--gedimmt); }
.modul-vergleich .vg-kurs-wert { font-size: 22px; }
.modul-vergleich .vg-kurs-delta { font-size: 13px; }
.modul-vergleich .vg-kurs-delta.hoch { color: var(--gruen); }
.modul-vergleich .vg-kurs-delta.runter { color: var(--rot); }
.modul-vergleich .ki-status.aus::before { display: none; }
.modul-vergleich .ki-fuss { margin-top: 18px; }
.modul-vergleich .vg-geruest { display: flex; flex-direction: column; gap: 12px; }
.modul-vergleich .vg-geruest-zeile { height: 18px; border-radius: 9px; background: var(--flaeche2); animation: vgPuls 1.4s ease-in-out infinite; }
.modul-vergleich .vg-geruest-zeile:nth-child(2) { width: 82%; } .modul-vergleich .vg-geruest-zeile:nth-child(3) { width: 64%; } .modul-vergleich .vg-geruest-zeile:nth-child(4) { width: 90%; }
@keyframes vgPuls { 50% { opacity: .45; } }
.modul-vergleich .ki-diagramm { margin: 0 0 18px; padding: 0 0 18px; border: 0; border-bottom: 1px solid var(--kante); border-radius: 0; }
"#;

/// Hardware heisst "Benchmark", alles andere "Vergleich".
const HARDWARE: &str = r"\b(?:rtx|gtx|rx\s?\d|radeon|geforce|arc\s?[ab]\d|ryzen|threadripper|core\s?(?:i\d|ultra)|i[3579]-?\d|xeon|epyc|apple\s?m\d|m\d\s?(?:pro|max)|snapdragon|exynos|tensor|dimensity|iphone|pixel|galaxy|macbook|thinkpad|laptop|notebook|ssd|nvme|ps5|xbox|switch)\b";

// ---- Kurse (nachgebaut aus module/kurs.js: kursAnfrage, kursHolen, kursStellen) ------------

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
    ("deutsche telekom", "DTE.DE"), ("deutsche börse", "DB1.DE"), ("allianz", "ALV.DE"), ("lvmh", "MC.PA"),
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
    ("kupferpreis", "HG=F"), ("platin", "PL=F"), ("palladium", "PA=F"), ("rohoel", "CL=F"), ("erdoel", "CL=F"), ("rohöl", "CL=F"),
    ("erdöl", "CL=F"), ("wti", "CL=F"), ("brent", "BZ=F"), ("erdgas", "NG=F"), ("weizen", "ZW=F"), ("mais", "ZC=F"), ("soja", "ZS=F"),
    ("kaffee", "KC=F"), ("kakao", "CC=F"), ("zucker", "SB=F"), ("baumwolle", "CT=F"), ("aluminium", "ALI=F"), ("heizoel", "HO=F"), ("benzin", "RB=F"),
];
const FIAT: &[&str] = &[
    "USD", "EUR", "GBP", "JPY", "CHF", "AUD", "CAD", "CNY", "SEK", "NOK", "DKK", "PLN", "INR", "KRW", "BRL", "NZD", "MXN", "ZAR", "SGD", "HKD", "TRY", "AED", "ILS", "THB", "CZK", "HUF", "RON", "BGN", "ISK", "CLP", "COP", "ARS", "IDR", "MYR", "PHP", "VND", "TWD",
];
const WAEHRUNG_NAMEN: &[(&str, &str)] = &[("euro", "EUR"), ("dollar", "USD"), ("pfund", "GBP"), ("yen", "JPY"), ("yuan", "CNY"), ("renminbi", "CNY"), ("franken", "CHF"), ("rand", "ZAR"), ("real", "BRL")];
/// Woerter, die nichts mit dem Kurs selbst zu tun haben.
const KURS_STOPP: &[&str] = &[
    "kurs", "kurse", "kursentwicklung", "preis", "preise", "preisentwicklung", "price", "heute", "jetzt", "aktuell", "aktuelle", "aktueller", "von", "der", "die", "das", "den", "dem", "und", "im", "am", "für", "fuer", "ist", "wie", "viel", "bitte", "kurz", "aktie", "aktien", "aktienkurs", "woert",
    "cours", "course",
];

#[derive(Clone, Copy, PartialEq)]
enum Art {
    Markt,
    Krypto,
    Aktie,
    Fiat,
}

fn suchen_in(liste: &[(&str, &'static str)], wort: &str) -> Option<&'static str> {
    liste.iter().find(|(k, _)| *k == wort).map(|(_, v)| *v)
}

/// Ein Wort nachschlagen: Marktdaten, Krypto, Aktie oder Waehrung.
fn kurs_nachschlagen(wort: &str) -> Option<(String, Art)> {
    if let Some(s) = suchen_in(MARKTDATEN, wort) {
        return Some((s.into(), Art::Markt));
    }
    if let Some(s) = suchen_in(KRYPTO, wort) {
        return Some((s.into(), Art::Krypto));
    }
    if let Some(s) = suchen_in(AKTIEN, wort) {
        return Some((s.into(), Art::Aktie));
    }
    let f = suchen_in(WAEHRUNG_NAMEN, wort).map(String::from).unwrap_or_else(|| if wort.len() == 3 && wort.chars().all(|c| c.is_ascii_lowercase()) { wort.to_uppercase() } else { String::new() });
    FIAT.contains(&f.as_str()).then_some((f, Art::Fiat))
}

/// Aus zwei Teilen das Kurs-Symbol bauen (oder None).
fn kurs_paar(von: &(String, Art), nach: &(String, Art)) -> Option<String> {
    use Art::*;
    Some(match (von.1, nach.1) {
        (Krypto, Fiat) => format!("{}-{}", von.0, nach.0),
        (Fiat, Krypto) => format!("{}-{}", nach.0, von.0),
        (Fiat, Fiat) => format!("{}{}=X", von.0, nach.0),
        (Krypto, Krypto) => format!("{}-{}=X", von.0, nach.0),
        (Aktie, Fiat) | (Markt, Fiat) => von.0.clone(),
        (Fiat, Markt) => nach.0.clone(),
        _ => return None,
    })
}

/// Aus einer Frage das Kurs-Symbol: "bitcoin" -> BTC-EUR, "apple aktie" -> AAPL, "gold" -> GC=F.
fn kurs_anfrage(frage: &str) -> Option<String> {
    let klein = frage.trim().to_lowercase();
    let woerter: Vec<&str> = klein.split(|c: char| c.is_whitespace() || ",!?;:".contains(c)).filter(|w| !w.is_empty()).collect();
    if woerter.is_empty() {
        return None;
    }
    // Muster 1: "X in Y"
    if let Some(i) = woerter.iter().position(|w| *w == "in") {
        if i > 0 && i + 1 < woerter.len() {
            if let (Some(von), Some(nach)) = (kurs_nachschlagen(woerter[i - 1]), kurs_nachschlagen(woerter[i + 1])) {
                if von.0 != nach.0 {
                    return kurs_paar(&von, &nach);
                }
            }
        }
    }
    // Muster 2: Mehrwort-Namen ("ftse all world", "dow jones") - laengere zuerst.
    for n in (1..=3).rev() {
        for fenster in woerter.windows(n) {
            if let Some(s) = suchen_in(MARKTDATEN, &fenster.join(" ")) {
                return Some(s.into());
            }
        }
    }
    // Muster 3: "bitcoin kurs", "apple aktie", "kurs von tesla".
    for w in &woerter {
        if KURS_STOPP.contains(w) {
            continue;
        }
        let Some(t) = kurs_nachschlagen(w) else { continue };
        return match t.1 {
            Art::Krypto => kurs_paar(&t, &("EUR".into(), Art::Fiat)),
            Art::Aktie | Art::Markt => Some(t.0),
            Art::Fiat => None, // "euro kurs" allein ist zu mehrdeutig
        };
    }
    None
}

/// Bei Wechselkursen vier Nachkommastellen, sonst je nach Groesse.
fn kurs_stellen(n: f64, symbol: &str) -> usize {
    let a = n.abs();
    if symbol.ends_with("=X") {
        return if a >= 0.01 { 4 } else { 6 };
    }
    if a >= 1.0 {
        2
    } else if a >= 0.01 {
        4
    } else {
        6
    }
}

async fn kurs_holen(symbol: &str, bereich: &str) -> Option<Value> {
    api::get_oder_nichts("/api/kurs", &[("symbol", symbol), ("bereich", bereich)]).await
}

// ---- Vergleich ---------------------------------------------------------------------------

fn liste(ctx: &ModulCtx, k: &str) -> Vec<String> {
    ctx.cfg(k).as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect()
}

/// "A vs B vs C" -> ["A", "B", "C"]; "vergleich A und B" -> ["A", "B"].
fn seiten_von(frage: &str, ctx: &ModulCtx) -> Option<Vec<String>> {
    let mut q = ersetzen(frage.trim(), r"[?!.]+$", "", "");
    let vorne = format!(r"^(?:{})\s+(?:zwischen\s+|von\s+)?", liste(ctx, "vorne").join("|"));
    let mit_vorne = re_treffer(&vorne, "iu", &q).is_some();
    q = ersetzen(&q, &vorne, "iu", "");
    let mut trenner = liste(ctx, "trenner");
    if trenner.is_empty() {
        trenner = vec![r"vs\.?".into(), "versus".into()];
    }
    if mit_vorne {
        trenner.extend(["und", "mit", "and"].map(String::from));
    }
    let re = js_sys::RegExp::new(&format!(r"\s+(?:{})\s+", trenner.join("|")), "iu");
    let seiten: Vec<String> = js_sys::JsString::from(q.as_str()).split_by_pattern(&re).iter().filter_map(|s| s.as_string()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if seiten.len() < 2 || seiten.len() > 4 {
        return None;
    }
    // "oder" nur bei kurzen Seiten - "sein oder nicht sein" ist kein Vergleich.
    let max = ctx.cfg_f64("maxWoerter").filter(|m| *m > 0.0).unwrap_or(3.0);
    if re_treffer(r"\s(?:oder|or)\s", "iu", &q).is_some() && seiten.iter().any(|x| x.split_whitespace().count() as f64 > max) {
        return None;
    }
    Some(seiten)
}

/// Nur die Daten aus Dieters Antwort: Diagramm-Bloecke und Tabellenzeilen.
fn nur_daten(text: &str) -> String {
    // Ein einziges JSON-Objekt ohne Codeblock: als Diagramm-Block behandeln.
    let roh = text.trim();
    if roh.starts_with('{') {
        return format!("```diagramm\n{roh}{}", if roh.ends_with('}') { "\n```" } else { "" });
    }
    let mut raus: Vec<&str> = Vec::new();
    let mut im_block = false;
    for zeile in text.split('\n') {
        let z = zeile.trim();
        if z.starts_with("```") {
            im_block = !im_block;
            raus.push(zeile);
        } else if im_block || z.starts_with('|') {
            raus.push(zeile);
        } else if raus.last().is_some_and(|l| !l.trim().is_empty()) {
            raus.push("");
        }
    }
    raus.join("\n")
}

/// Der Prompt fuer eine andere KI - mit allem, was nrzSearch gesammelt hat.
fn prompt_text(seiten: &[String], text: &str, material: &[Value], zusatz: &str) -> String {
    let d = js_sys::Date::new_0();
    let heute = format!("{}.{}.{}", d.get_date(), d.get_month() + 1, d.get_full_year());
    let mut teile = vec![
        format!("Ich möchte {} gründlich vergleichen. Unten steht alles, was meine Suchmaschine nrzSearch dazu gesammelt hat (Stand {heute}).", seiten.join(" und ")),
        "Bitte erstelle daraus einen ausführlichen, neutralen Vergleich: eine Tabelle mit allen wichtigen Merkmalen, Stärken und Schwächen jeder Seite und ein Fazit, für wen sich was eignet. Weise auf widersprüchliche oder veraltete Angaben hin und belege Aussagen mit den Quellen-Nummern.".into(),
        String::new(),
        "## Zusammenfassung von nrzSearch".into(),
        text.trim().to_string(),
    ];
    if !zusatz.is_empty() {
        teile.extend([String::new(), "## Aktuelle Kursdaten".into(), zusatz.to_string()]);
    }
    teile.extend([String::new(), "## Quellen und Auszüge".into()]);
    let feld = |m: &Value, k: &str| m.get(k).map(wert_text).unwrap_or_default();
    teile.extend(material.iter().map(|m| format!("\n[{}] {}\n{}\n{}", feld(m, "nr"), feld(m, "titel"), feld(m, "url"), feld(m, "auszug"))));
    teile.join("\n")
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    seiten_von(&ctx.frage, ctx).map(|s| json!(s))
}

#[component]
fn Geruest() -> Element {
    rsx! {
        div { class: "vg-geruest",
            for i in 0..4 {
                div { key: "{i}", class: "vg-geruest-zeile" }
            }
        }
    }
}

/// Zeichnet das Modul (steht in `<section class="modul modul-vergleich">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let seiten: Vec<String> = daten.as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
    let hardware = seiten.iter().any(|x| re_treffer(HARDWARE, "i", x).is_some());
    let mut kurse = use_signal(Vec::<Value>::new);
    let mut zusatz = use_signal(String::new);
    let mut status = use_signal(|| Some("Dieter sucht …".to_string()));
    let mut aus = use_signal(|| false);
    let mut text = use_signal(String::new);
    let mut quellen = use_signal(Vec::<Quelle>::new);
    let mut material = use_signal(Vec::<Value>::new);
    let mut fertig = use_signal(|| false);

    let (c, s) = (ctx.clone(), seiten.clone());
    use_future(move || {
        let (ctx, seiten) = (c.clone(), s.clone());
        async move {
            // Aktien und Krypto: die Kurse dazu - fuer alle Seiten oder gar nicht.
            let symbole: Vec<Option<String>> = seiten.iter().map(|x| kurs_anfrage(x)).collect();
            if symbole.iter().all(Option::is_some) {
                let holen = symbole.iter().flatten().map(|s| kurs_holen(s, "1y"));
                let geholt: Vec<Value> = futures::future::join_all(holen).await.into_iter().flatten().filter(|k| k.get("kurs").and_then(Value::as_f64).is_some_and(|x| x != 0.0)).collect();
                if !ctx.aktuell() {
                    return;
                }
                if geholt.len() == seiten.len() {
                    let f = |k: &Value, n: &str| k.get(n).map(wert_text).unwrap_or_default();
                    zusatz.set(
                        geholt
                            .iter()
                            .map(|k| {
                                let w = f(k, "waehrung");
                                let vorher = k.get("vorher").filter(|v| !v.is_null()).map_or_else(|| "?".to_string(), wert_text);
                                let quelle = Some(f(k, "quelle")).filter(|q| !q.is_empty()).unwrap_or_else(|| "Yahoo Finance".into());
                                format!("{} ({}): Kurs {} {w}, vor 1 Jahr {vorher} {w} (Quelle: {quelle})", f(k, "name"), f(k, "symbol"), f(k, "kurs"))
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                    kurse.set(geholt);
                }
            }
            if !ki::bereit().await {
                status.set(Some("Für den Vergleich braucht Dieter einen OpenRouter-Schlüssel (KI-Modus öffnen).".into()));
                aus.set(true);
                return;
            }
            let ab = Abbruch::neu();
            let anfrage = json!({ "frage": ctx.frage, "art": "vergleich", "seiten": seiten, "zusatz": zusatz(), "sprache": ctx.einstellungen().sprache });
            let mut fehler = String::new();
            let ergebnis = ki::fragen(&anfrage, &ab, |name, d| {
                if !ctx.aktuell() {
                    ab.abbrechen();
                    return;
                }
                match name {
                    "suche" => {
                        let fragen: Vec<&str> = d.get("fragen").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                        if !fragen.is_empty() {
                            status.set(Some(format!("Dieter sucht: {} …", fragen.join(" · "))));
                        }
                    }
                    "quellen" => {
                        let q = Quelle::liste(d.get("quellen").unwrap_or(&Value::Null));
                        status.set(Some(format!("{} Quellen gelesen - Dieter vergleicht …", q.len())));
                        quellen.set(q);
                    }
                    "material" => material.set(d.get("material").and_then(Value::as_array).cloned().unwrap_or_default()),
                    "text" => text.write().push_str(d.get("t").and_then(Value::as_str).unwrap_or_default()),
                    "fehler" => fehler = d.get("fehler").and_then(Value::as_str).unwrap_or_default().to_string(),
                    _ => {}
                }
            })
            .await;
            if let Err(f) = ergebnis {
                if fehler.is_empty() {
                    fehler = f.text;
                }
            }
            if !ctx.aktuell() {
                return;
            }
            if text.peek().trim().is_empty() {
                status.set(Some(if fehler.is_empty() { "Der Vergleich ist gerade nicht möglich.".into() } else { fehler }));
                aus.set(true);
                return;
            }
            status.set(None);
            fertig.set(true);
        }
    });

    let bl = bloecke(&nur_daten(&text.read()));
    let qs = quellen();
    let weiter = t("weiterfragen");
    let prompt = if fertig() { prompt_text(&seiten, &text.read(), &material.read(), &zusatz.read()) } else { String::new() };
    let frage = ctx.frage.clone();
    rsx! {
        ModulCss { name: "vergleich".to_string(), css: CSS }
        div { class: "vg-kopf",
            span { class: "ki-stern" }
            if hardware { "Benchmark" } else { "Vergleich" }
            span { class: "vg-titel",
                for (i, x) in seiten.iter().enumerate() {
                    if i > 0 {
                        span { class: "vg-vs", "vs" }
                    }
                    b { "{x}" }
                }
            }
        }
        if !kurse.read().is_empty() {
            div { class: "vg-kurse",
                for (i, k) in kurse.read().iter().enumerate() {
                    {
                        let f = |n: &str| k.get(n).map(wert_text).unwrap_or_default();
                        let kurs = k.get("kurs").and_then(Value::as_f64).unwrap_or(0.0);
                        let symbol = f("symbol");
                        let waehrung = f("waehrung");
                        let name = Some(f("name")).filter(|n| !n.is_empty()).unwrap_or_else(|| symbol.clone());
                        let link = Some(f("link")).filter(|l| !l.is_empty()).unwrap_or_else(|| "#".into());
                        let delta = k.get("vorher").and_then(Value::as_f64).filter(|v| *v != 0.0).map(|v| (kurs / v - 1.0) * 100.0);
                        rsx! {
                            a { key: "{i}", class: "vg-kurs", href: "{link}", target: "_blank", rel: "noreferrer",
                                span { class: "vg-kurs-name", "{name} " small { "{symbol}" } }
                                span { class: "vg-kurs-wert", "{zahl(kurs, kurs_stellen(kurs, &symbol))} {waehrung}" }
                                if let Some(d) = delta {
                                    span { class: if d >= 0.0 { "vg-kurs-delta hoch" } else { "vg-kurs-delta runter" },
                                        if d >= 0.0 { "▲ " } else { "▼ " }
                                        "{zahl(d.abs(), 1)} % in 1 Jahr"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(s) = status() {
            div { class: if aus() { "ki-status aus" } else { "ki-status" }, "{s}" }
        }
        div { class: "ki-text vg-text",
            if bl.is_empty() && !fertig() && !aus() {
                Geruest {}
            } else {
                {bloecke_zeigen(&bl, &qs, &[])}
            }
        }
        div { class: "vg-quellen",
            if fertig() && !qs.is_empty() {
                QuellenZeile { quellen: qs.clone() }
            }
        }
        if fertig() {
            div { class: "ki-fuss",
                KopierKnopf { text: prompt, beschriftung: "Prompt für andere KI kopieren", class: "vg-prompt" }
                button {
                    r#type: "button",
                    class: "ki-weiter",
                    onclick: move |_| {
                        let anfang = json!({ "frage": frage, "text": text(), "quellen": quellen() });
                        crate::pages::ki::weiterfragen(frage.clone(), Some(anfang));
                    },
                    span { class: "ki-stern" }
                    "{weiter}"
                }
            }
        }
    }
}
