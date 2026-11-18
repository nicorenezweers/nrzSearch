//! Modul "ip": "meine IP", "IP-Adresse" - die oeffentliche IPv4 und IPv6
//! mit Anbieter und Standort (vom Server: /api/ip).

use super::ModulCtx;
use crate::werkzeug::{Kopf, KopierKnopf, ModulCss};
use dioxus::prelude::*;
use serde_json::Value;

const CSS: &str = ".ip-gross { word-break: break-all; }";

const TITEL: &str = "Deine öffentliche IP-Adresse";

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

/// Zeichnet das Modul (steht in `<section class="modul modul-ip">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = (&ctx, &daten);
    let antwort = use_resource(|| crate::api::get_oder_nichts("/api/ip", &[]));

    let inhalt = match &*antwort.read() {
        None => rsx! { div { class: "w-text", "wird ermittelt …" } },
        Some(ip) => {
            // Ein Feld als Text (leer = fehlt).
            let feld = |k: &str| ip.as_ref().and_then(|v| v.get(k)).and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from);
            match feld("ip") {
                None => rsx! { div { class: "w-text", "Nicht ermittelbar." } },
                Some(haupt) => {
                    let (v4, v6, anbieter, ort) = (feld("v4"), feld("v6"), feld("anbieter"), feld("ort"));
                    let gross = v4.clone().unwrap_or(haupt);
                    rsx! {
                        div { class: "w-riesig ip-gross", "{gross}" }
                        table { class: "w-tabelle",
                            tbody {
                                for (name, wert) in [("IPv4", v4), ("IPv6", v6)] {
                                    if let Some(w) = wert {
                                        tr { key: "{name}",
                                            td { "{name}" }
                                            td { b { "{w}" } }
                                            td { KopierKnopf { text: w.clone(), class: "w-knopf klein".to_string() } }
                                        }
                                    }
                                }
                                if let Some(a) = anbieter {
                                    tr { td { "Anbieter" } td { colspan: "2", "{a}" } }
                                }
                                if let Some(o) = ort {
                                    tr { td { "Standort" } td { colspan: "2", "{o}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        ModulCss { name: "ip".to_string(), css: CSS }
        Kopf { titel: TITEL.to_string() }
        {inhalt}
    }
}
