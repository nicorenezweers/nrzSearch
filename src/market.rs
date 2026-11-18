// nrzSearch - die Kurskarte und das Wetter.
//
// Fragen wie "1 btc in eur" oder "apple kurs" bekommen die Kurskarte.
// Erste Quelle ist Yahoo Finance; lehnt Yahoo ab (429), kommen fuer Krypto
// Binance, Kraken und CoinGecko zum Zug, fuer Wechselkurspaare offene
// EZB-Kurse (mit Verlauf bis 1999). Die Karte zeigt immer den tatsaechlichen
// Kurs der Quelle, nie einen Schaetzwert.

use crate::util::{enc, hole, liste, s};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// Der Zeitraum der Kurskarte: Yahoo-Bereich, Takt, Tage fuer die EZB, Text.
fn zeitraum(bereich: &str) -> Option<(&'static str, &'static str, i64, &'static str)> {
    Some(match bereich {
        "1d" => ("1d", "5m", 5, "heute"),
        "5d" => ("5d", "15m", 10, "5 Tage"),
        "1mo" => ("1mo", "1d", 35, "1 Monat"),
        "6mo" => ("6mo", "1d", 185, "6 Monate"),
        "ytd" => ("1y", "1d", 400, "seit Jahresanfang"),
        "1y" => ("1y", "1d", 370, "1 Jahr"),
        "5y" => ("5y", "1wk", 1830, "5 Jahre"),
        "max" => ("max", "1mo", 10000, "gesamter Zeitraum"),
        _ => return None,
    })
}

/// Die Ersatznamen fuer ein Symbol (Yahoo kennt nicht alle).
fn alias(symbol: &str) -> &'static [&'static str] {
    match symbol {
        "^GDAXI" => &["^GDAXI", "^GDAXI.DE"],
        "^MDAXI" => &["^MDAXI", "^MDAXI.DE"],
        "^SDAXI" => &["^SDAXI", "^SDAXI.DE"],
        "^FTSE" => &["^FTSE", "^FTSE.L"],
        "^GSPC" => &["^GSPC", "^SPX"],
        "^N225" => &["^N225", "^NSEI"],
        "GC=F" => &["GC=F", "XAUUSD=X"],
        "SI=F" => &["SI=F", "XAGUSD=X"],
        _ => &[],
    }
}

/// Die Muenzen bei CoinGecko.
const COINGECKO: &[(&str, &str)] = &[
    ("BTC", "bitcoin"), ("ETH", "ethereum"), ("LTC", "litecoin"), ("XRP", "ripple"), ("SOL", "solana"),
    ("DOGE", "dogecoin"), ("ADA", "cardano"), ("BNB", "binancecoin"), ("DOT", "polkadot"),
    ("AVAX", "avalanche-2"), ("LINK", "chainlink"), ("TRX", "tron"),
];

/// Eine Zahl, die als Text kommt ("123.45").
fn zahl(v: &Value, k: &str) -> Option<f64> {
    s(v, k).parse().ok()
}

/// Ein Kurs ohne Verlauf (die Krypto-Boersen).
fn kurs_heute(quelle: &str, link: String, basis: &str, quote: &str, kurs: f64, vorher: Option<f64>, stand: i64) -> Value {
    json!({
        "quelle": quelle, "link": link, "name": format!("{basis}/{quote}"), "waehrung": quote,
        "kurs": kurs, "vorher": vorher, "verlauf": [], "stand": stand,
    })
}

#[derive(Default)]
pub struct Markt {
    /// Yahoo sperrt eine Weile, wenn es zu oft gefragt wird.
    yahoo_gesperrt_bis: Mutex<Option<Instant>>,
    /// Die Antworten (Yahoo eine Minute, EZB fuenf Minuten).
    cache: Mutex<HashMap<String, (Value, Instant)>>,
}

impl Markt {
    fn get(&self, client: &reqwest::Client, url: &str) -> reqwest::RequestBuilder {
        client.get(url).header("user-agent", USER_AGENT).timeout(Duration::from_secs(8))
    }

    async fn gemerkt(&self, schluessel: &str, max: Duration) -> Option<Value> {
        self.cache.lock().await.get(schluessel).filter(|(_, zeit)| zeit.elapsed() < max).map(|(v, _)| v.clone())
    }

    async fn merken(&self, schluessel: String, wert: &Value) {
        let mut cache = self.cache.lock().await;
        cache.retain(|_, (_, zeit)| zeit.elapsed() < Duration::from_secs(300));
        cache.insert(schluessel, (wert.clone(), Instant::now()));
    }

    /// Der Kurs von Yahoo mit dem Verlauf fuer den gewaehlten Zeitraum.
    async fn yahoo(&self, client: &reqwest::Client, symbol: &str, bereich: &str) -> Option<Value> {
        if self.yahoo_gesperrt_bis.lock().await.is_some_and(|bis| Instant::now() < bis) {
            return None;
        }
        let schluessel = format!("yahoo|{symbol}|{bereich}");
        if let Some(wert) = self.gemerkt(&schluessel, Duration::from_secs(60)).await {
            return Some(wert);
        }
        let (zeitraum, takt, ..) = zeitraum(bereich)?;
        let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?range={zeitraum}&interval={takt}", enc(symbol));
        let antwort = self.get(client, &url).send().await.ok()?;
        if antwort.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            *self.yahoo_gesperrt_bis.lock().await = Some(Instant::now() + Duration::from_secs(300));
            return None;
        }
        let roh: Value = antwort.error_for_status().ok()?.json().await.ok()?;
        let ergebnis = roh.pointer("/chart/result/0")?;
        let meta = ergebnis.get("meta")?;
        let zeiten = liste(ergebnis, "timestamp");
        let kurse = ergebnis.pointer("/indicators/quote/0/close").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
        let verlauf: Vec<Value> =
            zeiten.iter().zip(kurse).filter_map(|(z, k)| Some(json!([z.as_i64()?, k.as_f64()?]))).collect();
        let wert = json!({
            "quelle": "Yahoo Finance",
            "link": format!("https://finance.yahoo.com/quote/{}", enc(symbol)),
            "name": meta.get("longName").or(meta.get("shortName")).and_then(Value::as_str).unwrap_or(symbol),
            "waehrung": s(meta, "currency"),
            "kurs": meta.get("regularMarketPrice")?.as_f64()?,
            "vorher": meta.get("chartPreviousClose").or(meta.get("previousClose")).and_then(Value::as_f64),
            "verlauf": verlauf,
            "stand": meta.get("regularMarketTime").and_then(Value::as_i64).unwrap_or(0),
        });
        self.merken(schluessel, &wert).await;
        Some(wert)
    }

    /// Krypto bei Binance, Kraken und CoinGecko - der Reihe nach.
    async fn krypto(&self, client: &reqwest::Client, symbol: &str) -> Option<Value> {
        let (basis, quote) = symbol.split_once('-')?;

        let url = format!("https://api.binance.com/api/v3/ticker/24hr?symbol={basis}{quote}");
        if let Some(roh) = hole(self.get(client, &url)).await {
            if let Some(kurs) = zahl(&roh, "lastPrice") {
                let link = format!("https://www.binance.com/de/trading/{basis}_{quote}");
                let stand = roh.get("closeTime").and_then(Value::as_i64).unwrap_or(0) / 1000;
                return Some(kurs_heute("Binance", link, basis, quote, kurs, zahl(&roh, "openPrice"), stand));
            }
        }

        // Kraken nennt Bitcoin XBT und Dogecoin XDG.
        let k = match basis { "BTC" => "XBT", "DOGE" => "XDG", b => b };
        let url = format!("https://api.kraken.com/0/public/Ticker?pair={k}{quote}");
        if let Some(roh) = hole(self.get(client, &url)).await {
            if let Some(paar) = roh.get("result").and_then(Value::as_object).and_then(|r| r.values().next()) {
                if let Some(kurs) = paar.pointer("/c/0").and_then(Value::as_str).and_then(|k| k.parse().ok()) {
                    let link = format!("https://pro.kraken.com/app/trade/{k}{quote}");
                    return Some(kurs_heute("Kraken", link, k, quote, kurs, zahl(paar, "o"), 0));
                }
            }
        }

        let id = COINGECKO.iter().find(|(b, _)| *b == basis)?.1;
        let q = quote.to_lowercase();
        let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={id}&vs_currencies={q}&include_24hr_change=true");
        let roh = hole(self.get(client, &url)).await?;
        let preis = roh.get(id)?;
        let vorher = preis.get(format!("{q}_24h_change")).and_then(Value::as_f64);
        Some(kurs_heute("CoinGecko", "https://www.coingecko.com/".into(), basis, quote, preis.get(&q)?.as_f64()?, vorher, 0))
    }

    /// Der Wechselkurs der EZB (mit Verlauf zurueck bis 1999).
    async fn fiat(&self, client: &reqwest::Client, symbol: &str, bereich: &str) -> Option<Value> {
        // "EURUSD=X" -> von EUR nach USD
        let (von, nach) = (symbol.get(0..3)?, symbol.get(3..6)?);
        let tage = zeitraum(bereich)?.2;
        let schluessel = format!("fiat|{von}{nach}|{tage}");
        if let Some(wert) = self.gemerkt(&schluessel, Duration::from_secs(300)).await {
            return Some(wert);
        }
        let heute = chrono::Utc::now().date_naive();
        let url = format!("https://api.frankfurter.app/{}..{heute}?from={von}&to={nach}", heute - chrono::Duration::days(tage));
        let roh = hole(self.get(client, &url)).await?;
        // Die Tage kommen als "JJJJ-MM-TT" - sortiert wie die Zeit.
        let verlauf: Vec<Value> = roh
            .get("rates")
            .and_then(Value::as_object)?
            .iter()
            .filter_map(|(tag, werte)| {
                let stempel = chrono::NaiveDate::parse_from_str(tag, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0)?.and_utc().timestamp();
                Some(json!([stempel, werte.get(nach)?.as_f64()?]))
            })
            .collect();
        let kurs_von = |i: usize| verlauf.get(i).and_then(|v| v[1].as_f64());
        let wert = json!({
            "quelle": "EZB-Wechselkurs",
            "link": "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/eurofxref/",
            "name": format!("{von} / {nach}"),
            "waehrung": nach,
            "kurs": kurs_von(verlauf.len().checked_sub(1)?)?,
            "vorher": verlauf.len().checked_sub(2).and_then(kurs_von),
            "verlauf": verlauf,
            "stand": chrono::Utc::now().timestamp(),
        });
        self.merken(schluessel, &wert).await;
        Some(wert)
    }

    /// Der Kurs fuer ein Symbol - mit allen Ersatzquellen.
    pub async fn kurs(&self, client: &reqwest::Client, symbol: &str, bereich: &str) -> Value {
        let bereich = if zeitraum(bereich).is_some() { bereich } else { "1d" };
        let paar = symbol.len() == 7;
        let mut daten = None;
        // Yahoo zuerst (das ist die Karte, die man kennt).
        let eigenes = [symbol];
        let namen: &[&str] = match alias(symbol) {
            [] => &eigenes,
            a => a,
        };
        for name in namen {
            daten = self.yahoo(client, name, bereich).await;
            if daten.is_some() {
                break;
            }
        }
        if daten.is_none() && paar && symbol.contains('-') {
            daten = self.krypto(client, symbol).await;
        }
        if daten.is_none() && paar && symbol.ends_with("=X") {
            daten = self.fiat(client, symbol, bereich).await;
        }
        let mut wert = daten.unwrap_or_else(|| json!({"kurs": null, "quelle": null, "quelleText": "keine Kursquelle erreichbar"}));
        wert["symbol"] = json!(symbol);
        wert["bereich"] = json!(bereich);
        if wert["kurs"].is_number() {
            wert["zeitraum"] = json!(zeitraum(bereich).map(|z| z.3));
        }
        if s(&wert, "waehrung").is_empty() {
            wert["waehrung"] = json!(waehrung(symbol));
        }
        wert
    }
}

/// Die Waehrung eines Kurses (aus dem Symbol).
fn waehrung(symbol: &str) -> &str {
    match symbol.strip_suffix("=X") {
        Some(paar) if paar.len() == 6 => &paar[3..],
        _ => symbol.split_once('-').map_or("", |(_, quote)| quote),
    }
}

/// Die Wettervorhersage (Open-Meteo): jetzt, die naechsten 24 Stunden und
/// sieben Tage.
pub async fn wetter(client: &reqwest::Client, lat: f64, lon: f64) -> Option<Value> {
    if !(lat.abs() <= 90.0 && lon.abs() <= 180.0) {
        return None;
    }
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&timezone=auto&forecast_days=7&forecast_hours=24\
         &current=temperature_2m,apparent_temperature,weather_code,relative_humidity_2m,wind_speed_10m,precipitation,is_day\
         &hourly=temperature_2m,weather_code,precipitation_probability,wind_speed_10m,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset"
    );
    let roh = hole(client.get(url).timeout(Duration::from_secs(9))).await?;
    // Die Spalten von Open-Meteo in Zeilen.
    let zeilen = |block: &Value, felder: &[(&str, &str)]| -> Vec<Value> {
        liste(block, "time")
            .iter()
            .enumerate()
            .map(|(i, zeit)| {
                let mut zeile = json!({"zeit": zeit});
                for (name, feld) in felder {
                    zeile[*name] = block[*feld][i].clone();
                }
                zeile
            })
            .collect()
    };
    let tag = &roh["daily"];
    tag.get("time")?;
    let jetzt = &roh["current"];
    Some(json!({
        "aktuell": {
            "zeit": jetzt["time"], "temperatur": jetzt["temperature_2m"], "gefuehlt": jetzt["apparent_temperature"],
            "code": jetzt["weather_code"], "feuchte": jetzt["relative_humidity_2m"], "wind": jetzt["wind_speed_10m"],
            "niederschlag": jetzt["precipitation"], "tag": jetzt["is_day"],
        },
        "stunden": zeilen(&roh["hourly"], &[("temperatur", "temperature_2m"), ("code", "weather_code"), ("regen", "precipitation_probability"), ("wind", "wind_speed_10m"), ("tag", "is_day")]),
        "tage": zeilen(tag, &[("code", "weather_code"), ("max", "temperature_2m_max"), ("min", "temperature_2m_min"), ("regen", "precipitation_probability_max"), ("aufgang", "sunrise"), ("untergang", "sunset")]),
        "zeitzone": roh["timezone"],
        "quelle": "Open-Meteo",
        "link": "https://open-meteo.com/",
    }))
}
