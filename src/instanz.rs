// nrzSearch - die SearXNG-Instanz.
//
// Normalerweise die eingebaute (siehe searxng.rs) - sie hat Vorrang, sobald
// sie laeuft. SXNG_URL ist nur noch fuer eine zusaetzliche/externe Instanz
// (Entwicklung, Ausfall der eingebauten) und darf mehrere Adressen haben, mit Komma getrennt. Genommen wird
// die erste, die antwortet - so laeuft dasselbe Deployment lokal, im
// Docker-Netz und ueber ein VPN. Faellt die
// gewaehlte Adresse aus, wird neu gesucht.

use std::sync::RwLock;
use std::time::Duration;

pub struct Instanz {
    adressen: Vec<String>,
    aktiv: RwLock<String>,
    /// Die Adresse der eingebauten Instanz, sobald sie laeuft.
    eingebaut: RwLock<Option<String>>,
}

impl Instanz {
    pub fn neu(liste: &str) -> Self {
        let adressen: Vec<String> = liste
            .split(',')
            .map(|a| a.trim().trim_end_matches('/').to_string())
            .filter(|a| !a.is_empty())
            .collect();
        let erste = adressen.first().cloned().unwrap_or_default();
        Self { adressen, aktiv: RwLock::new(erste), eingebaut: RwLock::new(None) }
    }

    /// Die eingebaute Instanz ist (unter dieser Adresse) bereit - oder weg.
    pub fn eingebaut_setzen(&self, adresse: Option<String>) {
        *self.eingebaut.write().unwrap_or_else(|e| e.into_inner()) = adresse;
    }

    /// Die Adresse, die gerade benutzt wird: die eingebaute, sonst die gewaehlte externe.
    pub fn adresse(&self) -> String {
        if let Some(a) = self.eingebaut.read().unwrap_or_else(|e| e.into_inner()).clone() {
            return a;
        }
        self.aktiv.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Beim Start: warten (hoechstens `max`), bis eine Instanz bereit ist - die eingebaute
    /// braucht ein paar Sekunden. Danach die Adresse.
    pub async fn bereit(&self, max: Duration) -> String {
        let ende = tokio::time::Instant::now() + max;
        loop {
            let a = self.adresse();
            if !a.is_empty() || tokio::time::Instant::now() >= ende {
                return a;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Alle Adressen (eingebaute zuerst).
    pub fn adressen(&self) -> Vec<String> {
        let mut alle: Vec<String> = self.eingebaut.read().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
        alle.extend(self.adressen.iter().cloned());
        alle
    }

    /// Antwortet die Instanz unter dieser Adresse?
    async fn antwortet(client: &reqwest::Client, adresse: &str) -> bool {
        let anfrage = client.get(format!("{adresse}/config")).timeout(Duration::from_secs(4));
        matches!(anfrage.send().await, Ok(a) if a.status().is_success())
    }

    /// Die Adressen der Reihe nach pruefen und die erste nehmen, die
    /// antwortet. Gemeldet wird beim Start und bei jedem Wechsel.
    async fn waehlen(&self, client: &reqwest::Client, start: bool) {
        for adresse in &self.adressen {
            if Self::antwortet(client, adresse).await {
                let alt = std::mem::replace(&mut *self.aktiv.write().unwrap_or_else(|e| e.into_inner()), adresse.clone());
                if start || alt != *adresse {
                    println!("SearXNG: {adresse}");
                }
                return;
            }
        }
        eprintln!("SearXNG: keine Adresse antwortet ({})", self.adressen.join(", "));
    }

    /// Beim Start waehlen, danach alle 30 Sekunden pruefen, ob die gewaehlte
    /// Adresse noch antwortet - sonst neu waehlen.
    pub async fn ueberwachen(&self, client: reqwest::Client) {
        if self.adressen.is_empty() {
            return;
        }
        self.waehlen(&client, true).await;
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let extern_aktiv = self.aktiv.read().unwrap_or_else(|e| e.into_inner()).clone();
            if !Self::antwortet(&client, &extern_aktiv).await {
                self.waehlen(&client, false).await;
            }
        }
    }
}
