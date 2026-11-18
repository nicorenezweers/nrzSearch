//! /admin: beim allerersten Start die Einrichtung (Admin-Konto, OpenRouter), danach Anmeldung
//! und Dashboard: Insights, System, OpenRouter, Module, Profil - Portierung von
//! public/lib/admin.js. In der normalen Oberflaeche taucht davon nichts auf - nur der angemeldete
//! Admin hat oben rechts sein Profilbild.

use crate::api;
use crate::bib::dateien::groesse_text;
use crate::bib::diagramm::Diagramm;
use crate::components::ui::badge::{Badge, BadgeVariant};
use crate::components::ui::button::{Button, ButtonVariant};
use crate::components::ui::spinner::Spinner;
use crate::state::MODUL_KONFIG;
use crate::werkzeug::zahl;
use dioxus::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const MERKEN: &str = "nrz_admin";

/// Das Profil des angemeldeten Admins (fuer das Bild oben rechts) - sonst `None`.
pub static ADMIN_PROFIL: GlobalSignal<Option<Value>> = Signal::global(|| None);
/// Steht die Seite /admin?
pub static AUF_ADMIN: GlobalSignal<bool> = Signal::global(crate::auf_admin);

fn speicher() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn token() -> String {
    speicher().and_then(|s| s.get_item(MERKEN).ok().flatten()).unwrap_or_default()
}

fn token_setzen(t: &str) {
    if let Some(s) = speicher() {
        let _ = if t.is_empty() { s.remove_item(MERKEN) } else { s.set_item(MERKEN, t) };
    }
}

async fn admin_get(pfad: &str) -> api::Ergebnis<Value> {
    api::get_mit_token(&format!("/api/admin/{pfad}"), &token()).await
}

async fn admin_post(pfad: &str, daten: Value) -> api::Ergebnis<Value> {
    api::post(&format!("/api/admin/{pfad}"), &daten, Some(&token())).await
}

/// Der Stand: eingerichtet? angemeldet? (bei jedem Fehler `{}`).
pub async fn status() -> Value {
    admin_get("status").await.unwrap_or_else(|_| json!({}))
}

/// Das Profilbild oben rechts - nur fuer den angemeldeten Admin.
pub async fn bild_aktualisieren() {
    if token().is_empty() {
        *ADMIN_PROFIL.write() = None;
        return;
    }
    let s = status().await;
    let profil = (s.get("angemeldet").and_then(Value::as_bool) == Some(true)).then(|| s.get("profil").cloned().unwrap_or_else(|| json!({})));
    *ADMIN_PROFIL.write() = profil;
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

fn n(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Das Profilbild (oder der erste Buchstabe).
#[component]
pub fn ProfilBild(profil: Value, gross: bool) -> Element {
    let klasse = if gross { "ad-bild gross" } else { "ad-bild" };
    let bild = s(&profil, "bild").to_string();
    if !bild.is_empty() {
        return rsx! { img { class: "{klasse}", src: "{bild}", alt: "" } };
    }
    let name = if s(&profil, "name").trim().is_empty() { s(&profil, "email") } else { s(&profil, "name") };
    let buchstabe = name.trim().chars().next().unwrap_or('?').to_uppercase().to_string();
    rsx! { span { class: "{klasse}", "{buchstabe}" } }
}

#[derive(Clone, Debug, PartialEq)]
enum Ansicht {
    Laedt,
    Einrichtung,
    Anmeldung,
    /// Mit dem Fehler von OpenRouter aus der Einrichtung.
    Dashboard(Option<String>),
}

#[component]
pub fn AdminSeite() -> Element {
    let mut ansicht = use_signal(|| Ansicht::Laedt);
    use_future(move || async move {
        let st = status().await;
        let ja = |k: &str| st.get(k).and_then(Value::as_bool) == Some(true);
        ansicht.set(if !ja("eingerichtet") {
            Ansicht::Einrichtung
        } else if !ja("angemeldet") {
            Ansicht::Anmeldung
        } else {
            Ansicht::Dashboard(None)
        });
    });
    let inhalt = match ansicht() {
        Ansicht::Laedt => rsx! {
            div { class: "ad-seite ad-schmal flex justify-center py-16", Spinner { class: "size-6" } }
        },
        Ansicht::Einrichtung => rsx! { Einrichtung { ansicht } },
        Ansicht::Anmeldung => rsx! { Anmeldung { ansicht } },
        Ansicht::Dashboard(f) => rsx! { Dashboard { ansicht, ki_fehler: f } },
    };
    rsx! {
        main { id: "inhalt", class: "admin",
            div { class: "nrzui", {inhalt} }
        }
    }
}

/// Eine Meldung unter einem Formular (gut = gruen, sonst rot).
#[component]
fn Meldung(meldung: Signal<Option<(String, bool)>>) -> Element {
    let Some((text, gut)) = meldung() else { return rsx! {} };
    rsx! { span { class: if gut { "ad-meldung gut" } else { "ad-meldung fehler" }, "{text}" } }
}

/// Ein Eingabefeld mit Beschriftung (Optik der alten Admin-Seite).
#[component]
fn Feld(
    beschriftung: String,
    wert: Signal<String>,
    #[props(default = "text".to_string())] art: String,
    #[props(default)] zusatz: String,
    #[props(default)] platzhalter: String,
    #[props(default)] autocomplete: String,
    #[props(default)] pflicht: bool,
    #[props(default)] minlaenge: Option<u32>,
    #[props(default)] maxlaenge: Option<u32>,
) -> Element {
    rsx! {
        label {
            span {
                "{beschriftung}"
                if !zusatz.is_empty() {
                    " "
                    small { "· {zusatz}" }
                }
            }
            input {
                r#type: "{art}",
                value: "{wert}",
                placeholder: "{platzhalter}",
                autocomplete: if autocomplete.is_empty() { "off".to_string() } else { autocomplete.clone() },
                required: pflicht,
                minlength: minlaenge.map(|m| m.to_string()),
                maxlength: maxlaenge.map(|m| m.to_string()),
                oninput: move |e: FormEvent| wert.set(e.value()),
            }
        }
    }
}

// ---- Der allererste Start ----------------------------------------------------------------

#[component]
fn Einrichtung(ansicht: Signal<Ansicht>) -> Element {
    let name = use_signal(String::new);
    let email = use_signal(String::new);
    let passwort = use_signal(String::new);
    let schluessel = use_signal(String::new);
    let modell = use_signal(|| "openrouter/free".to_string());
    let mut meldung = use_signal(|| None::<(String, bool)>);
    rsx! {
        div { class: "ad-seite ad-schmal",
            h1 { span { class: "ki-stern" } "nrzSearch einrichten" }
            p { class: "ad-text",
                "Willkommen! Lege das Admin-Konto an - damit erreichst du später unter "
                b { "/admin" }
                " die Einstellungen. Besucher der Suche brauchen kein Konto."
            }
            form {
                class: "ad-form",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    let daten = json!({
                        "name": name(), "email": email(), "passwort": passwort(),
                        "schluessel": schluessel(), "modell": modell(),
                    });
                    spawn(async move {
                        match admin_post("einrichten", daten).await {
                            Ok(a) => {
                                token_setzen(s(&a, "token"));
                                bild_aktualisieren().await;
                                let ki = a.get("ki_fehler").and_then(Value::as_str).filter(|f| !f.is_empty()).map(String::from);
                                ansicht.set(Ansicht::Dashboard(ki));
                            }
                            Err(f) => meldung.set(Some((f.text, false))),
                        }
                    });
                },
                Feld { beschriftung: "Name", wert: name, autocomplete: "name", maxlaenge: 40 }
                Feld { beschriftung: "E-Mail", wert: email, art: "email", autocomplete: "email", pflicht: true }
                Feld {
                    beschriftung: "Passwort",
                    wert: passwort,
                    art: "password",
                    autocomplete: "new-password",
                    pflicht: true,
                    minlaenge: 8,
                    platzhalter: "mindestens 8 Zeichen",
                }
                h3 { "Dieter (KI) - OpenRouter" }
                p { class: "ad-text klein",
                    "Für die Übersicht mit KI, den KI-Modus und Vergleiche. Ein kostenloser Schlüssel: "
                    a { href: "https://openrouter.ai/keys", target: "_blank", rel: "noreferrer", "openrouter.ai/keys" }
                    ". Geht auch später."
                }
                Feld { beschriftung: "OpenRouter-Schlüssel", wert: schluessel, art: "password", platzhalter: "sk-or-…" }
                Feld { beschriftung: "Modell", wert: modell }
                Button { button_type: "submit", class: "h-10 rounded-full", "Einrichten" }
                Meldung { meldung }
            }
        }
    }
}

#[component]
fn Anmeldung(ansicht: Signal<Ansicht>) -> Element {
    let email = use_signal(String::new);
    let passwort = use_signal(String::new);
    let mut meldung = use_signal(|| None::<(String, bool)>);
    rsx! {
        div { class: "ad-seite ad-schmal",
            h1 { "Admin" }
            form {
                class: "ad-form",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    let daten = json!({ "email": email(), "passwort": passwort() });
                    spawn(async move {
                        match admin_post("anmelden", daten).await {
                            Ok(a) => {
                                token_setzen(s(&a, "token"));
                                bild_aktualisieren().await;
                                ansicht.set(Ansicht::Dashboard(None));
                            }
                            Err(f) => meldung.set(Some((f.text, false))),
                        }
                    });
                },
                Feld { beschriftung: "E-Mail", wert: email, art: "email", autocomplete: "email", pflicht: true }
                Feld { beschriftung: "Passwort", wert: passwort, art: "password", autocomplete: "current-password", pflicht: true }
                Button { button_type: "submit", class: "h-10 rounded-full", "Anmelden" }
                Meldung { meldung }
            }
        }
    }
}

// ---- Das Dashboard -----------------------------------------------------------------------

/// Ein Bild quadratisch auf 256 px zuschneiden (JPEG als data:-URL).
async fn bild_vorbereiten(datei: web_sys::File) -> Option<String> {
    let w = web_sys::window()?;
    let bild: web_sys::ImageBitmap = JsFuture::from(w.create_image_bitmap_with_blob(&datei).ok()?).await.ok()?.dyn_into().ok()?;
    let (b, h) = (f64::from(bild.width()), f64::from(bild.height()));
    let seite = b.min(h);
    let leinwand: web_sys::HtmlCanvasElement = w.document()?.create_element("canvas").ok()?.dyn_into().ok()?;
    leinwand.set_width(256);
    leinwand.set_height(256);
    let ctx: web_sys::CanvasRenderingContext2d = leinwand.get_context("2d").ok()??.dyn_into().ok()?;
    ctx.draw_image_with_image_bitmap_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(&bild, (b - seite) / 2.0, (h - seite) / 2.0, seite, seite, 0.0, 0.0, 256.0, 256.0).ok()?;
    leinwand.to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from_f64(0.88)).ok()
}

#[component]
fn Dashboard(ansicht: Signal<Ansicht>, ki_fehler: Option<String>) -> Element {
    let neu = use_signal(|| 0u32);
    let daten = use_resource(move || async move {
        let _ = neu();
        admin_get("uebersicht").await
    });
    // Abgelaufen oder abgemeldet: zur Anmeldung.
    use_effect(move || {
        if matches!(&*daten.read(), Some(Err(_))) {
            ansicht.set(Ansicht::Anmeldung);
        }
    });
    let gelesen = daten.read().clone();
    let d = match gelesen {
        None => return rsx! { div { class: "ad-seite flex justify-center py-16", Spinner { class: "size-6" } } },
        Some(Err(_)) => return rsx! {},
        Some(Ok(d)) => d,
    };
    let profil = d.get("profil").cloned().unwrap_or_else(|| json!({}));
    let st = d.get("statistik").cloned().unwrap_or_else(|| json!({}));
    let ablagen = d.get("ablagen").cloned().unwrap_or_else(|| json!({}));
    let ki = d.get("ki").cloned().unwrap_or_else(|| json!({}));
    let ki_bereit = ki.get("bereit").and_then(Value::as_bool) == Some(true);
    let alle_tage: Vec<(String, Vec<f64>)> = st
        .get("tage")
        .and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default())).collect())
        .unwrap_or_default();
    let tage: Vec<(String, Vec<f64>)> = alle_tage.iter().skip(alle_tage.len().saturating_sub(30)).cloned().collect();
    let tag = |t: &str| t.get(5..).unwrap_or(t).to_string();
    let wert = |w: &[f64], i: usize| w.get(i).copied().unwrap_or(0.0);
    let linie = json!({
        "typ": "linie", "titel": "Suchen und KI-Antworten je Tag",
        "reihen": [
            { "name": "Suchen", "punkte": tage.iter().map(|(t, w)| json!([tag(t), wert(w, 0)])).collect::<Vec<_>>() },
            { "name": "KI-Antworten", "punkte": tage.iter().map(|(t, w)| json!([tag(t), wert(w, 1)])).collect::<Vec<_>>() },
        ],
    });
    let balken = json!({
        "typ": "balken", "titel": "Tokens je Tag",
        "daten": tage.iter().skip(tage.len().saturating_sub(14)).map(|(t, w)| json!({ "name": tag(t), "wert": wert(w, 2) })).collect::<Vec<_>>(),
    });
    let seit = st.get("seit").map(|v| JsValue::from_str(&v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))).map(|v| {
        let datum = js_sys::Date::new(&v);
        String::from(datum.to_locale_date_string("de-DE", &JsValue::UNDEFINED))
    });

    rsx! {
        div { class: "ad-seite",
            div { class: "ad-kopf",
                ProfilBild { profil: profil.clone(), gross: true }
                div {
                    h1 { "Admin" }
                    small { "{s(&profil, \"name\")} · {s(&profil, \"email\")}" }
                }
                Button { href: "/", variant: ButtonVariant::Outline, class: "ml-auto h-10 rounded-full", "Zur Suche" }
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-10 rounded-full",
                    onclick: move |_| {
                        spawn(async move {
                            let _ = admin_post("abmelden", json!({})).await;
                            token_setzen("");
                            *ADMIN_PROFIL.write() = None;
                            ansicht.set(Ansicht::Anmeldung);
                        });
                    },
                    "Abmelden"
                }
            }
            if let Some(f) = ki_fehler {
                p { class: "ad-meldung fehler", "OpenRouter: {f} - unten neu eintragen." }
            }
            section { class: "ad-karte",
                h2 { "Insights" }
                div { class: "ad-zahlen",
                    div { b { {zahl(n(&st, "suchen"), 0)} } span { "Suchen" } }
                    div { b { {zahl(n(&st, "antworten"), 0)} } span { "KI-Antworten" } }
                    div { b { {zahl(n(&st, "tokens"), 0)} } span { "Tokens" } }
                    div { b { {zahl(n(&ablagen, "browser"), 0)} } span { "Browser mit Chats" } }
                }
                if tage.len() > 1 {
                    Diagramm { spec: linie }
                    Diagramm { spec: balken }
                } else {
                    p { class: "ad-text klein", "Der Verlauf erscheint ab dem zweiten Tag." }
                }
                p { class: "ad-text klein",
                    if let Some(seit) = seit { "Seit {seit} · " }
                    "nur anonyme Zahlen, keine Suchanfragen, keine Personen."
                }
            }
            section { class: "ad-karte",
                h2 { "System" }
                table { class: "ad-tabelle",
                    tbody {
                        tr { td { "nrzSearch" } td { {s(&d, "version")} } }
                        tr { td { "SearXNG" } td { SearxngStatus { status: d.get("searxng").cloned().unwrap_or(Value::Null), instanz: s(&d, "instanz").to_string() } } }
                        tr {
                            td { "Chats (verschlüsselt)" }
                            td { "{zahl(n(&ablagen, \"browser\"), 0)} Browser · {groesse_text(n(&ablagen, \"groesse\") as u64)}" }
                        }
                        tr {
                            td { "Dieter (KI)" }
                            td {
                                if ki_bereit {
                                    Badge { variant: BadgeVariant::Success, "bereit" }
                                    " {s(&ki, \"modell\")}"
                                } else {
                                    Badge { variant: BadgeVariant::Muted, "nicht eingerichtet" }
                                }
                            }
                        }
                    }
                }
            }
            KiKarte { modell: s(&ki, "modell").to_string(), bereit: ki_bereit }
            ModulKarte { module: d.get("module").cloned().unwrap_or_else(|| json!({})) }
            ProfilKarte { profil, neu }
        }
    }
}

#[component]
fn KiKarte(modell: String, bereit: bool) -> Element {
    let modell = use_signal(|| modell);
    let mut schluessel = use_signal(String::new);
    let mut meldung = use_signal(|| None::<(String, bool)>);
    rsx! {
        section { class: "ad-karte",
            h2 { "OpenRouter" }
            form {
                class: "ad-form",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    let mut daten = json!({ "modell": modell().trim() });
                    let k = schluessel().trim().to_string();
                    if !k.is_empty() {
                        daten["schluessel"] = json!(k);
                    }
                    spawn(async move {
                        match admin_post("ki", daten).await {
                            Ok(_) => {
                                schluessel.set(String::new());
                                meldung.set(Some(("Gespeichert.".into(), true)));
                            }
                            Err(f) => meldung.set(Some((f.text, false))),
                        }
                    });
                },
                Feld { beschriftung: "Modell", wert: modell, platzhalter: "openrouter/free" }
                Feld {
                    beschriftung: "Schlüssel",
                    zusatz: "leer lassen = unverändert",
                    wert: schluessel,
                    art: "password",
                    platzhalter: if bereit { "•••••• eingetragen" } else { "sk-or-…" },
                }
                div { class: "ad-zeile",
                    Button { button_type: "submit", class: "h-10 rounded-full", "Speichern" }
                    Meldung { meldung }
                }
            }
        }
    }
}

#[component]
fn ModulKarte(module: Value) -> Element {
    let schalter_start: Vec<(String, bool)> = module
        .get("module")
        .and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, m)| (k.clone(), m.get("an").and_then(Value::as_bool) != Some(false))).collect())
        .unwrap_or_default();
    let mut schalter = use_signal(|| schalter_start);
    let mut konfig = use_signal(|| serde_json::to_string_pretty(&module).unwrap_or_default());
    let mut meldung = use_signal(|| None::<(String, bool)>);
    rsx! {
        section { class: "ad-karte",
            h2 { "Module" }
            form {
                class: "ad-form",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    let Ok(mut json) = serde_json::from_str::<Value>(&konfig.peek()) else {
                        meldung.set(Some(("Das JSON ist nicht gültig.".into(), false)));
                        return;
                    };
                    for (name, an) in schalter.peek().iter() {
                        if let Some(m) = json.get_mut("module").and_then(|m| m.get_mut(name)).and_then(Value::as_object_mut) {
                            m.insert("an".into(), json!(an));
                        }
                    }
                    spawn(async move {
                        match admin_post("module", json!({ "module": json })).await {
                            Ok(_) => {
                                konfig.set(serde_json::to_string_pretty(&json).unwrap_or_default());
                                *MODUL_KONFIG.write() = Some(json);
                                meldung.set(Some(("Gespeichert - gilt ab der nächsten Suche.".into(), true)));
                            }
                            Err(f) => meldung.set(Some((f.text, false))),
                        }
                    });
                },
                p { class: "ad-text", "Welche Antworten über den Treffern erscheinen. Gilt ab der nächsten Suche." }
                div { class: "ad-module",
                    for (i, (name, an)) in schalter.read().iter().cloned().enumerate() {
                        label { key: "{name}", class: "ad-schalter",
                            input {
                                r#type: "checkbox",
                                checked: an,
                                onchange: move |e: FormEvent| {
                                    if let Some(s) = schalter.write().get_mut(i) {
                                        s.1 = e.checked();
                                    }
                                },
                            }
                            span {}
                            "{name}"
                        }
                    }
                }
                details { class: "ad-mehr",
                    summary { "module.json bearbeiten" }
                    textarea { name: "konfig", spellcheck: "false", value: "{konfig}", oninput: move |e: FormEvent| konfig.set(e.value()) }
                }
                div { class: "ad-zeile",
                    Button { button_type: "submit", class: "h-10 rounded-full", "Speichern" }
                    Meldung { meldung }
                }
            }
        }
    }
}

#[component]
fn ProfilKarte(profil: Value, neu: Signal<u32>) -> Element {
    let name = use_signal(|| s(&profil, "name").to_string());
    let mut passwort = use_signal(String::new);
    let mut meldung = use_signal(|| None::<(String, bool)>);
    let hat_bild = !s(&profil, "bild").is_empty();
    // Ein neues Bild oder keins: Profil speichern, Bild oben rechts und Dashboard neu.
    let bild_speichern = move |bild: Value| {
        spawn(async move {
            match admin_post("profil", json!({ "bild": bild })).await {
                Ok(_) => {
                    bild_aktualisieren().await;
                    neu += 1;
                }
                Err(f) => meldung.set(Some((f.text, false))),
            }
        });
    };
    rsx! {
        section { class: "ad-karte",
            h2 { "Profil" }
            div { class: "ad-profil",
                div { class: "ad-bildwahl",
                    ProfilBild { profil: profil.clone(), gross: true }
                    label { class: "ad-knopf",
                        "Bild ändern"
                        input {
                            r#type: "file",
                            id: "ad-bild-datei",
                            accept: "image/*",
                            hidden: true,
                            onchange: move |_| {
                                let Some(feld) = crate::components::helfer::element::<web_sys::HtmlInputElement>("ad-bild-datei") else { return };
                                let Some(datei) = feld.files().and_then(|l| l.get(0)) else { return };
                                feld.set_value("");
                                spawn(async move {
                                    match bild_vorbereiten(datei).await {
                                        Some(url) => bild_speichern(json!(url)),
                                        None => meldung.set(Some(("Das Bild ließ sich nicht lesen.".into(), false))),
                                    }
                                });
                            },
                        }
                    }
                    if hat_bild {
                        Button { variant: ButtonVariant::Ghost, class: "rounded-full", onclick: move |_| bild_speichern(Value::Null), "Entfernen" }
                    }
                }
                form {
                    class: "ad-form",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        let mut daten = json!({ "name": name() });
                        if !passwort().is_empty() {
                            daten["passwort"] = json!(passwort());
                        }
                        spawn(async move {
                            match admin_post("profil", daten).await {
                                Ok(_) => {
                                    passwort.set(String::new());
                                    meldung.set(Some(("Gespeichert.".into(), true)));
                                    bild_aktualisieren().await;
                                }
                                Err(f) => meldung.set(Some((f.text, false))),
                            }
                        });
                    },
                    Feld { beschriftung: "Name", wert: name, maxlaenge: 40 }
                    Feld {
                        beschriftung: "Neues Passwort",
                        zusatz: "leer lassen = unverändert",
                        wert: passwort,
                        art: "password",
                        autocomplete: "new-password",
                        minlaenge: 8,
                    }
                    div { class: "ad-zeile",
                        Button { button_type: "submit", class: "h-10 rounded-full", "Speichern" }
                        Meldung { meldung }
                    }
                }
            }
        }
    }
}


/// Die eingebaute SearXNG-Instanz: Fassung, Zustand, automatische Updates - und "Jetzt
/// aktualisieren" (holt die neueste Fassung ohne Unterbrechung der Suche).
#[component]
fn SearxngStatus(status: Value, instanz: String) -> Element {
    let mut meldung = use_signal(String::new);
    if status.is_null() {
        return rsx! { "{instanz} (extern)" };
    }
    let text = |k: &str| status.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let laeuft = status.get("laeuft").and_then(Value::as_bool) == Some(true);
    let neueste = text("neueste");
    let version = text("version");
    let auto = status.get("automatisch").and_then(Value::as_u64);
    rsx! {
        div { class: "flex flex-col gap-1",
            div { class: "flex flex-wrap items-center gap-2",
                if laeuft {
                    Badge { variant: BadgeVariant::Success, "läuft" }
                } else {
                    Badge { variant: BadgeVariant::Muted, "startet" }
                }
                span { "eingebaut · {version}" }
                if !neueste.is_empty() && neueste != version {
                    Badge { variant: BadgeVariant::Muted, "neu: {neueste}" }
                }
            }
            small { class: "text-muted-foreground",
                {text("meldung")}
                if let Some(h) = auto { " · automatische Updates alle {h} h" } else { " · automatische Updates aus" }
            }
            div { class: "flex items-center gap-2",
                Button {
                    variant: ButtonVariant::Outline,
                    class: "h-8 rounded-full",
                    onclick: move |_| {
                        spawn(async move {
                            let r = admin_post("searxng", json!({})).await;
                            meldung.set(match r {
                                Ok(_) => "Update wird gesucht - die Suche läuft dabei weiter.".into(),
                                Err(e) => e.text,
                            });
                        });
                    },
                    "Jetzt aktualisieren"
                }
                small { class: "text-muted-foreground", "{meldung}" }
            }
        }
    }
}
