// nrzSearch - Spam-Schutz: wie oft ein Nutzer (seine IP) etwas darf.
//
// Ein Eimer je IP und Art: er fuellt sich mit `pro_minute` Marken pro
// Minute bis `stoss` auf, jede Anfrage nimmt eine. Ist er leer, kommt 429.
// Nur im Arbeitsspeicher - gezaehlt wird nichts auf Dauer.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Default)]
pub struct Schutz {
    eimer: Mutex<HashMap<(IpAddr, &'static str), (f64, Instant)>>,
}

impl Schutz {
    pub fn erlaubt(&self, ip: IpAddr, art: &'static str, pro_minute: f64, stoss: f64) -> bool {
        let mut eimer = self.eimer.lock().unwrap_or_else(|e| e.into_inner());
        if eimer.len() > 20_000 {
            eimer.clear();
        }
        let jetzt = Instant::now();
        let (marken, zuletzt) = eimer.entry((ip, art)).or_insert((stoss, jetzt));
        *marken = (*marken + zuletzt.elapsed().as_secs_f64() * pro_minute / 60.0).min(stoss);
        *zuletzt = jetzt;
        if *marken < 1.0 {
            return false;
        }
        *marken -= 1.0;
        true
    }
}
