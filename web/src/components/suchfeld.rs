//! Die Suchpille (oben und auf der Startseite): Vorschlaege (entprellt, Pfeiltasten, alte
//! Antworten verworfen), × leeren, Bueroklammer und Ablegen von Dateien, Chips der Anhaenge.
//! Mit Anhaengen oeffnet Enter einen Arbeitsbereich im KI-Modus.

use super::helfer::{Klammer, Lupe, element, fokus, use_klick_daneben};
use crate::bib::dateien::{datei_art, groesse_text, hinzufuegen};
use crate::i18n::t;
use crate::state::{self, ANHAENGE, EINGABE, EINSTELLUNGEN, ZUSTAND};
use crate::{api, pages};
use dioxus::prelude::*;
use serde_json::Value;
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

/// Eine Suche aus einer Suchleiste: leer fuehrt zur Startseite, sonst neue Suche (der KI-Modus
/// wird dabei zu "Alle").
pub fn starte(q: &str) {
    state::suchen(q);
}

/// Die Dateien eines Datei-Feldes (Bueroklammer) oder eines Ablegens.
fn dateien_aus(liste: Option<web_sys::FileList>) -> Vec<web_sys::File> {
    let Some(l) = liste else { return Vec::new() };
    (0..l.length()).filter_map(|i| l.get(i)).collect()
}

#[component]
pub fn Suchfeld(start: bool) -> Element {
    let (feld_id, liste_id, datei_id, zone_id) =
        if start { ("q-start", "vorschlaege-start", "datei-start", "feld-start") } else { ("q", "vorschlaege", "datei-kopf", "feld-kopf") };
    let mut text = use_signal(|| ZUSTAND.peek().q.clone());
    let mut vorschlaege = use_signal(Vec::<String>::new);
    let mut index = use_signal(|| -1i64);
    let mut getippt = use_signal(String::new);
    let mut ziehen = use_signal(|| false);
    // Zaehlt jede Eingabe - eine Antwort, die zu spaet kommt, wird verworfen.
    let zaehler = use_hook(|| Rc::new(Cell::new(0u64)));

    // Jede neue Suche (auch Zurueck/Vor): im Feld steht die Frage.
    use_effect(move || {
        let q = ZUSTAND.read().q.clone();
        text.set(q);
    });

    let mut zu = {
        let zaehler = zaehler.clone();
        move || {
            zaehler.set(zaehler.get() + 1);
            if !vorschlaege.peek().is_empty() {
                vorschlaege.set(Vec::new());
            }
            if *index.peek() != -1 {
                index.set(-1);
            }
        }
    };
    use_klick_daneben(zone_id, {
        let mut zu = zu.clone();
        move || zu()
    });

    let holen = {
        let zaehler = zaehler.clone();
        move |q: String| {
            let n = zaehler.get() + 1;
            zaehler.set(n);
            let z = zaehler.clone();
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(120).await;
                if z.get() != n {
                    return;
                }
                let frage = q.trim().to_string();
                let liste: Vec<String> = if frage.chars().count() < 2 {
                    Vec::new()
                } else {
                    let d = api::get_oder_nichts("/api/vorschlaege", &[("q", &frage)]).await;
                    if z.get() != n {
                        return;
                    }
                    d.as_ref()
                        .and_then(|d| d.get("vorschlaege"))
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default()
                };
                getippt.set(q.to_lowercase());
                index.set(-1);
                vorschlaege.set(liste);
            });
        }
    };

    let mut abschicken = {
        let mut zu = zu.clone();
        move |q: String| {
            zu();
            if ANHAENGE.peek().is_empty() {
                starte(&q);
            } else {
                pages::ki::arbeitsbereich_starten(q);
            }
        }
    };

    let mut bewegen = move |schritt: i64| {
        let liste = vorschlaege.peek().clone();
        let n = liste.len() as i64;
        if n == 0 {
            return;
        }
        let i = (*index.peek() + schritt + n).rem_euclid(n);
        index.set(i);
        text.set(liste[i as usize].clone());
    };

    let anhaenge = ANHAENGE.read();
    // Die Texte folgen der gewaehlten Sprache.
    let _ = EINSTELLUNGEN.read();
    let offen = !vorschlaege.read().is_empty();
    let feld_klasse = format!(
        "feld{}{}{}",
        if start { " feld-start" } else { "" },
        if offen { " offen" } else { "" },
        if ziehen() { " ziehen" } else { "" }
    );
    let platzhalter = if anhaenge.is_empty() { "" } else { "Was soll Dieter mit den Dateien tun? (Enter)" };
    let wert = text.read().clone();
    let leer = wert.is_empty();
    let getippt_jetzt = getippt.read().clone();
    let aktiv = *index.read();

    let feld = rsx! {
        div {
            id: zone_id,
            class: "{feld_klasse}",
            ondragover: move |e: DragEvent| {
                let hat_dateien = e
                    .data()
                    .downcast::<web_sys::DragEvent>()
                    .and_then(|d| d.data_transfer())
                    .is_some_and(|dt| dt.types().includes(&"Files".into(), 0));
                if hat_dateien {
                    e.prevent_default();
                    ziehen.set(true);
                }
            },
            ondragleave: move |e: DragEvent| {
                let drin = e
                    .data()
                    .downcast::<web_sys::DragEvent>()
                    .and_then(|d| d.related_target())
                    .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
                    .zip(element::<web_sys::Element>(zone_id))
                    .is_some_and(|(t, zone)| zone.contains(Some(&t)));
                if !drin {
                    ziehen.set(false);
                }
            },
            ondrop: move |e: DragEvent| {
                let liste = dateien_aus(e.data().downcast::<web_sys::DragEvent>().and_then(|d| d.data_transfer()).and_then(|dt| dt.files()));
                if liste.is_empty() {
                    return;
                }
                e.prevent_default();
                ziehen.set(false);
                spawn(hinzufuegen(liste));
            },
            if start {
                span { class: "lupe-links", Lupe {} }
            }
            input {
                r#type: "search",
                id: feld_id,
                name: if !start { "q" },
                "aria-label": "Suche",
                autocomplete: "off",
                autocapitalize: "off",
                spellcheck: "false",
                placeholder: platzhalter,
                value: "{wert}",
                onmounted: move |e| async move {
                    if start {
                        let _ = e.set_focus(true).await;
                    }
                },
                oninput: {
                    let holen = holen.clone();
                    move |e: FormEvent| {
                        let v = e.value();
                        text.set(v.clone());
                        *EINGABE.write() = v.clone();
                        holen(v);
                    }
                },
                onkeydown: {
                    let mut zu = zu.clone();
                    move |e: KeyboardEvent| match e.key() {
                        Key::ArrowDown => {
                            e.prevent_default();
                            bewegen(1);
                        }
                        Key::ArrowUp => {
                            e.prevent_default();
                            bewegen(-1);
                        }
                        Key::Escape => zu(),
                        _ => {}
                    }
                },
            }
            button {
                r#type: "button",
                class: "leeren",
                id: if start { "leeren-start" } else { "leeren" },
                "aria-label": "Eingabe löschen",
                hidden: leer,
                onclick: move |_| {
                    text.set(String::new());
                    *EINGABE.write() = String::new();
                    zu();
                    fokus(feld_id);
                },
                "×"
            }
            button {
                r#type: "button",
                class: "anhang-knopf",
                "aria-label": "Dateien anhängen",
                title: "Dateien anhängen - oder einfach hierher ziehen",
                onclick: move |_| {
                    if let Some(el) = element::<web_sys::HtmlElement>(datei_id) {
                        el.click();
                    }
                },
                Klammer {}
            }
            input {
                r#type: "file",
                id: datei_id,
                multiple: true,
                hidden: true,
                onchange: move |_| {
                    let Some(el) = element::<web_sys::HtmlInputElement>(datei_id) else { return };
                    let liste = dateien_aus(el.files());
                    el.set_value("");
                    if !liste.is_empty() {
                        spawn(hinzufuegen(liste));
                    }
                },
            }
            if !start {
                span { class: "trenner", "aria-hidden": "true" }
                button { r#type: "submit", class: "senden", "aria-label": "Suche", Lupe {} }
            }
            ul {
                class: "vorschlaege",
                id: liste_id,
                role: "listbox",
                "aria-label": "Suchvorschläge",
                hidden: !offen,
                for (i, v) in vorschlaege.read().iter().enumerate() {
                    li {
                        key: "{v}",
                        role: "option",
                        "aria-selected": if i as i64 == aktiv { "true" } else { "false" },
                        onmousedown: {
                            let v = v.clone();
                            let mut abschicken = abschicken.clone();
                            move |e: MouseEvent| {
                                e.prevent_default();
                                abschicken(v.clone());
                            }
                        },
                        Lupe {}
                        VorschlagText { text: v.clone(), getippt: getippt_jetzt.clone() }
                    }
                }
            }
        }
        div { class: "anhaenge", hidden: anhaenge.is_empty(),
            for (i, d) in anhaenge.iter().enumerate() {
                span { key: "{i}-{d.name}", class: "datei-chip", title: "{d.name}",
                    if d.typ == "bild" && d.daten.is_some() {
                        img { src: d.daten.clone().unwrap_or_default(), alt: "" }
                    } else {
                        span { class: "datei-art", {datei_art(&d.name, &d.typ)} }
                    }
                    span { class: "datei-name", "{d.name}" }
                    small {
                        if let Some(s) = d.seiten {
                            "{s} S."
                        } else {
                            {groesse_text(d.groesse)}
                        }
                    }
                    button {
                        r#type: "button",
                        class: "datei-weg",
                        "aria-label": "{d.name} entfernen",
                        onclick: move |_| {
                            let mut a = ANHAENGE.write();
                            if i < a.len() {
                                a.remove(i);
                            }
                        },
                        "×"
                    }
                }
            }
        }
    };

    if start {
        rsx! {
            form {
                class: "start-form",
                id: "start-suche",
                role: "search",
                onsubmit: {
                    let mut abschicken = abschicken.clone();
                    move |e: FormEvent| {
                        e.prevent_default();
                        abschicken(text.peek().clone());
                    }
                },
                {feld}
                div { class: "start-knoepfe",
                    button {
                        r#type: "button",
                        class: "start-knopf start-ki",
                        id: "start-ki",
                        // "KI-Modus": die Frage geht direkt an Dieter (mit Dateien als Arbeitsbereich).
                        onclick: move |_| {
                            let q = text.peek().trim().to_string();
                            if !ANHAENGE.peek().is_empty() {
                                pages::ki::arbeitsbereich_starten(q);
                                return;
                            }
                            let mut z = ZUSTAND.write();
                            z.q = q;
                            z.seite = 1;
                            z.kategorie = "ki".into();
                            z.lauf += 1;
                        },
                        span { class: "ki-stern" }
                        {t("ki_modus")}
                    }
                }
            }
        }
    } else {
        rsx! {
            form {
                class: "suche",
                id: "suche",
                role: "search",
                autocomplete: "off",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    abschicken(text.peek().clone());
                },
                {feld}
            }
        }
    }
}

/// Wie bei Google: der getippte Teil normal, der Rest fett.
#[component]
fn VorschlagText(text: String, getippt: String) -> Element {
    let anfang = if text.to_lowercase().starts_with(&getippt) { getippt.chars().count() } else { 0 };
    let vorn: String = text.chars().take(anfang).collect();
    let rest: String = text.chars().skip(anfang).collect();
    rsx! {
        span { "{vorn}" b { "{rest}" } }
    }
}
