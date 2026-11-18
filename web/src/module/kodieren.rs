//! Modul "kodieren": "base64 encode Hallo", "url decode %C3%A4", "Base64" -
//! Base64 und URL-Kodierung in beide Richtungen, live beim Tippen.
//! Portierung von public/module/kodieren.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, KopierKnopf};
use dioxus::prelude::*;
use serde_json::{Value, json};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Wie btoa(UTF-8-Bytes): Standard-Base64 mit Auffuellung.
fn base64_hin(t: &str) -> String {
    let b = t.as_bytes();
    let mut aus = String::with_capacity(b.len().div_ceil(3) * 4);
    for stueck in b.chunks(3) {
        let n = (u32::from(stueck[0]) << 16) | (u32::from(*stueck.get(1).unwrap_or(&0)) << 8) | u32::from(*stueck.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= stueck.len() {
                aus.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                aus.push('=');
            }
        }
    }
    aus
}

/// Wie atob (forgiving-base64) + TextDecoder: Leerraum wird ueberlesen, ungueltig -> None.
fn base64_her(t: &str) -> Option<String> {
    let mut zeichen: Vec<u8> = t.trim().bytes().filter(|c| !matches!(c, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')).collect();
    if zeichen.len() % 4 == 0 {
        for _ in 0..2 {
            if zeichen.last() == Some(&b'=') {
                zeichen.pop();
            }
        }
    }
    if zeichen.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::with_capacity(zeichen.len() * 3 / 4);
    let mut puffer = 0u32;
    let mut bits = 0;
    for c in zeichen {
        let wert = ALPHABET.iter().position(|&a| a == c)? as u32;
        puffer = (puffer << 6) | wert;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((puffer >> bits) as u8);
            puffer &= (1 << bits) - 1;
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Kodieren (hin) oder dekodieren (her); None = ungueltige Eingabe.
fn rechnen(art: &str, richtung: &str, text: &str) -> Option<String> {
    match (art, richtung) {
        ("base64", "her") => base64_her(text),
        ("base64", _) => Some(base64_hin(text)),
        (_, "her") => js_sys::decode_uri_component(&text.replace('+', " ")).ok().map(String::from),
        _ => Some(String::from(js_sys::encode_uri_component(text))),
    }
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let art = ctx.gruppe(1).unwrap_or_default().to_lowercase();
    let wort = ctx.gruppe(2).unwrap_or_default().to_lowercase();
    let richtung = if matches!(wort.as_str(), "decode" | "dekodieren" | "decoder") { "her" } else { "hin" };
    let text = ctx.gruppen.get(3).cloned().flatten().unwrap_or_default();
    Some(json!({ "art": art, "richtung": richtung, "text": text }))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-kodieren">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let start_art = daten["art"].as_str().unwrap_or("base64").to_string();
    let titel = if start_art == "base64" { "Base64" } else { "URL-Kodierung" };
    let mut art = use_signal(|| start_art.clone());
    let mut richtung = use_signal(|| daten["richtung"].as_str().unwrap_or("hin").to_string());
    let mut text = use_signal(|| daten["text"].as_str().unwrap_or_default().to_string());

    let ergebnis = rechnen(&art(), &richtung(), &text());
    let (aus, klasse) = match ergebnis {
        Some(a) => (a, "w-eingabe w-flaeche ko-aus"),
        None => ("Ungültige Eingabe".to_string(), "w-eingabe w-flaeche ko-aus fehler"),
    };

    rsx! {
        Kopf { titel: titel.to_string() }
        div { class: "w-zeile",
            select { class: "w-eingabe ko-art", oninput: move |e| art.set(e.value()),
                option { value: "base64", selected: art() == "base64", "Base64" }
                option { value: "url", selected: art() == "url", "URL" }
            }
            select { class: "w-eingabe ko-richtung", oninput: move |e| richtung.set(e.value()),
                option { value: "hin", selected: richtung() == "hin", "Kodieren" }
                option { value: "her", selected: richtung() == "her", "Dekodieren" }
            }
        }
        textarea {
            class: "w-eingabe w-flaeche ko-ein",
            rows: "3",
            placeholder: "Text eingeben …",
            value: "{text}",
            oninput: move |e| text.set(e.value()),
        }
        div { class: "w-zeile",
            span { class: "w-text", "Ergebnis" }
            KopierKnopf { text: aus.clone(), class: "w-knopf klein ko-kopie".to_string() }
        }
        textarea { class: "{klasse}", rows: "3", readonly: true, value: "{aus}" }
    }
}
