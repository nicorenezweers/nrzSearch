// nrzSearch - kleine Helfer, die an mehreren Stellen gebraucht werden.

use serde_json::Value;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

/// Ein Text aus einem JSON-Objekt ("" wenn es ihn nicht gibt).
pub fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Eine Liste aus einem JSON-Objekt (leer wenn es sie nicht gibt).
pub fn liste<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// Ein Wert in einer URL (alles ausser A-Z a-z 0-9 - _ . ~ maskiert).
pub fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Leerraum zusammenziehen - und Text, den eine Suchmaschine doppelt liefert
/// ("Ein Satz. Mehr.Ein Satz. Mehr."), nur einmal.
pub fn eine_zeile(s: &str) -> String {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    doppelt_weg(&t)
}

fn doppelt_weg(t: &str) -> String {
    let zeichen: Vec<char> = t.chars().collect();
    if zeichen.len() < 60 {
        return t.to_string();
    }
    let anfang: String = zeichen[..30].iter().collect();
    let rest: String = zeichen[30..].iter().collect();
    if let Some(p) = rest.find(&anfang) {
        let (vorn, hinten) = rest.split_at(p);
        let erster = format!("{anfang}{vorn}");
        // Die Wiederholung ist der Anfang des Textes (ganz oder abgeschnitten).
        if erster.trim().starts_with(hinten.trim().trim_end_matches(['…', '.', ' '])) || hinten.trim().starts_with(erster.trim()) {
            return erster.trim_end().to_string();
        }
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn doppelter_text() {
        let a = "Ein einfacher und schneller Rechner für alle Geräte, der auch algebraische Notation unterstützt.";
        assert_eq!(super::eine_zeile(&format!("{a}{a}")), a);
        assert_eq!(super::eine_zeile(a), a);
    }
}

/// JSON holen - nur eine Antwort mit Status 2xx zaehlt.
pub async fn hole(anfrage: reqwest::RequestBuilder) -> Option<Value> {
    let antwort = anfrage.header("accept", "application/json").send().await.ok()?;
    if !antwort.status().is_success() {
        return None;
    }
    antwort.json().await.ok()
}

/// Eine Bremse: zwischen zwei Anfragen an denselben Dienst liegt
/// mindestens `abstand` (Nominatim, Wikipedia und Wikidata sind streng).
pub struct Bremse {
    naechste: Mutex<Instant>,
    abstand: Duration,
}

impl Bremse {
    pub fn neu(abstand_ms: u64) -> Self {
        Self { naechste: Mutex::new(Instant::now()), abstand: Duration::from_millis(abstand_ms) }
    }

    /// Den naechsten freien Platz reservieren und bis dahin warten - die
    /// Sperre wird dabei nicht gehalten, wartende Anfragen stehen Schlange.
    pub async fn warten(&self) {
        let start = {
            let mut naechste = self.naechste.lock().await;
            let start = (*naechste).max(Instant::now());
            *naechste = start + self.abstand;
            start
        };
        tokio::time::sleep_until(start).await;
    }
}

/// Der Host aus einer Adresse (ohne "www.").
fn host_von(u: &reqwest::Url) -> String {
    let h = u.host_str().unwrap_or("");
    h.strip_prefix("www.").unwrap_or(h).to_string()
}

/// Der Host und die zweite Adresszeile wie bei der grossen Suchmaschine:
/// "origin › pfad › pfad".
pub fn host_und_pfad(url: &str) -> (String, String) {
    let Ok(u) = reqwest::Url::parse(url) else { return Default::default() };
    let mut pfad = format!("{}://{}", u.scheme(), u.host_str().unwrap_or(""));
    for teil in u.path().split('/').filter(|t| !t.is_empty()) {
        pfad.push_str(" › ");
        pfad.push_str(teil);
    }
    (host_von(&u), pfad)
}

/// Manche Maschinen liefern Bild-URLs relativ zur Suchinstanz.
pub fn bild_url(roh: &str, sxng: &str) -> String {
    let u = roh.trim();
    if u.is_empty() || u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else if let Some(rest) = u.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        format!("{sxng}/{}", u.trim_start_matches('/'))
    }
}

/// Das Datum aus dem ISO-Format der Instanz.
pub fn datum_von(roh: &str) -> Option<String> {
    let utc = |d: chrono::NaiveDateTime| d.and_utc().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    chrono::DateTime::parse_from_rfc3339(roh)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .ok()
        .or_else(|| {
            // Ohne Zeitzone (DuckDuckGo News, Reuters): "2026-10-21T07:34:00".
            let zeit = roh.get(..19)?.replace(' ', "T");
            chrono::NaiveDateTime::parse_from_str(&zeit, "%Y-%m-%dT%H:%M:%S").ok().map(utc)
        })
        .or_else(|| {
            let tag = chrono::NaiveDate::parse_from_str(roh.get(..10)?, "%Y-%m-%d").ok()?;
            Some(format!("{tag}T00:00:00.000Z"))
        })
        .or_else(|| vor(roh).map(|d| utc(chrono::Utc::now().naive_utc() - d)))
}

/// Relative Angaben wie bei Brave News: "vor 7 Stunden", "3 days ago", "gestern".
fn vor(roh: &str) -> Option<chrono::Duration> {
    let roh = roh.to_lowercase();
    if roh.contains("gestern") || roh.contains("yesterday") {
        return Some(chrono::Duration::days(1));
    }
    if !(roh.contains("vor") || roh.contains("ago")) {
        return None;
    }
    let mut woerter = roh.split_whitespace();
    let n: i64 = woerter.by_ref().find_map(|w| w.parse().ok())?;
    let einheit = woerter.next()?;
    let minuten = [("min", 1), ("stund", 60), ("std", 60), ("hour", 60), ("tag", 1440), ("day", 1440),
        ("woche", 10080), ("week", 10080), ("monat", 43200), ("month", 43200)]
        .iter()
        .find(|(w, _)| einheit.starts_with(w))?
        .1;
    Some(chrono::Duration::minutes(n * minuten))
}

/// Fuer den Vergleich: klein, ohne Umlaute, ohne Satzzeichen.
pub fn normieren(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        match c {
            'ä' => out.push('a'),
            'ö' => out.push('o'),
            'ü' => out.push('u'),
            'ß' => out.push_str("ss"),
            c if c.is_alphanumeric() => out.push(c),
            _ if !out.ends_with(' ') => out.push(' '),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Der lesbare Text einer Webseite: nur der Hauptteil (<main>/<article>, wenn
/// es ihn gibt), ohne Skripte, Stile, Navigation, Kopf und Fuss.
pub fn seitentext(html: &str) -> String {
    // Klein geschrieben sind die Positionen dieselben (nur ASCII aendert sich).
    let klein = html.to_ascii_lowercase();
    let start = ["<main", "<article"].iter().filter_map(|m| klein.find(m)).min().unwrap_or(0);
    let (html, klein) = (&html[start..], &klein[start..]);
    const WEG: [&str; 10] = ["script", "style", "noscript", "svg", "nav", "footer", "header", "form", "aside", "template"];
    let mut aus = String::with_capacity(html.len() / 4);
    let mut i = 0;
    while i < html.len() {
        if klein.as_bytes()[i] != b'<' {
            let n = klein[i..].find('<').map_or(html.len(), |e| i + e);
            aus.push_str(&html[i..n]);
            i = n;
            continue;
        }
        // Ein Block, dessen Inhalt nicht zaehlt: bis zu seinem Ende springen.
        let rest = &klein[i + 1..];
        let block = WEG.iter().find(|t| rest.starts_with(*t) && !rest[t.len()..].starts_with(|c: char| c.is_ascii_alphanumeric()));
        let j = block.map_or(i, |t| {
            let ende = format!("</{t}");
            klein[i..].find(&ende).map_or(html.len(), |e| i + e + ende.len())
        });
        i = klein.get(j..).and_then(|r| r.find('>')).map_or(html.len(), |e| j + e + 1);
        aus.push(' ');
    }
    let aus = aus
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    eine_zeile(&aus)
}
