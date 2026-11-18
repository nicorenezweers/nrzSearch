//! Modul "uebersetzer" - Portierung von public/module/uebersetzer.js: "Guten Morgen auf
//! Englisch", "good morning auf deutsch", "uebersetze Danke ins Japanische", "translate hello to
//! spanish", "Uebersetzer" - eine Uebersetzungskarte wie bei Google: links der Text (die Sprache
//! erkennt Dieter), rechts die Uebersetzung, ⇄ tauscht. Beim Tippen wird neu uebersetzt.
//! Uebersetzt wird ueber POST /api/uebersetzen (Dieter), vorgelesen mit der Sprachausgabe des
//! Browsers.
//!
//! module.json: verzoegerung (ms nach dem Tippen), standardZiel ("en").

use super::ModulCtx;
use crate::api;
use crate::i18n::aktuelle_sprache;
use crate::werkzeug::{KopierKnopf, ModulCss, re_treffer};
use dioxus::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

const CSS: &str = r#"
.modul-uebersetzer .ue-kopf { display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; gap: 10px; margin-bottom: 12px; }
.modul-uebersetzer select { height: 38px; padding: 0 12px; border: 1px solid var(--kante); border-radius: 19px; background: var(--bg); color: var(--hoch); font: 500 14px Arial; cursor: pointer; }
.modul-uebersetzer .ue-tauschen { width: 38px; height: 38px; border: 0; border-radius: 50%; background: none; color: var(--schrift); font-size: 18px; cursor: pointer; }
.modul-uebersetzer .ue-tauschen:hover { background: var(--flaeche2); }
.modul-uebersetzer .ue-felder { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.modul-uebersetzer .ue-feld { display: flex; flex-direction: column; min-height: 140px; border: 1px solid var(--kante); border-radius: 14px; overflow: hidden; }
.modul-uebersetzer .ue-ergebnis { background: var(--flaeche2); border-color: transparent; }
.modul-uebersetzer textarea { flex: 1; padding: 14px 16px; border: 0; outline: 0; resize: none; background: none; color: var(--schrift); font: 20px/1.45 Arial; }
.modul-uebersetzer .ue-aus { flex: 1; padding: 14px 16px; font: 20px/1.45 Arial; color: var(--schrift); white-space: pre-wrap; overflow-wrap: anywhere; }
.modul-uebersetzer .ue-aus.laedt { opacity: .5; }
.modul-uebersetzer .ue-leiste { display: flex; align-items: center; gap: 6px; min-height: 40px; padding: 4px 8px 4px 16px; }
.modul-uebersetzer .ue-erkannt, .modul-uebersetzer .ue-status { flex: 1; font-size: 12px; color: var(--gedimmt); }
.modul-uebersetzer .ue-leiste button { height: 30px; padding: 0 10px; border: 0; border-radius: 15px; background: none; color: var(--gedimmt); font: 13px Arial; cursor: pointer; }
.modul-uebersetzer .ue-leiste button:hover { background: var(--flaeche3); color: var(--schrift); }
@media (max-width: 700px) { .modul-uebersetzer .ue-felder { grid-template-columns: 1fr; } }
"#;

const CODES: [&str; 40] = [
    "de", "en", "fr", "es", "it", "pt", "nl", "pl", "ru", "uk", "tr", "ar", "fa", "he", "hi", "bn", "ur", "ta", "te", "mr", "zh", "ja", "ko", "th", "vi", "id", "ms", "sv", "da", "no", "fi", "cs", "sk", "hu", "ro", "bg", "el", "hr", "la", "sw",
];

/// Wie man die Sprachen nennt - deutsch und englisch ("ins Englische", "auf Englisch", "to
/// english" …). Der Stamm reicht ("englisch", "englische").
const WOERTER: &[(&str, &[&str])] = &[
    ("de", &["deutsch", "german"]),
    ("en", &["englisch", "english"]),
    ("fr", &["französisch", "franzoesisch", "french"]),
    ("es", &["spanisch", "spanish"]),
    ("it", &["italienisch", "italian"]),
    ("pt", &["portugiesisch", "portuguese"]),
    ("nl", &["niederländisch", "niederlaendisch", "holländisch", "hollaendisch", "dutch"]),
    ("pl", &["polnisch", "polish"]),
    ("ru", &["russisch", "russian"]),
    ("uk", &["ukrainisch", "ukrainian"]),
    ("tr", &["türkisch", "tuerkisch", "turkish"]),
    ("ar", &["arabisch", "arabic"]),
    ("fa", &["persisch", "farsi", "persian"]),
    ("he", &["hebräisch", "hebraeisch", "hebrew"]),
    ("hi", &["hindi"]),
    ("bn", &["bengalisch", "bengali"]),
    ("ur", &["urdu"]),
    ("ta", &["tamil"]),
    ("te", &["telugu"]),
    ("mr", &["marathi"]),
    ("zh", &["chinesisch", "mandarin", "chinese"]),
    ("ja", &["japanisch", "japanese"]),
    ("ko", &["koreanisch", "korean"]),
    ("th", &["thailändisch", "thai"]),
    ("vi", &["vietnamesisch", "vietnamese"]),
    ("id", &["indonesisch", "indonesian"]),
    ("ms", &["malaiisch", "malay"]),
    ("sv", &["schwedisch", "swedish"]),
    ("da", &["dänisch", "daenisch", "danish"]),
    ("no", &["norwegisch", "norwegian"]),
    ("fi", &["finnisch", "finnish"]),
    ("cs", &["tschechisch", "czech"]),
    ("sk", &["slowakisch", "slovak"]),
    ("hu", &["ungarisch", "hungarian"]),
    ("ro", &["rumänisch", "rumaenisch", "romanian"]),
    ("bg", &["bulgarisch", "bulgarian"]),
    ("el", &["griechisch", "greek"]),
    ("hr", &["kroatisch", "croatian"]),
    ("la", &["latein", "lateinisch", "latin"]),
    ("sw", &["suaheli", "swahili"]),
];

const SPRACH: &str = r"([\p{L}]+)";

/// "Englische", "englischen", "English" -> "en".
fn sprache_von(wort: &str) -> Option<&'static str> {
    let w = wort.to_lowercase();
    let w = w.trim_end_matches(['.', '!', '?']);
    WOERTER.iter().find(|(_, liste)| liste.iter().any(|l| w == *l || [format!("{l}e"), format!("{l}en"), format!("{l}es")].iter().any(|x| x == w))).map(|(c, _)| *c)
}

/// Der Name einer Sprache in der gewaehlten Sprache der Oberflaeche (Intl.DisplayNames).
fn sprach_namen() -> Option<(js_sys::Object, js_sys::Function)> {
    let intl = js_sys::Reflect::get(&js_sys::global(), &"Intl".into()).ok()?;
    let bauer: js_sys::Function = js_sys::Reflect::get(&intl, &"DisplayNames".into()).ok()?.dyn_into().ok()?;
    let s = aktuelle_sprache();
    let s = if s == "all" { "de".to_string() } else { s };
    let optionen = js_sys::Object::new();
    js_sys::Reflect::set(&optionen, &"type".into(), &"language".into()).ok()?;
    let namen: js_sys::Object = js_sys::Reflect::construct(&bauer, &js_sys::Array::of2(&js_sys::Array::of1(&s.into()), &optionen)).ok()?.dyn_into().ok()?;
    let of: js_sys::Function = js_sys::Reflect::get(&namen, &"of".into()).ok()?.dyn_into().ok()?;
    Some((namen, of))
}

fn sprach_name(code: &str) -> String {
    sprach_namen().and_then(|(n, of)| of.call1(&n, &code.into()).ok()).and_then(|v| v.as_string()).unwrap_or_else(|| code.to_string())
}

/// Alle Sprachen (Code, Name), nach Namen sortiert.
fn sprachen() -> Vec<(&'static str, String)> {
    let namen = sprach_namen();
    let name = |c: &str| namen.as_ref().and_then(|(n, of)| of.call1(n, &c.into()).ok()).and_then(|v| v.as_string()).unwrap_or_else(|| c.to_string());
    let mut liste: Vec<(&'static str, String)> = CODES.iter().map(|c| (*c, name(c))).collect();
    liste.sort_by(|a, b| {
        let o = js_sys::JsString::from(a.1.as_str()).locale_compare(&b.1, &js_sys::Array::new(), &js_sys::Object::new());
        o.cmp(&0)
    });
    liste
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let q = ctx.frage.trim();
    if re_treffer(r"^(?:übersetzer|uebersetzer|übersetzen|translator|translate|übersetzung)$", "iu", q).is_some() {
        return Some(json!({ "text": "", "ziel": ctx.cfg_str("standardZiel").unwrap_or("en") }));
    }
    let muster = [
        // "uebersetze Danke ins Japanische", "translate hello to spanish"
        format!(r"^(?:übersetze|übersetz|uebersetze|translate)\s+(.+?)\s+(?:auf|in|ins|nach|to|into)\s+{SPRACH}$"),
        // "Guten Morgen auf Englisch", "Danke ins Spanische", "hello in german"
        format!(r"^(.+?)\s+(?:auf|ins|in|nach|to|into)\s+{SPRACH}(?:\s+(?:übersetzen|übersetzt|translate|translated))?$"),
    ];
    for m in &muster {
        let Some(g) = re_treffer(m, "iu", q) else { continue };
        let text = g.get(1).cloned().flatten().unwrap_or_default();
        let ziel = g.get(2).cloned().flatten().and_then(|w| sprache_von(&w));
        if let (Some(ziel), false) = (ziel, text.trim().is_empty()) {
            let text = crate::bib::diagramm::ersetzen(text.trim(), r#"^["„“']|["„“']$"#, "g", "");
            return Some(json!({ "text": text, "ziel": ziel }));
        }
    }
    None
}

/// Vorlesen mit der Sprachausgabe des Browsers.
fn vorlesen(text: &str, sprache: &str) {
    let Some(stimme) = web_sys::window().and_then(|w| w.speech_synthesis().ok()) else { return };
    let Ok(satz) = web_sys::SpeechSynthesisUtterance::new_with_text(text) else { return };
    satz.set_lang(sprache);
    stimme.cancel();
    stimme.speak(&satz);
}

/// Zeichnet das Modul (steht in `<section class="modul modul-uebersetzer">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let start = daten.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
    let verzoegerung = ctx.cfg_f64("verzoegerung").filter(|v| *v > 0.0).unwrap_or(600.0) as u32;
    let mut text = use_signal(|| start.clone());
    let mut ziel = use_signal(|| daten.get("ziel").and_then(Value::as_str).unwrap_or("en").to_string());
    let mut quelle = use_signal(String::new);
    let mut aus = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut erkannt = use_signal(String::new);
    let mut laedt = use_signal(|| false);
    let mut lauf = use_signal(|| 0u64);
    let mut tippen = use_signal(|| 0u64);
    let mut feld: Signal<Option<web_sys::HtmlTextAreaElement>> = use_signal(|| None);
    let liste = use_hook(sprachen);

    let passen = move || {
        if let Some(f) = feld.peek().as_ref() {
            let _ = f.style().set_property("height", "auto");
            let _ = f.style().set_property("height", &format!("{}px", f.scroll_height().max(90)));
        }
    };
    let uebersetzen = move || {
        spawn(async move {
            *lauf.write() += 1;
            let mein = *lauf.peek();
            let q = text.peek().trim().to_string();
            if q.is_empty() {
                aus.set(String::new());
                status.set(String::new());
                erkannt.set(String::new());
                return;
            }
            status.set("übersetzt …".into());
            laedt.set(true);
            let antwort = api::post("/api/uebersetzen", &json!({ "text": q, "ziel": *ziel.peek() }), None).await;
            if mein != *lauf.peek() {
                return;
            }
            match antwort {
                Ok(d) => {
                    aus.set(d.get("text").and_then(Value::as_str).unwrap_or_default().to_string());
                    erkannt.set(d.get("quelle").and_then(Value::as_str).unwrap_or_default().to_string());
                    status.set(String::new());
                }
                Err(f) => {
                    let t = if f.status == 0 || f.text.starts_with("Anfrage fehlgeschlagen") { "Die Übersetzung ist gerade nicht möglich.".to_string() } else { f.text };
                    status.set(t);
                    aus.set(String::new());
                }
            }
            laedt.set(false);
        });
    };
    let s2 = start.clone();
    use_effect(move || {
        if !s2.is_empty() {
            uebersetzen();
        }
    });

    let erkannt_text = if !erkannt.read().is_empty() && quelle.read().is_empty() { format!("Erkannt: {}", sprach_name(&erkannt.read())) } else { String::new() };
    let ziel_jetzt = ziel();
    let quelle_jetzt = quelle();
    rsx! {
        ModulCss { name: "uebersetzer".to_string(), css: CSS }
        div { class: "ue-kopf",
            select {
                class: "ue-quelle",
                "aria-label": "Ausgangssprache",
                onchange: move |e| {
                    quelle.set(e.value());
                    uebersetzen();
                },
                option { value: "", selected: quelle_jetzt.is_empty(), "Sprache erkennen" }
                for (c, n) in liste.iter() {
                    option { key: "{c}", value: "{c}", selected: quelle_jetzt == *c, "{n}" }
                }
            }
            button {
                r#type: "button",
                class: "ue-tauschen",
                "aria-label": "Sprachen tauschen",
                title: "Tauschen",
                // ⇄: die Uebersetzung wird der neue Text, die Sprachen tauschen.
                onclick: move |_| {
                    let von = if quelle.peek().is_empty() { erkannt.peek().clone() } else { quelle.peek().clone() };
                    if von.is_empty() || von == *ziel.peek() {
                        return;
                    }
                    let neu = aus.peek().clone();
                    let z = ziel.peek().clone();
                    quelle.set(z);
                    ziel.set(von);
                    if !neu.is_empty() {
                        text.set(neu.clone());
                        if let Some(f) = feld.peek().as_ref() {
                            f.set_value(&neu);
                        }
                    }
                    passen();
                    uebersetzen();
                },
                "⇄"
            }
            select {
                class: "ue-ziel",
                "aria-label": "Zielsprache",
                onchange: move |e| {
                    ziel.set(e.value());
                    uebersetzen();
                },
                for (c, n) in liste.iter() {
                    option { key: "{c}", value: "{c}", selected: ziel_jetzt == *c, "{n}" }
                }
            }
        }
        div { class: "ue-felder",
            div { class: "ue-feld",
                textarea {
                    class: "ue-text",
                    rows: "3",
                    placeholder: "Text eingeben",
                    "aria-label": "Text",
                    value: "{text}",
                    onmounted: move |e| {
                        let f: Option<web_sys::HtmlTextAreaElement> = e.data().downcast::<web_sys::Element>().and_then(|el| el.clone().dyn_into().ok());
                        if let Some(f) = &f {
                            if f.value().is_empty() {
                                let _ = f.focus();
                            }
                        }
                        feld.set(f);
                        passen();
                    },
                    oninput: move |e| {
                        text.set(e.value());
                        passen();
                        *tippen.write() += 1;
                        let jetzt = *tippen.peek();
                        spawn(async move {
                            gloo_timers::future::TimeoutFuture::new(verzoegerung).await;
                            if *tippen.peek() == jetzt {
                                uebersetzen();
                            }
                        });
                    },
                }
                div { class: "ue-leiste",
                    span { class: "ue-erkannt", "{erkannt_text}" }
                    button {
                        r#type: "button",
                        class: "ue-hoeren",
                        title: "Vorlesen",
                        onclick: move |_| {
                            let s = if !quelle.peek().is_empty() { quelle.peek().clone() } else if !erkannt.peek().is_empty() { erkannt.peek().clone() } else { "de".into() };
                            vorlesen(&text.peek(), &s);
                        },
                        "🔊"
                    }
                }
            }
            div { class: "ue-feld ue-ergebnis",
                div { class: if laedt() { "ue-aus laedt" } else { "ue-aus" }, "aria-live": "polite", "{aus}" }
                div { class: "ue-leiste",
                    span { class: "ue-status", "{status}" }
                    button {
                        r#type: "button",
                        class: "ue-hoeren",
                        title: "Vorlesen",
                        onclick: move |_| vorlesen(&aus.peek(), &ziel.peek()),
                        "🔊"
                    }
                    KopierKnopf { text: aus(), class: "ue-kopieren" }
                }
            }
        }
    }
}
