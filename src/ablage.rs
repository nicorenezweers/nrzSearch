// nrzSearch - die Ablage fuer Chats und Arbeitsbereiche, ohne Anmeldung.
//
// Jeder Browser bekommt beim ersten Besuch ein zufaelliges Token (256 Bit,
// im localStorage). Damit holt er seine Chats vom Server - und nur er: die
// Ablage liegt unter einem Hash des Tokens, und alles darin ist mit einem
// Schluessel aus dem Token verschluesselt (HKDF-SHA256, AES-256-GCM). Der
// Server speichert das Token nie und kann die Daten allein nicht lesen.
//
// Ablage:  ablagen/<sha256(token)>/<hash(teil)>.bin   die Teile, verschluesselt

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct Ablagen {
    ordner: PathBuf,
}

/// Die Ablage eines Browsers.
pub struct Ablage {
    ordner: PathBuf,
    schluessel: [u8; 32],
}

impl Ablagen {
    pub fn neu(ordner: &str) -> Self {
        Self { ordner: PathBuf::from(ordner) }
    }

    /// Fuer den Admin: wie viele Browser etwas gespeichert haben, wie viel.
    pub fn uebersicht(&self) -> (usize, u64) {
        let ordner: Vec<_> = std::fs::read_dir(&self.ordner).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).collect();
        let groesse = ordner.iter().flat_map(|o| std::fs::read_dir(o.path()).into_iter().flatten().flatten()).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum();
        (ordner.len(), groesse)
    }

    /// Die Ablage zu einem Token ("t_" und 64 Hex-Zeichen).
    pub fn oeffnen(&self, token: &str) -> Result<Ablage, String> {
        let token = token.trim();
        let gueltig = token.strip_prefix("t_").is_some_and(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()));
        if !gueltig {
            return Err("Ungültiges Token.".into());
        }
        let mut schluessel = [0u8; 32];
        Hkdf::<Sha256>::new(Some(b"nrzSearch-ablage"), token.as_bytes()).expand(b"schluessel", &mut schluessel).map_err(|_| "Schlüssel-Fehler.")?;
        Ok(Ablage { ordner: self.ordner.join(hex(&Sha256::digest(format!("nrzSearch-ablage:{token}")))), schluessel })
    }
}

impl Ablage {
    /// Die Datei eines Teils - der Name ist gehasht, auch er verraet nichts.
    fn datei(&self, teil: &str) -> PathBuf {
        self.ordner.join(format!("{}.bin", &hex(&Sha256::digest(teil.as_bytes()))[..32]))
    }

    pub fn lesen(&self, teil: &str) -> Result<Option<Vec<u8>>, String> {
        let Ok(roh) = std::fs::read(self.datei(teil)) else { return Ok(None) };
        if roh.len() < 12 {
            return Err("Die gespeicherten Daten sind beschädigt.".into());
        }
        let (nonce, daten) = roh.split_at(12);
        Aes256Gcm::new(&self.schluessel.into())
            .decrypt(Nonce::from_slice(nonce), daten)
            .map(Some)
            .map_err(|_| "Die Daten lassen sich nicht entschlüsseln.".into())
    }

    pub fn schreiben(&self, teil: &str, daten: &[u8]) -> Result<(), String> {
        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut nonce).map_err(|_| "Kein Zufall verfügbar.".to_string())?;
        let mut roh = nonce.to_vec();
        roh.extend(Aes256Gcm::new(&self.schluessel.into()).encrypt(Nonce::from_slice(&nonce), daten).map_err(|_| "Verschlüsseln ging nicht.".to_string())?);
        std::fs::create_dir_all(&self.ordner).map_err(|e| format!("Speichern ging nicht: {e}"))?;
        atomar(&self.datei(teil), &roh)
    }

    pub fn entfernen(&self, teil: &str) {
        let _ = std::fs::remove_file(self.datei(teil));
    }
}

/// Erst daneben schreiben, dann umbenennen - nie eine halbe Datei.
fn atomar(ziel: &Path, daten: &[u8]) -> Result<(), String> {
    let neu = ziel.with_extension("neu");
    std::fs::write(&neu, daten).and_then(|_| std::fs::rename(&neu, ziel)).map_err(|e| format!("Speichern ging nicht: {e}"))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
