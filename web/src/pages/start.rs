//! Die Startseite: Logo, Suchfeld und "KI-Modus" - Portierung von startseite() in public/app.js.

use crate::components::suchfeld::Suchfeld;
use dioxus::prelude::*;

#[component]
pub fn Startseite() -> Element {
    rsx! {
        main { id: "inhalt",
            div { class: "start",
                h1 { class: "start-logo",
                    img { class: "raben-bild", src: crate::RABE, alt: "" }
                    span { class: "wortmarke", "nrzSearch" }
                }
                Suchfeld { start: true }
            }
        }
    }
}
