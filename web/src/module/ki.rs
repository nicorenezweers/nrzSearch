//! Modul "ki" - Portierung von public/module/ki.js: Uebersicht mit KI ueber den Treffern, wie
//! bei Google. Die Antwort kommt als Strom von /api/ki: der Server sucht selbst, liest die ersten
//! Seiten und belegt alles mit Quellen. "Im KI-Modus weiterfragen" fuehrt das Gespraech im Reiter
//! "KI-Modus" fort.
//!
//! module.json: mindestWoerter (ab wie vielen Woertern), hoehe (px bis "Mehr anzeigen"),
//! nicht (Muster, bei denen keine Uebersicht kommt - z. B. Wetter).

use super::ModulCtx;
use crate::api::Abbruch;
use crate::bib::ki::{self, KiAntwort, Quelle, QuellenZeile};
use crate::i18n::t;
use crate::werkzeug::{ModulCss, re_treffer};
use dioxus::prelude::*;
use serde_json::{Value, json};

/// Die Uebersicht ist "frei" (ohne Rahmen) - im Original eine Klasse an der Section; hier am
/// inneren Kasten `.ki-box`. Ohne Antwort bleibt die Section unsichtbar.
const CSS: &str = r#"
#widgets > section.modul.modul-ki { border: 0; border-radius: 0; padding: 0 0 22px; border-bottom: 1px solid var(--kante); }
section.modul-ki:not(:has(.ki-box)) { display: none; }
.modul-ki .ki-box.ki-offen .ki-text { max-height: none; }
.modul-ki .ki-box.ki-gekuerzt:not(.ki-offen) .ki-text::after { content: ""; position: absolute; inset: auto 0 0; height: 70px; background: linear-gradient(transparent, var(--bg)); }
"#;

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let q = ctx.frage.trim();
    let mindest = ctx.cfg_f64("mindestWoerter").unwrap_or(2.0);
    if (q.split_whitespace().count() as f64) < mindest {
        return None;
    }
    let nicht = ctx.cfg("nicht").as_array().into_iter().flatten().filter_map(Value::as_str).any(|m| re_treffer(m, "iu", q).is_some());
    (!nicht).then(|| Value::String(q.to_string()))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-ki">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let frage = daten.as_str().unwrap_or(&ctx.frage).to_string();
    let hoehe = ctx.cfg_f64("hoehe").filter(|h| *h > 0.0).unwrap_or(260.0);
    let mut bereit = use_signal(|| false);
    let mut aus = use_signal(|| false);
    let mut text = use_signal(String::new);
    let mut quellen = use_signal(Vec::<Quelle>::new);
    let mut fertig = use_signal(|| false);
    let mut gekuerzt = use_signal(|| false);
    let mut offen = use_signal(|| false);
    let mut feld: Signal<Option<web_sys::Element>> = use_signal(|| None);

    let (c, f) = (ctx.clone(), frage.clone());
    use_future(move || {
        let (ctx, frage) = (c.clone(), f.clone());
        async move {
            if !ki::bereit().await {
                aus.set(true);
                return;
            }
            bereit.set(true);
            let ab = Abbruch::neu();
            let anfrage = json!({ "frage": frage, "kurz": true, "sprache": ctx.einstellungen().sprache });
            let _ = ki::fragen(&anfrage, &ab, |name, d| {
                if !ctx.aktuell() {
                    ab.abbrechen();
                    return;
                }
                match name {
                    "quellen" => quellen.set(Quelle::liste(d.get("quellen").unwrap_or(&Value::Null))),
                    "text" => text.write().push_str(d.get("t").and_then(Value::as_str).unwrap_or_default()),
                    _ => {}
                }
            })
            .await;
            // Ohne Antwort keine Uebersicht - wie bei Google.
            if !ctx.aktuell() || text.peek().trim().is_empty() {
                aus.set(true);
                return;
            }
            fertig.set(true);
            gloo_timers::future::TimeoutFuture::new(0).await;
            if let Some(el) = feld.peek().as_ref() {
                gekuerzt.set(el.scroll_height() > el.client_height() + 4);
            }
        }
    });

    if aus() || !bereit() {
        return rsx! { ModulCss { name: "ki".to_string(), css: CSS } };
    }
    let mut klasse = String::from("ki-box");
    if gekuerzt() {
        klasse.push_str(" ki-gekuerzt");
    }
    if offen() {
        klasse.push_str(" ki-offen");
    }
    let leer = text.read().is_empty();
    let (uebersicht, mehr, weiter) = (t("uebersicht"), t("mehr_anzeigen"), t("weiterfragen"));
    rsx! {
        ModulCss { name: "ki".to_string(), css: CSS }
        div { class: "{klasse}", style: "--ki-hoehe:{hoehe}px",
            div { class: "ki-kopf", span { class: "ki-stern" } "{uebersicht}" }
            div {
                class: if leer { "ki-text ki-laedt" } else { "ki-text" },
                onmounted: move |e| feld.set(e.data().downcast::<web_sys::Element>().cloned()),
                if leer {
                    div { class: "skelett-zeile" }
                    div { class: "skelett-zeile" }
                    div { class: "skelett-zeile kurz" }
                } else {
                    KiAntwort { text: text(), quellen: quellen() }
                }
            }
            if fertig() {
                if !quellen.read().is_empty() {
                    QuellenZeile { quellen: quellen() }
                }
                div { class: "ki-fuss",
                    if gekuerzt() && !offen() {
                        button { r#type: "button", class: "ki-mehr", onclick: move |_| offen.set(true),
                            "{mehr} "
                            span { "▾" }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "ki-weiter",
                        onclick: move |_| {
                            let anfang = json!({ "frage": frage, "text": text(), "quellen": quellen() });
                            crate::pages::ki::weiterfragen(frage.clone(), Some(anfang));
                        },
                        span { class: "ki-stern" }
                        "{weiter}"
                    }
                }
            }
        }
    }
}
