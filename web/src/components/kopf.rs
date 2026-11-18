//! Die Kopfleiste: Logo, Suchpille, oben rechts das Profilbild des Admins, darunter Reiter und
//! Tools, ganz unten der Ladebalken.

use super::reiter::Reiter;
use super::suchfeld::Suchfeld;
use crate::pages::admin::{ADMIN_PROFIL, AUF_ADMIN, ProfilBild};
use crate::pages::suche::LAEDT;
use crate::state::{EINGABE, ZUSTAND, Zustand};
use dioxus::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

#[component]
pub fn Topbar(start: bool) -> Element {
    // Die Hoehe der Kopfleiste - darunter bleibt die Bildvorschau stehen (--topbar).
    let beobachter = use_hook(|| {
        let c = Closure::<dyn FnMut(js_sys::Array)>::new(move |eintraege: js_sys::Array| {
            let Some(e) = eintraege.get(0).dyn_into::<web_sys::ResizeObserverEntry>().ok() else { return };
            let hoehe = e.target().dyn_into::<web_sys::HtmlElement>().map(|h| h.offset_height()).unwrap_or(0);
            if let Some(wurzel) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
                if let Some(h) = wurzel.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = h.style().set_property("--topbar", &format!("{hoehe}px"));
                }
            }
        });
        let b = web_sys::ResizeObserver::new(c.as_ref().unchecked_ref()).ok();
        c.forget();
        b
    });
    let b2 = beobachter.clone();
    use_drop(move || {
        if let Some(b) = &b2 {
            b.disconnect();
        }
    });

    rsx! {
        div {
            class: "topbar",
            id: "topbar",
            onmounted: move |_| {
                if let (Some(b), Some(el)) = (&beobachter, super::helfer::element::<web_sys::Element>("topbar")) {
                    b.observe(&el);
                }
            },
            header { class: "kopf",
                a {
                    class: "logo",
                    href: "/",
                    "aria-label": "nrzSearch - Startseite",
                    onclick: move |e: MouseEvent| {
                        e.prevent_default();
                        let lauf = ZUSTAND.peek().lauf + 1;
                        *ZUSTAND.write() = Zustand { lauf, ..Zustand::default() };
                        *EINGABE.write() = String::new();
                    },
                    img { class: "raben-bild", src: crate::RABE, alt: "", width: "30", height: "30" }
                    span { class: "wortmarke", "nrzSearch" }
                }
                if !start {
                    Suchfeld { start: false }
                }
                AdminBild {}
            }
            if !start {
                Reiter {}
            }
            if LAEDT() {
                div { class: "ladebalken", "aria-hidden": "true" }
            }
        }
    }
}

/// Oben rechts: nur der angemeldete Admin sieht sein Profilbild - sonst nichts.
#[component]
fn AdminBild() -> Element {
    let profil = ADMIN_PROFIL.read().clone();
    let Some(p) = profil else { return rsx! {} };
    rsx! {
        a {
            class: "admin-bild",
            id: "admin-bild",
            href: "/admin",
            title: "Admin",
            onclick: move |e: MouseEvent| {
                e.prevent_default();
                if let Some(h) = web_sys::window().and_then(|w| w.history().ok()) {
                    let _ = h.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/admin"));
                }
                *AUF_ADMIN.write() = true;
            },
            ProfilBild { profil: p, gross: false }
        }
    }
}
