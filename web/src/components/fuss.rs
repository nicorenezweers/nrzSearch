//! Der Footer: nrzSearch mit der Version der SearXNG-Instanz, die Sprachwahl und die anonyme
//! Statistik (Suchen, KI-Antworten, Tokens).

use crate::api;
use crate::i18n::{SPRACHEN, t};
use crate::pages::suche::sprache_setzen;
use crate::state::EINSTELLUNGEN;
use crate::werkzeug::zahl;
use dioxus::prelude::*;
use serde_json::Value;

/// Kurz: "1,2 M", "45 k", "9.876".
fn kurz(n: f64) -> String {
    if n >= 1e6 {
        format!("{} M", zahl(n / 1e6, 1))
    } else if n >= 1e4 {
        format!("{} k", zahl(n / 1e3, 0))
    } else {
        zahl(n, 0)
    }
}

#[component]
pub fn Fuss() -> Element {
    let version = use_resource(|| async { api::get_oder_nichts("/api/version", &[]).await });
    let statistik = use_resource(|| async { api::get_oder_nichts("/api/statistik", &[]).await });
    let sprache = EINSTELLUNGEN.read().sprache.clone();

    let instanz = version
        .read()
        .as_ref()
        .and_then(|v| v.as_ref())
        .and_then(|v| v.get("version").and_then(Value::as_str).map(|v| format!(" · SearXNG {v}")))
        .unwrap_or_default();
    let zahlen = statistik.read().as_ref().and_then(|s| s.as_ref()).map(|s| {
        let n = |k: &str| s.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        format!("{} {} · {} {} · {} Tokens", kurz(n("suchen")), t("suchen"), kurz(n("antworten")), t("antworten"), kurz(n("tokens")))
    });

    rsx! {
        footer { class: "fuss",
            div { "nrzSearch" span { id: "fuss-instanz", "{instanz}" } }
            label { class: "fuss-sprache",
                span { id: "fuss-sprache-name", {t("sprache")} }
                " "
                select {
                    id: "fuss-sprache",
                    "aria-label": "Sprache",
                    onchange: move |e: FormEvent| sprache_setzen(&e.value()),
                    for (code, name) in SPRACHEN.iter() {
                        option { key: "{code}", value: *code, selected: sprache == *code, "{name}" }
                    }
                }
            }
            span { class: "fuss-statistik", id: "fuss-statistik", {zahlen.unwrap_or_default()} }
        }
    }
}
