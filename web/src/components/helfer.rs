//! Kleine Helfer fuer den Kern: Symbole (Lupe, Bueroklammer), Klick daneben, Seite nach oben,
//! Elemente per id.

use crate::werkzeug::im_scope;
use dioxus::prelude::*;
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure};

pub fn dokument() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

/// Ein Element per id als `T` (HtmlElement, HtmlInputElement …).
pub fn element<T: JsCast>(id: &str) -> Option<T> {
    dokument()?.get_element_by_id(id)?.dyn_into::<T>().ok()
}

/// Den Fokus in ein Feld setzen.
pub fn fokus(id: &str) {
    if let Some(el) = element::<web_sys::HtmlElement>(id) {
        let _ = el.focus();
    }
}

/// Die Seite nach oben rollen (sanft oder sofort).
pub fn nach_oben(sanft: bool) {
    if let Some(w) = web_sys::window() {
        let o = web_sys::ScrollToOptions::new();
        o.set_top(0.0);
        if sanft {
            o.set_behavior(web_sys::ScrollBehavior::Smooth);
        }
        w.scroll_to_with_scroll_to_options(&o);
    }
}

/// Liegt das Ziel eines Ereignisses in einem Element, das zu `selektor` passt?
pub fn ziel_in(e: &web_sys::Event, selektor: &str) -> Option<web_sys::Element> {
    e.target()?.dyn_into::<web_sys::Element>().ok()?.closest(selektor).ok()?
}

/// Ein Klick neben das Element mit `id` ruft `zu()` (Aufklapper, Vorschlaege).
pub fn use_klick_daneben(id: &'static str, zu: impl FnMut() + 'static) {
    let zelle = use_hook(|| Rc::new(std::cell::RefCell::new(None::<Box<dyn FnMut()>>)));
    *zelle.borrow_mut() = Some(Box::new(zu));
    let hoerer = use_hook(|| {
        let z = zelle.clone();
        let aufruf = im_scope(move |()| {
            if let Some(f) = z.borrow_mut().as_mut() {
                f();
            }
        });
        let c = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let drin = match (e.target().and_then(|t| t.dyn_into::<web_sys::Node>().ok()), dokument().and_then(|d| d.get_element_by_id(id))) {
                (Some(t), Some(el)) => el.contains(Some(&t)),
                _ => false,
            };
            if !drin {
                aufruf(());
            }
        });
        if let Some(d) = dokument() {
            let _ = d.add_event_listener_with_callback("click", c.as_ref().unchecked_ref());
        }
        Rc::new(c)
    });
    use_drop(move || {
        if let Some(d) = dokument() {
            let _ = d.remove_event_listener_with_callback("click", (*hoerer).as_ref().unchecked_ref());
        }
    });
}

/// Die Lupe (Suchen, Vorschlaege, aehnliche Suchanfragen).
#[component]
pub fn Lupe() -> Element {
    rsx! {
        svg { class: "lupe", "viewBox": "0 0 24 24", "aria-hidden": "true",
            path {
                fill: "currentColor",
                d: "M15.5 14h-.79l-.28-.27A6.47 6.47 0 0 0 16 9.5 6.5 6.5 0 1 0 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z",
            }
        }
    }
}

/// Die Bueroklammer (Dateien anhaengen).
#[component]
pub fn Klammer() -> Element {
    rsx! {
        svg { "viewBox": "0 0 24 24", "aria-hidden": "true",
            path {
                fill: "currentColor",
                d: "M16.5 6v11.5a4 4 0 0 1-8 0V5a2.5 2.5 0 0 1 5 0v10.5a1 1 0 0 1-2 0V6H10v9.5a2.5 2.5 0 0 0 5 0V5a4 4 0 0 0-8 0v12.5a5.5 5.5 0 0 0 11 0V6h-1.5z",
            }
        }
    }
}
