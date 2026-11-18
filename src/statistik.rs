// nrzSearch - anonyme Zaehler fuer den Footer: wie viele Suchen, wie viele
// KI-Antworten, wie viele Tokens. Keine Fragen, keine IPs, keine Personen -
// nur drei Zahlen (gesamt und je Tag, 60 Tage), gespeichert in einer kleinen
// Datei (STATISTIK_DATEI).

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

pub struct Statistik {
    datei: String,
    seit: String,
    suchen: AtomicU64,
    antworten: AtomicU64,
    tokens: AtomicU64,
    /// Je Tag: [Suchen, KI-Antworten, Tokens].
    tage: Mutex<BTreeMap<String, [u64; 3]>>,
    geaendert: AtomicBool,
}

impl Statistik {
    pub fn neu(datei: String) -> Self {
        let alt: Value = std::fs::read(&datei).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        let zahl = |k: &str| AtomicU64::new(alt[k].as_u64().unwrap_or(0));
        Self {
            seit: alt["seit"].as_str().map(str::to_string).unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string()),
            suchen: zahl("suchen"),
            antworten: zahl("antworten"),
            tokens: zahl("tokens"),
            tage: Mutex::new(serde_json::from_value(alt["tage"].clone()).unwrap_or_default()),
            geaendert: AtomicBool::new(false),
            datei,
        }
    }

    fn heute(&self, feld: usize, n: u64) {
        let mut tage = self.tage.lock().unwrap_or_else(|e| e.into_inner());
        tage.entry(chrono::Utc::now().format("%Y-%m-%d").to_string()).or_default()[feld] += n;
        while tage.len() > 60 {
            let erster = tage.keys().next().cloned().unwrap_or_default();
            tage.remove(&erster);
        }
        self.geaendert.store(true, Ordering::Relaxed);
    }

    pub fn suche(&self) {
        self.suchen.fetch_add(1, Ordering::Relaxed);
        self.heute(0, 1);
    }

    pub fn antwort(&self) {
        self.antworten.fetch_add(1, Ordering::Relaxed);
        self.heute(1, 1);
    }

    pub fn tokens(&self, n: u64) {
        if n > 0 {
            self.tokens.fetch_add(n, Ordering::Relaxed);
            self.heute(2, n);
        }
    }

    pub fn json(&self) -> Value {
        json!({
            "suchen": self.suchen.load(Ordering::Relaxed),
            "antworten": self.antworten.load(Ordering::Relaxed),
            "tokens": self.tokens.load(Ordering::Relaxed),
            "seit": self.seit,
            "tage": *self.tage.lock().unwrap_or_else(|e| e.into_inner()),
        })
    }

    /// Jede Minute speichern, wenn sich etwas getan hat.
    pub async fn speichern(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if self.geaendert.swap(false, Ordering::Relaxed) {
                let _ = tokio::fs::write(&self.datei, self.json().to_string()).await;
            }
        }
    }
}
