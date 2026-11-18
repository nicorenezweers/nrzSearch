// nrzSearch - die KI: "Übersicht mit KI" und der KI-Modus (OpenRouter).
//
// Zu jeder Frage sucht der Server selbst in der Instanz, liest die ersten
// Seiten und gibt sie der KI als nummerierte Quellen [1], [2] ... - die
// Antwort kommt als Strom (Server-Sent Events). Im Gespraech formuliert die
// KI fuer jede Folgefrage eine eigene Suche, schaut also selbst nach.
//
// Der Schluessel kommt aus OPENROUTER_API_KEY oder aus der Datei
// OPENROUTER_KEY_FILE - eingetragen bei der Einrichtung (/admin).
// Gespeichert wird sonst nichts - nur die Uebersichten 30 Minuten im Speicher.

use crate::search::{suche_einmal, SucheParams};
use crate::util::{liste, s, seitentext};
use crate::statistik::Statistik;
use crate::AppState;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

type Ereignis = (String, Value);
/// So viele eigene Suchen stellt sich Dieter zu jeder Frage.
const SUCHEN: usize = 5;
/// Diagramme: Dieter liefert die Zahlen als JSON, der Browser zeichnet sie
/// (public/lib/diagramm.js).
const DIAGRAMM: &str = "Helfen Zahlen beim Verstehen (Wahlergebnisse, Umfragen, Statistiken, Entwicklungen über die Zeit, \
 Anteile, Benchmarks), füge genau ein Diagramm ein - als Codeblock mit der Sprache diagramm und gültigem JSON, zum Beispiel:\n\
 ```diagramm\n{\"typ\": \"balken\", \"titel\": \"Landtagswahl Sachsen-Anhalt 2021\", \"einheit\": \"%\", \"quellen\": [1], \
 \"daten\": [{\"name\": \"CDU\", \"wert\": 37.1, \"vorher\": 29.8}, {\"name\": \"AfD\", \"wert\": 20.8, \"vorher\": 24.3}]}\n```\n\
 Typen: \"balken\" (Kategorien und Wahlergebnisse; \"vorher\" ist freiwillig der Vergleichswert, z. B. die letzte Wahl), \
 \"torte\" (Anteile an einem Ganzen, aufgebaut wie balken) und \"linie\" (Entwicklung über die Zeit: {\"typ\": \"linie\", \
 \"titel\": \"…\", \"einheit\": \"…\", \"reihen\": [{\"name\": \"…\", \"punkte\": [[\"2020\", 1.5], [\"2021\", 2.1]]}]}). \
 Freiwillig je Eintrag \"farbe\" als #rrggbb (bei Parteien die Parteifarbe). Nur Zahlen aus den Quellen - ohne passende \
 Zahlen kein Diagramm. Das Diagramm ersetzt keine Antwort: schreib den Text trotzdem.";
/// So viel Text aus den Dateien des Arbeitsbereichs geht mit (Zeichen).
const DATEI_ZEICHEN: usize = 160_000;
/// So viel vom bisherigen Gespraech geht mit (Zeichen).
const VERLAUF_ZEICHEN: usize = 80_000;
const UEBERSICHT_HALTEN: Duration = Duration::from_secs(30 * 60);

pub struct Ki {
    url: String,
    modell: RwLock<String>,
    datei: String,
    schluessel: RwLock<Option<String>>,
    /// Uebersetzungen: (Ziel, Text) -> (Zeit, Ausgangssprache, Uebersetzung).
    uebersetzungen: Mutex<HashMap<(String, String), (Instant, String, String)>>,
    /// Uebersichten und Vergleiche: Quellen, Text, Material (fuer den Prompt).
    uebersichten: Mutex<HashMap<String, (Instant, Value, String, Value)>>,
    statistik: Arc<Statistik>,
}

impl Ki {
    pub fn neu(url: String, modell: String, datei: String, statistik: Arc<Statistik>) -> Self {
        let schluessel = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .or_else(|| std::fs::read_to_string(&datei).ok())
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());
        // Ein anderes Modell kann neben dem Schluessel liegen (<datei>.modell).
        let modell = std::fs::read_to_string(format!("{datei}.modell")).ok().map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).unwrap_or(modell);
        Self { url, modell: RwLock::new(modell), datei, schluessel: RwLock::new(schluessel), uebersichten: Default::default(), uebersetzungen: Default::default(), statistik }
    }

    pub async fn status(&self) -> Value {
        json!({"bereit": self.schluessel.read().await.is_some(), "modell": *self.modell.read().await})
    }

    /// Fuer den Admin: Schluessel ersetzen und/oder das Modell waehlen.
    pub async fn admin_setzen(&self, client: &reqwest::Client, schluessel: Option<&str>, modell: Option<&str>) -> Result<(), String> {
        if let Some(k) = schluessel.map(str::trim).filter(|k| !k.is_empty()) {
            let pruefung = client.get(format!("{}/key", self.url)).bearer_auth(k).timeout(Duration::from_secs(10)).send().await;
            if !matches!(pruefung, Ok(a) if a.status().is_success()) {
                return Err("OpenRouter kennt diesen Schlüssel nicht.".into());
            }
            std::fs::write(&self.datei, k).map_err(|e| format!("Speichern ging nicht ({}): {e}", self.datei))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&self.datei, std::fs::Permissions::from_mode(0o600));
            }
            *self.schluessel.write().await = Some(k.to_string());
        }
        if let Some(m) = modell.map(str::trim).filter(|m| !m.is_empty()) {
            if m.len() > 100 || !m.chars().all(|c| c.is_ascii_alphanumeric() || "/:-_.".contains(c)) {
                return Err("Das ist kein gültiger Modellname (z. B. openrouter/free).".into());
            }
            std::fs::write(format!("{}.modell", self.datei), m).map_err(|e| format!("Speichern ging nicht: {e}"))?;
            *self.modell.write().await = m.to_string();
        }
        Ok(())
    }

    async fn anfrage(&self, client: &reqwest::Client, nachrichten: &Value, strom: bool, max: u32) -> Result<reqwest::Response, String> {
        let k = self.schluessel.read().await.clone().ok_or("Kein OpenRouter-Schlüssel eingetragen.")?;
        // "openrouter/free" nimmt jedes Mal ein anderes Modell - lehnt eins
        // ab, kommt beim naechsten Versuch ein anderes dran.
        let mut fehler = String::new();
        for versuch in 0..3 {
            // usage: OpenRouter meldet die verbrauchten Tokens (fuer die Statistik).
            let mut koerper = json!({"model": *self.modell.read().await, "messages": nachrichten, "stream": strom, "max_tokens": max, "usage": {"include": true}});
            // Ohne Nachdenken: die freien Modelle verbrauchen sonst Zeit und
            // Tokens damit, und die Antwort kommt erst nach 20 Sekunden.
            // Manche muessen nachdenken - beim letzten Versuch duerfen sie.
            if versuch < 2 {
                koerper["reasoning"] = json!({"enabled": false});
            }
            let antwort = client
                .post(format!("{}/chat/completions", self.url))
                .bearer_auth(&k)
                .header("X-Title", "nrzSearch")
                .timeout(Duration::from_secs(120))
                .json(&koerper)
                .send()
                .await
                .map_err(|_| "OpenRouter ist nicht erreichbar.".to_string())?;
            match antwort.status().as_u16() {
                200..=299 => return Ok(antwort),
                401 | 403 => return Err("Der OpenRouter-Schlüssel wird nicht angenommen.".into()),
                402 => return Err("Bei OpenRouter ist kein Guthaben mehr da.".into()),
                429 => fehler = "Die kostenlosen KI-Modelle sind gerade ausgelastet - bitte gleich noch einmal.".into(),
                c => {
                    let v: Value = antwort.json().await.unwrap_or_default();
                    let grund = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("");
                    fehler = format!("OpenRouter antwortet mit Fehler {c}. {grund}").trim().to_string();
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        Err(fehler)
    }

    /// Eine kurze Antwort in einem Stueck (fuer die Suchwoerter). Kommt ein
    /// Pruefmodell dran, ist das keine Antwort - dann ein anderes.
    async fn kurz_fragen(&self, client: &reqwest::Client, nachrichten: Value, max: u32) -> Option<String> {
        for _ in 0..3 {
            let v: Value = self.anfrage(client, &nachrichten, false, max).await.ok()?.json().await.ok()?;
            self.statistik.tokens(v.pointer("/usage/total_tokens").and_then(Value::as_u64).unwrap_or(0));
            if nur_pruefer(s(&v, "model")) {
                continue;
            }
            return v.pointer("/choices/0/message/content").and_then(Value::as_str).map(str::to_string);
        }
        None
    }

    /// Die Antwort als Strom: jedes Stueck geht sofort als "text" raus.
    async fn stroemen(&self, client: &reqwest::Client, nachrichten: Value, max: u32, tx: &mpsc::Sender<Ereignis>) -> Result<String, String> {
        let mut strom = self.anfrage(client, &nachrichten, true, max).await?.bytes_stream();
        let (mut puffer, mut ganz) = (Vec::<u8>::new(), String::new());
        while let Some(stueck) = strom.next().await {
            puffer.extend_from_slice(&stueck.map_err(|_| "Die Verbindung zu OpenRouter ist abgebrochen.".to_string())?);
            // Nur ganze Zeilen lesen - ein Zeichen kann ueber zwei Stuecke gehen.
            while let Some(ende) = puffer.iter().position(|&b| b == b'\n') {
                let zeile: Vec<u8> = puffer.drain(..=ende).collect();
                let zeile = String::from_utf8_lossy(&zeile);
                let Some(daten) = zeile.trim().strip_prefix("data:").map(str::trim) else { continue };
                if daten == "[DONE]" {
                    return Ok(ganz);
                }
                let Ok(v) = serde_json::from_str::<Value>(daten) else { continue };
                if let Some(f) = v.pointer("/error/message").and_then(Value::as_str) {
                    return Err(format!("OpenRouter: {f}"));
                }
                if let Some(n) = v.pointer("/usage/total_tokens").and_then(Value::as_u64) {
                    self.statistik.tokens(n);
                }
                // Ein Pruefmodell ("User Safety: safe") - nichts davon zeigen,
                // der Aufrufer versucht ein anderes.
                if ganz.is_empty() && nur_pruefer(s(&v, "model")) {
                    return Ok(String::new());
                }
                if let Some(t) = v.pointer("/choices/0/delta/content").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                    ganz.push_str(t);
                    if tx.send(("text".into(), json!({"t": t}))).await.is_err() {
                        return Ok(ganz);
                    }
                }
            }
        }
        Ok(ganz)
    }
}

/// Eine Seite lesen - nur HTML, nur der Text, hoechstens `max` Zeichen.
async fn seite_lesen(client: &reqwest::Client, url: &str, max: usize) -> Option<String> {
    let antwort = client
        .get(url)
        .header("accept", "text/html")
        .header("user-agent", concat!("Mozilla/5.0 (compatible; nrzSearch/", env!("CARGO_PKG_VERSION"), ")"))
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .ok()?;
    let html = antwort.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|c| c.contains("html"));
    if !antwort.status().is_success() || !html {
        return None;
    }
    let roh = antwort.bytes().await.ok()?;
    let text = seitentext(&String::from_utf8_lossy(&roh[..roh.len().min(3_000_000)]));
    (text.len() > 200).then(|| text.chars().take(max).collect())
}

/// POST /api/ki - { frage, verlauf: [{rolle: "ich"|"ki", text}], kurz, sprache,
/// art: "vergleich" mit seiten: ["A", "B"] und zusatz (z. B. Kursdaten) }.
/// Ereignisse: suche, quellen, material (beim Vergleich), text, fehler, fertig.
pub fn starten(st: Arc<AppState>, anfrage: Value) -> mpsc::Receiver<Ereignis> {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        if let Err(f) = ablauf(&st, &anfrage, &tx).await {
            let _ = tx.send(("fehler".into(), json!({"fehler": f}))).await;
        }
        let _ = tx.send(("fertig".into(), json!({}))).await;
    });
    rx
}

async fn ablauf(st: &AppState, anfrage: &Value, tx: &mpsc::Sender<Ereignis>) -> Result<(), String> {
    let ki = &st.ki;
    let frage: String = s(anfrage, "frage").trim().chars().take(1000).collect();
    if frage.is_empty() {
        return Err("Die Frage fehlt.".into());
    }
    let kurz = anfrage["kurz"].as_bool() == Some(true);
    // Der Vergleich: "A vs B" - Tabelle, Staerken, Fazit, und das gelesene
    // Material geht mit an den Browser (fuer den Prompt an eine andere KI).
    let seiten: Vec<String> = liste(anfrage, "seiten").iter().filter_map(Value::as_str).map(|x| x.chars().take(80).collect()).take(4).collect();
    let vergleich = s(anfrage, "art") == "vergleich" && seiten.len() >= 2;
    let zusatz: String = s(anfrage, "zusatz").chars().take(3000).collect();
    // Der Arbeitsbereich: eigene Dateien (als Text) und Bilder (als data:-URL).
    let dateien: Vec<(String, String)> = liste(anfrage, "dateien").iter().map(|d| (s(d, "name").to_string(), s(d, "text").to_string())).collect();
    let bilder: Vec<String> = liste(anfrage, "bilder").iter().filter_map(|b| b.get("daten").and_then(Value::as_str)).filter(|u| u.starts_with("data:image/")).take(8).map(str::to_string).collect();
    let arbeit = !dateien.is_empty() || !bilder.is_empty();
    let sprache = Some(s(anfrage, "sprache")).filter(|x| !x.is_empty()).unwrap_or("de").to_string();
    // Das ganze Gespraech - nur bei sehr langen faellt der Anfang weg.
    let mut verlauf: Vec<Value> = Vec::new();
    let mut platz = VERLAUF_ZEICHEN;
    for n in liste(anfrage, "verlauf").iter().rev() {
        let text: String = s(n, "text").chars().take(12_000).collect();
        if text.len() > platz {
            break;
        }
        platz -= text.len();
        let rolle = if s(n, "rolle") == "ki" { "assistant" } else { "user" };
        verlauf.insert(0, json!({"role": rolle, "content": text}));
    }

    // Die Uebersicht (der Vergleich) zur selben Frage steht noch im Speicher?
    let merken = (kurz || vergleich) && !arbeit;
    let merk = format!("{sprache}\n{vergleich}\n{}", frage.to_lowercase());
    if merken {
        let treffer = ki.uebersichten.lock().unwrap_or_else(|e| e.into_inner()).get(&merk).filter(|e| e.0.elapsed() < UEBERSICHT_HALTEN).cloned();
        if let Some((_, quellen, text, material)) = treffer {
            let _ = tx.send(("quellen".into(), json!({"quellen": quellen}))).await;
            if vergleich {
                let _ = tx.send(("material".into(), json!({"material": material}))).await;
            }
            let _ = tx.send(("text".into(), json!({"t": text}))).await;
            return Ok(());
        }
    }

    // 1. Was suchen? Dieter stellt sich im Hintergrund selbst SUCHEN Fragen
    // aus verschiedenen Blickwinkeln - gleichzeitig laeuft schon die Suche
    // nach der Frage selbst (bei einer Folgefrage nicht: die steht nicht
    // fuer sich allein).
    let eigene = async {
        let auftrag = if arbeit {
            let namen = dateien.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
            format!("Der Nutzer arbeitet mit eigenen Dateien ({namen}). Nur wenn die Antwort zusätzlich aktuelles Wissen aus dem Netz \
             braucht, schreib bis zu 3 kurze Websuchen - sonst schreib nur das Wort KEINE.")
        } else if vergleich {
            format!("Schreib {SUCHEN} verschiedene kurze Websuchen, um {} gründlich zu vergleichen: technische Daten bzw. Kennzahlen \
             jeder Seite, Benchmarks oder Tests, direkte Vergleiche, Preise, aktuelle Nachrichten. Gern auf Englisch, wenn es dort \
             bessere Daten gibt.", seiten.join(" und "))
        } else {
            format!("Schreib {SUCHEN} verschiedene kurze Websuchen, die zusammen die letzte Frage des Gesprächs gründlich beantworten - \
             jede aus einem anderen Blickwinkel, in der Sprache der Frage.")
        };
        let mut n = vec![json!({"role": "system", "content": format!(
            "Du bist Dieter, der KI-Assistent der Suchmaschine nrzSearch. {auftrag} Eine Suche pro Zeile, nur die Suchwörter, \
             ohne Nummern, ohne Erklärung.")})];
        n.extend(verlauf.iter().cloned());
        n.push(json!({"role": "user", "content": frage}));
        ki.kurz_fragen(&st.client, Value::Array(n), 200)
            .await
            .filter(|t| !(arbeit && t.to_uppercase().contains("KEINE")))
            .map(|t| suchen_lesen(&t))
            .unwrap_or_default()
    };
    let einmal = |q: String| {
        let sprache = sprache.clone();
        async move {
            let params = HashMap::from([("q".to_string(), q), ("sprache".to_string(), sprache)]);
            suche_einmal(st, SucheParams::neu(params)).await.map(|d| liste(&d, "treffer").to_vec()).unwrap_or_default()
        }
    };
    let (mut fragen, erste) = tokio::join!(eigene, async {
        if verlauf.is_empty() && !arbeit { einmal(frage.clone()).await } else { Vec::new() }
    });
    if fragen.is_empty() && !verlauf.is_empty() && !arbeit {
        fragen.push(frage.clone());
    }
    let _ = tx.send(("suche".into(), json!({"q": fragen.first().unwrap_or(&frage), "fragen": fragen}))).await;

    // Hoert der Browser nicht mehr zu (Link geklickt, neue Suche): aufhoeren.
    if tx.is_closed() {
        return Ok(());
    }
    // 2. Alle Suchen gleichzeitig, zusammengefuehrt, 3. die ersten Seiten lesen.
    let mut listen = vec![erste];
    listen.extend(futures_util::future::join_all(fragen.iter().map(|q| einmal(q.clone()))).await);
    let treffer = zusammenfuehren(&listen, if kurz { 8 } else if vergleich { 12 } else { 10 });
    let (lesen, laenge) = if kurz { (3, 1800) } else if vergleich { (7, 3000) } else { (5, 3000) };
    let texte = futures_util::future::join_all(treffer.iter().take(lesen).map(|t| seite_lesen(&st.client, s(t, "url"), laenge))).await;
    if tx.is_closed() {
        return Ok(());
    }
    let quellen: Vec<Value> = treffer
        .iter()
        .enumerate()
        .map(|(i, t)| json!({"nr": i + 1, "titel": t["titel"], "url": t["url"], "host": t["host"], "text": t["text"]}))
        .collect();
    let _ = tx.send(("quellen".into(), json!({"quellen": quellen}))).await;

    let mut kontext = String::new();
    let mut material = Vec::new();
    for (i, t) in treffer.iter().enumerate() {
        let inhalt = texte.get(i).cloned().flatten().unwrap_or_else(|| s(t, "text").to_string());
        kontext += &format!("[{}] {} - {}\n{inhalt}\n\n", i + 1, s(t, "titel"), s(t, "url"));
        material.push(json!({"nr": i + 1, "titel": t["titel"], "url": t["url"], "auszug": inhalt.chars().take(2000).collect::<String>()}));
    }
    if kontext.is_empty() && !arbeit {
        kontext = "(Die Suche hat nichts gefunden.)".into();
    }
    // Die Dateien des Nutzers - zusammen hoechstens DATEI_ZEICHEN, jede
    // bekommt ihren Teil.
    if !dateien.is_empty() {
        let anteil = DATEI_ZEICHEN / dateien.len();
        let mut teil = String::from("Die Dateien des Nutzers im Arbeitsbereich:\n\n");
        for (i, (name, text)) in dateien.iter().enumerate() {
            let gekuerzt = text.chars().count() > anteil;
            let inhalt: String = text.chars().take(anteil).collect();
            teil += &format!("[D{}] {name}\n{inhalt}{}\n\n", i + 1, if gekuerzt { "\n(… gekürzt)" } else { "" });
        }
        kontext = format!("{teil}{}", if kontext.is_empty() { String::new() } else { format!("Dazu aus dem Netz:\n\n{kontext}") });
    }
    if !zusatz.is_empty() {
        kontext += &format!("Weitere Daten von nrzSearch (aktuell):\n{zusatz}\n\n");
    }
    let material = Value::Array(material);
    if vergleich {
        let _ = tx.send(("material".into(), json!({"material": material}))).await;
    }

    let heute = chrono::Utc::now().format("%d.%m.%Y");
    let system = if arbeit {
        format!("Du bist Dieter, der KI-Assistent von nrzSearch - hier im Arbeitsbereich. Der Nutzer hat eigene Dateien hochgeladen \
         (unten als [D1], [D2] …; Bilder hängen an seiner Nachricht). Du bist ein kompetenter, verlässlicher Assistent: du fasst \
         zusammen, beantwortest Fragen zu den Dateien, findest Details, vergleichst, rechnest, übersetzt und schreibst Texte wie \
         E-Mails, Briefe, Antworten oder Berichte. Heute ist der {heute}.\n\
         Regeln: Stütze dich auf die Dateien und nenne sie als [D1], [D2] …; Quellen aus dem Netz als [1], [2] …. Erfinde nichts, \
         was nicht in den Dateien steht - fehlt etwas, sag es. Schreibst du einen Text zum Weiterverwenden (E-Mail, Brief, Antwort), \
         setze genau diesen Text in einen Codeblock mit der Sprache text (```text … ```), mit Betreff, wenn es eine E-Mail ist - \
         davor und danach höchstens ein kurzer Satz. Sonst gegliedert mit ### Zwischenüberschriften, Stichpunkten und **Fettem**. \
         Antworte in der Sprache des Nutzers.\n{DIAGRAMM}")
    } else if vergleich {
        let spalten = seiten.join(" | ");
        let reihen = seiten.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(", ");
        format!("Du bist Dieter, der KI-Assistent von nrzSearch. Erstelle aus den Quellen, die nrzSearch gerade gesucht und gelesen \
         hat, einen Benchmark-Vergleich von {} - NUR Diagramme und eine Tabelle, KEIN anderer Text: keine Einleitung, keine \
         Überschriften, keine Erklärungen, kein Fazit. Heute ist der {heute}. Gib genau aus:\n\
         1. Ein Diagramm \"Leistung gesamt\" als ```diagramm mit typ \"balken\", einheit \"%\", die stärkste Seite = 100 und die \
         anderen relativ dazu (aus Benchmarks in den Quellen; bei Aktien statt dessen die Entwicklung in % über 1 Jahr, bei \
         KI-Modellen ein bekannter Benchmark-Wert).\n\
         2. Zwei bis drei Diagramme als ```diagramm mit typ \"gruppen\", so aufgebaut: {{\"typ\": \"gruppen\", \"titel\": \"Spiele (1080p, \
         hoch)\", \"einheit\": \"FPS\", \"besser\": \"mehr\", \"quellen\": [1, 3], \"reihen\": [{reihen}], \"daten\": [{{\"name\": \
         \"Cyberpunk 2077\", \"werte\": [25, 78]}}]}} - die Werte in der Reihenfolge der reihen. Passend zum Thema: Grafikkarten \
         (FPS in Spielen, 3DMark/synthetische Werte, Stromverbrauch mit \"besser\": \"weniger\", Preis), Prozessoren (Cinebench, \
         Spiele, Verbrauch), Laptops/PCs/Handys (Geekbench, Akku, Gewicht, Preis), Aktien (Umsatz, Gewinn, KGV, Marktwert), \
         KI-Modelle (Benchmarks wie MMLU, GPQA, SWE-bench, Preis pro Mio. Tokens mit \"besser\": \"weniger\").\n\
         3. Eine Markdown-Tabelle mit dem Kopf | Merkmal | {spalten} | und 8 bis 14 Zeilen technischer Daten bzw. Kennzahlen, jede \
         Zelle knapp mit Zahl, Einheit und Quelle in eckigen Klammern, z. B. 4 GB GDDR5 [2]. Unbekannt: –.\n\
         Nur Zahlen aus den Quellen (oder sicher bekannte Herstellerangaben) - nichts erfinden, lieber ein Diagramm weniger. \
         Jedes Diagramm als eigener ```diagramm-Block, die Tabelle als Markdown - kein gemeinsames JSON-Objekt. \
         Gültiges JSON, Zahlen ohne Einheit.", seiten.join(" und "))
    } else if kurz {
        format!("Du bist Dieter, der KI-Assistent von nrzSearch - einer eigenen, privaten Suchmaschine ohne Tracking, die ihre Treffer \
         aus einer eigenen SearXNG-Instanz holt. Du schreibst die \"Übersicht mit KI\" über den Suchergebnissen. Heute ist der {heute}. Beantworte die Suchanfrage knapp aus den Quellen: \
         ein Satz als direkte Antwort (das Wichtigste **fett**), danach höchstens 4 kurze Stichpunkte mit **Fettem Begriff:** Erklärung, \
         gern unter einer Zwischenüberschrift mit ###. Höchstens 130 Wörter. Belege jede Aussage mit der Nummer der Quelle in eckigen Klammern, \
         z. B. [1] oder [2][3]. Erfinde nichts, was nicht in den Quellen steht. Antworte in der Sprache der Suchanfrage.\n{DIAGRAMM}")
    } else {
        format!("Du bist Dieter, der KI-Assistent von nrzSearch - einer eigenen, privaten Suchmaschine ohne Tracking, die ihre Treffer \
         aus einer eigenen SearXNG-Instanz holt. Hier bist du im KI-Modus von nrzSearch. Zu jeder Frage hat nrzSearch für dich im Netz \
         gesucht und die wichtigsten Seiten gelesen - das sind die Quellen. Fragt jemand, wer du bist, stell dich als Dieter von nrzSearch \
         vor - nenne kein Modell und keinen Anbieter. Heute ist der {heute}. Beantworte die Frage gründlich, aber auf den Punkt, \
         aus den Suchergebnissen: zuerst die direkte Antwort, dann Einzelheiten mit ### Zwischenüberschriften, Stichpunkten und **Fettem**. \
         Belege Aussagen mit der Nummer der Quelle in eckigen Klammern, z. B. [1] oder [2][4]. Steht etwas nicht in den Quellen, sag es \
         ehrlich oder nutze vorsichtig dein Wissen. Antworte in der Sprache der Frage.\n{DIAGRAMM}")
    };
    // Die Sprache, die der Nutzer unten gewaehlt hat.
    let system = if sprache == "all" { system } else { format!("{system}\nAntworte in der Sprache mit dem Code „{sprache}“.") };
    let mut nachrichten = vec![json!({"role": "system", "content": system})];
    nachrichten.extend(verlauf);
    let gesucht = if fragen.is_empty() { frage.clone() } else { fragen.join("“, „") };
    let bezug = if nachrichten.len() > 1 { "Beantworte die letzte Nachricht im Zusammenhang des ganzen bisherigen Gesprächs. " } else { "" };
    let text = if arbeit && fragen.is_empty() {
        format!("{kontext}{bezug}Nachricht: {frage}")
    } else {
        format!("nrzSearch hat für dich gesucht nach „{gesucht}“. Die Ergebnisse:\n\n{kontext}{bezug}Nachricht: {frage}")
    };
    // Bilder gehen als Teile der Nachricht mit (nur Modelle, die sehen koennen).
    let inhalt = if bilder.is_empty() {
        json!(text)
    } else {
        let mut teile = vec![json!({"type": "text", "text": text})];
        teile.extend(bilder.iter().map(|u| json!({"type": "image_url", "image_url": {"url": u}})));
        Value::Array(teile)
    };
    nachrichten.push(json!({"role": "user", "content": inhalt}));

    // 4. Die Antwort als Strom.
    // Liefert ein Modell nichts (es hat nur nachgedacht), versucht es ein anderes.
    let nachrichten = Value::Array(nachrichten);
    let mut text = String::new();
    for _ in 0..3 {
        if tx.is_closed() {
            return Ok(());
        }
        text = ki.stroemen(&st.client, nachrichten.clone(), if vergleich { 3000 } else { 1500 }, tx).await?;
        if !text.trim().is_empty() {
            break;
        }
    }
    if text.trim().is_empty() {
        return Err("Die KI hat keine Antwort gegeben - bitte noch einmal.".into());
    }
    ki.statistik.antwort();
    if merken {
        let mut speicher = ki.uebersichten.lock().unwrap_or_else(|e| e.into_inner());
        if speicher.len() > 300 {
            speicher.clear();
        }
        speicher.insert(merk, (Instant::now(), Value::Array(quellen), text, material));
    }
    Ok(())
}

/// Die Suchen aus Dieters Antwort: eine pro Zeile, ohne Nummern und Zeichen.
fn suchen_lesen(text: &str) -> Vec<String> {
    let mut fragen: Vec<String> = Vec::new();
    for zeile in text.lines() {
        let q = zeile
            .trim()
            .trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | '*' | '•' | ' '))
            .trim_matches(['"', '„', '“', '\'', '`'])
            .trim();
        if !q.is_empty() && q.len() < 150 && !fragen.iter().any(|f| f.eq_ignore_ascii_case(q)) {
            fragen.push(q.to_string());
        }
    }
    fragen.truncate(SUCHEN);
    fragen
}

/// Die Treffer aller Suchen zusammenfuehren: Punkte nach Platz, die Frage
/// selbst zaehlt etwas mehr. Eine Seite, die mehrere Suchen finden, steht oben.
fn zusammenfuehren(listen: &[Vec<Value>], max: usize) -> Vec<Value> {
    let mut punkte: Vec<(f64, Value)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (k, liste) in listen.iter().enumerate() {
        let gewicht = if k == 0 { 1.5 } else { 1.0 };
        for (platz, t) in liste.iter().take(10).enumerate() {
            let wert = gewicht / (platz as f64 + 1.0);
            match index.get(s(t, "url")) {
                Some(&n) => punkte[n].0 += wert,
                None => {
                    index.insert(s(t, "url").to_string(), punkte.len());
                    punkte.push((wert, t.clone()));
                }
            }
        }
    }
    punkte.sort_by(|a, b| b.0.total_cmp(&a.0));
    punkte.into_iter().take(max).map(|(_, t)| t).collect()
}

/// Ein Modell, das nur Inhalte prueft (z. B. "nemotron-content-safety")
/// und "User Safety: safe" antwortet - "openrouter/free" waehlt es manchmal.
fn nur_pruefer(modell: &str) -> bool {
    let m = modell.to_lowercase();
    ["safety", "guard", "shield", "moderat"].iter().any(|w| m.contains(w))
}

/// Sprachen, in die Dieter uebersetzt (Code, englischer Name fuer den Prompt).
const UEBERSETZ_SPRACHEN: [(&str, &str); 40] = [
    ("de", "German"), ("en", "English"), ("fr", "French"), ("es", "Spanish"), ("it", "Italian"), ("pt", "Portuguese"),
    ("nl", "Dutch"), ("pl", "Polish"), ("ru", "Russian"), ("uk", "Ukrainian"), ("tr", "Turkish"), ("ar", "Arabic"),
    ("fa", "Persian"), ("he", "Hebrew"), ("hi", "Hindi"), ("bn", "Bengali"), ("ur", "Urdu"), ("ta", "Tamil"), ("te", "Telugu"),
    ("mr", "Marathi"), ("zh", "Chinese (Simplified)"), ("ja", "Japanese"), ("ko", "Korean"), ("th", "Thai"), ("vi", "Vietnamese"),
    ("id", "Indonesian"), ("ms", "Malay"), ("sv", "Swedish"), ("da", "Danish"), ("no", "Norwegian"), ("fi", "Finnish"),
    ("cs", "Czech"), ("sk", "Slovak"), ("hu", "Hungarian"), ("ro", "Romanian"), ("bg", "Bulgarian"), ("el", "Greek"),
    ("hr", "Croatian"), ("la", "Latin"), ("sw", "Swahili"),
];

/// Uebersetzen: in die Sprache `ziel` - zurueck kommt die erkannte
/// Ausgangssprache und die Uebersetzung. Gleiche Texte kommen aus dem
/// Speicher (eine Stunde), ohne neue Tokens.
pub async fn uebersetzen(st: &AppState, text: &str, ziel: &str) -> Result<(String, String), String> {
    let text: String = text.trim().chars().take(3000).collect();
    let Some((ziel, name)) = UEBERSETZ_SPRACHEN.iter().find(|(c, _)| *c == ziel) else {
        return Err("Diese Sprache kenne ich nicht.".into());
    };
    if text.is_empty() {
        return Ok((String::new(), String::new()));
    }
    let schluessel = (ziel.to_string(), text.clone());
    let gemerkt = st.ki.uebersetzungen.lock().unwrap_or_else(|e| e.into_inner()).get(&schluessel).filter(|e| e.0.elapsed() < Duration::from_secs(3600)).cloned();
    if let Some((_, quelle, aus)) = gemerkt {
        return Ok((quelle, aus));
    }
    let nachrichten = json!([
        {"role": "system", "content": format!("You are a professional translator. Translate the user's text into {name}. \
          Keep the meaning, tone, formatting and line breaks; do not add explanations, quotes or notes. \
          Answer in exactly this form: first line the ISO 639-1 code of the source language in square brackets, e.g. [de], \
          then the translation on the following lines.")},
        {"role": "user", "content": text},
    ]);
    let max = (text.chars().count() as u32 * 3).clamp(200, 3000);
    let antwort = st.ki.kurz_fragen(&st.client, nachrichten, max).await.ok_or("Die Übersetzung ist gerade nicht möglich - bitte gleich noch einmal.")?;
    let antwort = antwort.trim();
    let (quelle, aus) = match antwort.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((code, rest)) if code.len() <= 5 => (code.trim().to_lowercase(), rest.trim().to_string()),
        _ => (String::new(), antwort.to_string()),
    };
    if aus.is_empty() {
        return Err("Die Übersetzung ist leer geblieben - bitte noch einmal.".into());
    }
    let mut speicher = st.ki.uebersetzungen.lock().unwrap_or_else(|e| e.into_inner());
    if speicher.len() > 500 {
        speicher.clear();
    }
    speicher.insert(schluessel, (Instant::now(), quelle.clone(), aus.clone()));
    Ok((quelle, aus))
}
