//! nrzSearch - die Weboberflaeche in Rust.
//!
//! Dioxus 0.7 (WASM, laeuft komplett im Browser), Komponenten aus nrzUI, Optik wie Google in
//! dunkel (assets/style.css). Der Server (axum) liefert die fertigen Dateien aus und die API.

mod api;
mod bib;
mod components;
mod i18n;
mod module;
mod pages;
mod state;
mod werkzeug;

use dioxus::prelude::*;
use state::{ZUSTAND, url_lesen};
use wasm_bindgen::{JsCast, closure::Closure};

const TAILWIND: Asset = asset!("/assets/tailwind.css");
const STYLE: Asset = asset!("/assets/style.css");
const RABE: Asset = asset!("/assets/raven.svg");
const LOGO: Asset = asset!("/assets/logo.svg");

fn main() {
    dioxus::launch(App);
}

/// Ist die Adresse /admin?
pub fn auf_admin() -> bool {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .is_some_and(|p| p.trim_end_matches('/') == "/admin")
}

/// Die Adresse zum Zustand schreiben - ein Verlaufseintrag je Seite (Startseite: "/").
fn adresse_setzen(z: &state::Zustand) {
    if !(z.q.is_empty() && z.kategorie == "general" && z.seite <= 1) {
        state::url_setzen(z);
        return;
    }
    if let Some(w) = web_sys::window() {
        let jetzt = format!("{}{}", w.location().pathname().unwrap_or_default(), w.location().search().unwrap_or_default());
        if jetzt != "/" && jetzt != "/?" {
            let _ = w.history().and_then(|h| h.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/")));
        }
    }
}

#[component]
fn App() -> Element {
    use pages::admin::AUF_ADMIN;
    // Beim Start und bei Zurueck/Vor: den Zustand aus der Adresse lesen.
    use_hook(|| {
        // Der Wurzelknoten von Dioxus soll das Flex-Layout von body nicht stoeren.
        if let Some(wurzel) = components::helfer::element::<web_sys::HtmlElement>("main") {
            let _ = wurzel.style().set_property("display", "contents");
        }
        let mut z = url_lesen();
        z.lauf = 1;
        *ZUSTAND.write() = z;
        let zurueck = werkzeug::im_scope(move |()| {
            let mut z = url_lesen();
            z.lauf = ZUSTAND.peek().lauf + 1;
            *pages::suche::NEU_AUFBAU.write() += 1;
            *AUF_ADMIN.write() = auf_admin();
            *ZUSTAND.write() = z;
        });
        let c = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| zurueck(()));
        // "/" springt ins Suchfeld - wie bei Google.
        let taste = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            if e.key() != "/" || components::helfer::ziel_in(&e, "input, textarea, select").is_some() {
                return;
            }
            let feld = components::helfer::element::<web_sys::HtmlElement>("q-start").or_else(|| components::helfer::element::<web_sys::HtmlElement>("q"));
            if let Some(f) = feld {
                e.prevent_default();
                let _ = f.focus();
            }
        });
        if let Some(w) = web_sys::window() {
            let _ = w.add_event_listener_with_callback("popstate", c.as_ref().unchecked_ref());
            if let Some(d) = w.document() {
                let _ = d.add_event_listener_with_callback("keydown", taste.as_ref().unchecked_ref());
            }
        }
        c.forget();
        taste.forget();
    });
    // Die Konfiguration der Module frueh holen (die Ergebnisseite braucht sie sofort).
    use_future(|| async {
        if let Some(k) = api::get_oder_nichts("/api/module", &[]).await {
            *state::MODUL_KONFIG.write() = Some(k);
        }
    });
    // Beim allerersten Start: die Einrichtung (/admin). Danach sieht nur der angemeldete Admin
    // oben rechts sein Profilbild - sonst nichts.
    use_future(|| async {
        let s = pages::admin::status().await;
        if s.get("eingerichtet").and_then(serde_json::Value::as_bool) == Some(false) && !auf_admin() {
            if let Some(h) = web_sys::window().and_then(|w| w.history().ok()) {
                let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/admin"));
            }
            *AUF_ADMIN.write() = true;
        } else {
            pages::admin::bild_aktualisieren().await;
        }
    });
    // Adresse, Titel, Klasse "startseite" am body und die Sprache der Seite.
    use_effect(|| {
        let z = ZUSTAND.read().clone();
        let admin = *AUF_ADMIN.read();
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
        let start = !admin && z.q.is_empty() && z.kategorie != "ki";
        if let Some(b) = doc.body() {
            let _ = b.class_list().toggle_with_force("startseite", start);
        }
        if admin {
            doc.set_title("Admin - nrzSearch");
            return;
        }
        adresse_setzen(&z);
        doc.set_title(&if z.q.is_empty() { "nrzSearch".to_string() } else { format!("{} - nrzSearch", z.q) });
    });
    use_effect(|| {
        let sprache = state::EINSTELLUNGEN.read().sprache.clone();
        if let Some(html) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
            let _ = html.set_attribute("lang", if sprache == "all" { "de" } else { &sprache });
        }
    });

    let z = ZUSTAND.read();
    // Was die KI fuer die vorige Seite noch schreibt, wird nicht mehr gebraucht - vor dem
    // Zeichnen der neuen Seite, damit ein neues Gespraech nicht gleich mit abgebrochen wird.
    let letzter = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0u64)));
    if letzter.replace(z.lauf) != z.lauf && letzter.get() > 1 {
        bib::ki::abbrechen();
    }
    let admin = *AUF_ADMIN.read();
    let start = z.q.is_empty() && z.kategorie != "ki";
    let seite = if admin {
        rsx! { pages::admin::AdminSeite {} }
    } else if start {
        rsx! { pages::start::Startseite {} }
    } else if z.kategorie == "ki" {
        rsx! { pages::ki::KiSeite {} }
    } else {
        rsx! { pages::suche::Ergebnisseite {} }
    };

    rsx! {
        document::Link { rel: "icon", href: LOGO, r#type: "image/svg+xml" }
        document::Link { rel: "stylesheet", href: TAILWIND }
        document::Link { rel: "stylesheet", href: STYLE }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        document::Meta { name: "color-scheme", content: "dark" }
        document::Meta { name: "theme-color", content: "#1f1f1f" }
        document::Meta { name: "description", content: "nrzSearch - private Suche über die eigene SearXNG-Instanz" }
        if admin {
            {seite}
        } else {
            components::kopf::Topbar { start }
            {seite}
            components::fuss::Fuss {}
        }
    }
}
