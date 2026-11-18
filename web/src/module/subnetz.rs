//! Modul "subnetz": "192.168.1.0/24", "Subnetz-Rechner" - Netz, Broadcast,
//! Maske, Wildcard, erster und letzter Host und die Zahl der Hosts.
//! Portierung von public/module/subnetz.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, zahl};
use dioxus::prelude::*;
use serde_json::Value;

const CSS: &str = ".sn-ein { width: 100%; max-width: 320px; font-family: ui-monospace, Menlo, Consolas, monospace; margin-bottom: 8px; }";

fn zu_ip(n: u64) -> String {
    [24, 16, 8, 0].iter().map(|b| ((n >> b) % 256).to_string()).collect::<Vec<_>>().join(".")
}

/// Eine Zahl aus 1 bis `max` Ziffern (0-9).
fn ziffern(s: &str, max: usize) -> Option<u64> {
    (!s.is_empty() && s.len() <= max && s.bytes().all(|c| c.is_ascii_digit())).then(|| s.parse().ok()).flatten()
}

/// "a.b.c.d/p" -> die Zeilen der Tabelle, oder None (falsches Format).
fn rechnen(text: &str) -> Option<Vec<(&'static str, String)>> {
    let (ip, praefix) = text.trim().split_once('/')?;
    let p = ziffern(praefix, 2)?;
    let oktette: Vec<u64> = ip.split('.').map(|o| ziffern(o, 3)).collect::<Option<_>>()?;
    if oktette.len() != 4 || oktette.iter().any(|&o| o > 255) || p > 32 {
        return None;
    }
    let groesse = 1u64 << (32 - p);
    let zahl_ip = oktette.iter().fold(0u64, |s, o| s * 256 + o);
    let netz = zahl_ip / groesse * groesse;
    let broadcast = netz + groesse - 1;
    let hosts = if p >= 31 { groesse } else { groesse - 2 };
    let erstes = oktette[0];
    Some(vec![
        ("Netzadresse", format!("{}/{p}", zu_ip(netz))),
        ("Broadcast", zu_ip(broadcast)),
        ("Subnetzmaske", zu_ip((1u64 << 32) - groesse)),
        ("Wildcard", zu_ip(groesse - 1)),
        ("Erster Host", zu_ip(if p >= 31 { netz } else { netz + 1 })),
        ("Letzter Host", zu_ip(if p >= 31 { broadcast } else { broadcast - 1 })),
        ("Nutzbare Hosts", zahl(hosts as f64, 0)),
        ("Klasse", (if erstes < 128 { "A" } else if erstes < 192 { "B" } else if erstes < 224 { "C" } else { "D/E" }).to_string()),
    ])
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(Value::from(ctx.gruppe(1).unwrap_or("192.168.1.0/24")))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-subnetz">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let mut eingabe = use_signal(|| daten.as_str().unwrap_or_default().to_string());
    let zeilen = rechnen(&eingabe());
    rsx! {
        ModulCss { name: "subnetz".to_string(), css: CSS }
        Kopf { titel: "Subnetz-Rechner (IPv4 / CIDR)".to_string() }
        input {
            class: "w-eingabe sn-ein",
            value: "{eingabe}",
            spellcheck: "false",
            placeholder: "10.0.0.0/8",
            oninput: move |e| eingabe.set(e.value()),
        }
        table { class: "w-tabelle sn-aus",
            if let Some(zeilen) = zeilen {
                for (k, v) in zeilen {
                    tr { key: "{k}",
                        td { "{k}" }
                        td { b { "{v}" } }
                    }
                }
            } else {
                tr { td { class: "fehler", "Bitte im Format 192.168.1.0/24 eingeben." } }
            }
        }
    }
}
