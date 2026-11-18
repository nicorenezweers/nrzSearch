// nrzSearch - der Admin: genau ein Konto, angelegt beim allerersten Start
// (/admin). Er richtet OpenRouter ein, sieht Insights und schaltet Module.
// In der normalen Oberflaeche sieht man davon nichts.
//
// ADMIN_DATEI: E-Mail, Name, Bild, Passwort (Argon2id-Hash) und die Hashes
// der offenen Sitzungen (30 Tage). Das Token einer Sitzung kennt nur der
// Browser des Admins.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

const SITZUNG_TAGE: i64 = 30;

pub struct Admin {
    datei: String,
    daten: Mutex<Value>,
}

impl Admin {
    pub fn neu(datei: String) -> Self {
        let daten = std::fs::read(&datei).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or(Value::Null);
        Self { datei, daten: Mutex::new(daten) }
    }

    fn sperre(&self) -> std::sync::MutexGuard<'_, Value> {
        self.daten.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn speichern(&self, d: &Value) -> Result<(), String> {
        let neu = format!("{}.neu", self.datei);
        std::fs::write(&neu, d.to_string()).and_then(|_| std::fs::rename(&neu, &self.datei)).map_err(|e| format!("Speichern ging nicht ({}): {e}", self.datei))
    }

    pub fn eingerichtet(&self) -> bool {
        self.sperre()["passwort"].is_string()
    }

    /// Das Profil fuer den Browser (ohne Passwort und Sitzungen).
    pub fn profil(&self) -> Value {
        let d = self.sperre();
        json!({"email": d["email"], "name": d["name"], "bild": d["bild"]})
    }

    /// Der allererste Start: das Admin-Konto anlegen - nur, solange es keins gibt.
    pub fn einrichten(&self, email: &str, passwort: &str, name: &str) -> Result<String, String> {
        let email = email.trim().to_lowercase();
        if !email.contains('@') || email.len() > 200 {
            return Err("Bitte eine gültige E-Mail-Adresse eingeben.".into());
        }
        if passwort.chars().count() < 8 {
            return Err("Das Passwort braucht mindestens 8 Zeichen.".into());
        }
        let mut d = self.sperre();
        if d["passwort"].is_string() {
            return Err("nrzSearch ist schon eingerichtet.".into());
        }
        *d = json!({"email": email, "name": name.trim().chars().take(40).collect::<String>(), "passwort": hash(passwort)?,
            "erstellt": chrono::Utc::now().to_rfc3339(), "sitzungen": []});
        let token = sitzung_neu(&mut d)?;
        self.speichern(&d)?;
        Ok(token)
    }

    pub fn anmelden(&self, email: &str, passwort: &str) -> Result<String, String> {
        let mut d = self.sperre();
        let stimmt = d["email"].as_str() == Some(email.trim().to_lowercase().as_str())
            && d["passwort"].as_str().and_then(|h| PasswordHash::new(h).ok()).is_some_and(|h| Argon2::default().verify_password(passwort.as_bytes(), &h).is_ok());
        if !stimmt {
            return Err("E-Mail oder Passwort stimmen nicht.".into());
        }
        let token = sitzung_neu(&mut d)?;
        self.speichern(&d)?;
        Ok(token)
    }

    /// Ist das eine offene Sitzung des Admins?
    pub fn pruefen(&self, token: &str) -> bool {
        if token.len() < 20 {
            return false;
        }
        let h = token_hash(token);
        let jetzt = chrono::Utc::now().timestamp();
        self.sperre()["sitzungen"].as_array().is_some_and(|l| l.iter().any(|s| s["hash"] == h && s["bis"].as_i64().unwrap_or(0) > jetzt))
    }

    pub fn abmelden(&self, token: &str) {
        let mut d = self.sperre();
        let h = token_hash(token);
        if let Some(l) = d["sitzungen"].as_array_mut() {
            l.retain(|s| s["hash"] != h);
        }
        let _ = self.speichern(&d);
    }

    /// Name, Bild (data:-URL oder null) und/oder Passwort aendern.
    pub fn profil_aendern(&self, a: &Value) -> Result<(), String> {
        let mut d = self.sperre();
        if let Some(name) = a["name"].as_str() {
            d["name"] = json!(name.trim().chars().take(40).collect::<String>());
        }
        match &a["bild"] {
            Value::String(b) if b.starts_with("data:image/") && b.len() < 600_000 => d["bild"] = json!(b),
            Value::String(_) => return Err("Das Bild ist zu groß.".into()),
            Value::Null if a.get("bild").is_some() => d["bild"] = Value::Null,
            _ => {}
        }
        if let Some(neu) = a["passwort"].as_str() {
            if neu.chars().count() < 8 {
                return Err("Das Passwort braucht mindestens 8 Zeichen.".into());
            }
            d["passwort"] = json!(hash(neu)?);
        }
        self.speichern(&d)
    }
}

fn hash(passwort: &str) -> Result<String, String> {
    let mut salz = [0u8; 16];
    getrandom::getrandom(&mut salz).map_err(|_| "Kein Zufall verfügbar.".to_string())?;
    let salz = SaltString::encode_b64(&salz).map_err(|_| "Salz-Fehler.")?;
    Ok(Argon2::default().hash_password(passwort.as_bytes(), &salz).map_err(|_| "Das Passwort lässt sich nicht sichern.")?.to_string())
}

fn token_hash(token: &str) -> String {
    Sha256::digest(token.trim().as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Eine neue Sitzung (abgelaufene fliegen dabei raus).
fn sitzung_neu(d: &mut Value) -> Result<String, String> {
    let mut roh = [0u8; 32];
    getrandom::getrandom(&mut roh).map_err(|_| "Kein Zufall verfügbar.".to_string())?;
    let token = format!("a_{}", roh.iter().map(|b| format!("{b:02x}")).collect::<String>());
    let jetzt = chrono::Utc::now().timestamp();
    let mut liste: Vec<Value> = d["sitzungen"].as_array().cloned().unwrap_or_default();
    liste.retain(|s| s["bis"].as_i64().unwrap_or(0) > jetzt);
    liste.push(json!({"hash": token_hash(&token), "bis": jetzt + SITZUNG_TAGE * 86_400}));
    d["sitzungen"] = Value::Array(liste);
    Ok(token)
}
