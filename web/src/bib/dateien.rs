//! Dateien fuer den Arbeitsbereich: lesen und anhaengen - Portierung von public/lib/dateien.js.
//!
//! Text, Code, CSV, JSON, Mails … liest der Browser selbst. PDF, Word, Excel, PowerPoint und
//! OpenDocument liest der Server (POST /api/datei - er vergisst die Datei danach). Bilder werden
//! verkleinert und gehen als Bild an Dieter.
//!
//! Fuer den Kern (Suchleisten):
//! - [`hinzufuegen`]`(Vec<web_sys::File>)` - liest die Dateien und haengt sie an `state::ANHAENGE`
//!   (sofort als Chip "liest …", dann fertig oder rot mit Fehler).
//! - [`AnhangChips`]` {}` - die Chips aus `ANHAENGE` mit × zum Entfernen (`div.anhaenge`, leer =
//!   nichts). Einfach unter das Suchfeld setzen.
//! - [`AnhangKnopf`]` {}` - die Bueroklammer mit verstecktem Datei-Feld (haengt an `ANHAENGE`).
//! - [`ablage_dateien`]`(&DragEvent)` - die Dateien aus einem `ondrop` (fuer eigene Ablegezonen).
//! - [`groesse_text`], [`datei_art`], [`fuer_dieter`].

use crate::api;
use crate::state::{ANHAENGE, Datei};
use dioxus::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const TEXT: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "xml", "html", "htm", "css", "js", "mjs", "ts", "tsx", "jsx", "py", "rs", "go", "java", "kt", "c", "h", "cpp", "hpp", "cs", "php", "rb", "sh", "bash", "zsh", "ps1", "sql", "yaml", "yml", "toml", "ini", "cfg", "conf", "log", "eml", "vcf", "ics", "srt",
    "vtt", "tex", "rtf", "env", "gitignore", "dockerfile",
];
const DOKUMENT: &[&str] = &["pdf", "docx", "docm", "dotx", "pptx", "xlsx", "xlsm", "odt", "ods", "odp", "odg"];
const MAX_BYTES: f64 = (50u64 << 20) as f64;

/// Die Bueroklammer (wie im alten app.js).
pub const KLAMMER: &str = "M16.5 6v11.5a4 4 0 0 1-8 0V5a2.5 2.5 0 0 1 5 0v10.5a1 1 0 0 1-2 0V6H10v9.5a2.5 2.5 0 0 0 5 0V5a4 4 0 0 0-8 0v12.5a5.5 5.5 0 0 0 11 0V6h-1.5z";

/// Die Endung (klein), wenn der Name eine hat.
fn endung(name: &str) -> Option<String> {
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase())
}

/// Groesse lesbar: "512 B", "12 KB", "3,4 MB".
pub fn groesse_text(b: u64) -> String {
    if b < 1024 {
        format!("{b} B")
    } else if b < 1 << 20 {
        format!("{} KB", (b as f64 / 1024.0).round())
    } else {
        format!("{:.1} MB", b as f64 / f64::from(1 << 20)).replace('.', ",")
    }
}

/// Ein kurzes Kuerzel fuer den Chip: PDF, DOC, XLS, PPT, BILD, TXT …
pub fn datei_art(name: &str, typ: &str) -> String {
    if typ == "bild" || typ.starts_with("image/") {
        return "BILD".into();
    }
    let e = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match e.as_str() {
        "pdf" => "PDF",
        "docx" | "docm" | "dotx" | "odt" => "DOC",
        "xlsx" | "xlsm" | "ods" => "XLS",
        "csv" => "CSV",
        "pptx" | "odp" => "PPT",
        "eml" => "MAIL",
        "json" => "JSON",
        "md" => "MD",
        _ => "TXT",
    }
    .into()
}

/// Ein Bild verkleinern (laengste Seite hoechstens 1600 px) - als JPEG-data:-URL.
async fn bild_lesen(datei: &web_sys::File) -> Option<String> {
    let fenster = web_sys::window()?;
    let bild: web_sys::ImageBitmap = JsFuture::from(fenster.create_image_bitmap_with_blob(datei).ok()?).await.ok()?.dyn_into().ok()?;
    let (b, h) = (f64::from(bild.width()), f64::from(bild.height()));
    let mass = (1600.0 / b.max(h).max(1.0)).min(1.0);
    let leinwand: web_sys::HtmlCanvasElement = fenster.document()?.create_element("canvas").ok()?.dyn_into().ok()?;
    leinwand.set_width((b * mass).round() as u32);
    leinwand.set_height((h * mass).round() as u32);
    let ctx: web_sys::CanvasRenderingContext2d = leinwand.get_context("2d").ok()??.dyn_into().ok()?;
    ctx.draw_image_with_image_bitmap_and_dw_and_dh(&bild, 0.0, 0.0, f64::from(leinwand.width()), f64::from(leinwand.height())).ok()?;
    leinwand.to_data_url_with_type_and_encoder_options("image/jpeg", &JsValue::from(0.85)).ok()
}

/// Eine Datei lesen: { name, typ: "text"|"bild", groesse, text?, daten?, seiten?, fehler? }.
pub async fn datei_lesen(datei: &web_sys::File) -> Datei {
    let name = datei.name();
    let mime = datei.type_();
    let basis = Datei { name: name.clone(), typ: "text".into(), groesse: datei.size() as u64, ..Default::default() };
    let fehler = |text: &str| Datei { fehler: Some(text.into()), ..basis.clone() };
    if datei.size() > MAX_BYTES {
        return fehler("Größer als 50 MB.");
    }
    if mime.starts_with("image/") {
        return match bild_lesen(datei).await {
            Some(daten) => Datei { typ: "bild".into(), daten: Some(daten), ..basis },
            None => Datei { typ: "bild".into(), ..fehler("Die Datei ließ sich nicht lesen.") },
        };
    }
    let e = endung(&name);
    if e.as_deref().is_some_and(|e| DOKUMENT.contains(&e)) {
        let pfad = api::url_mit("/api/datei", &[("name", &name)]);
        let art = if mime.is_empty() { "application/octet-stream" } else { &mime };
        return match api::post_bytes(&pfad, &JsValue::from(datei.clone()), art).await {
            Ok(v) => Datei {
                text: v.get("text").and_then(Value::as_str).map(String::from),
                seiten: v.get("seiten").and_then(Value::as_u64).map(|s| s as u32),
                ..basis
            },
            Err(f) if f.status != 0 && !f.text.starts_with("Anfrage fehlgeschlagen") => fehler(&f.text),
            Err(_) => fehler("Nicht lesbar."),
        };
    }
    if e.as_deref().is_none_or(|e| TEXT.contains(&e)) || mime.starts_with("text/") {
        let text = match JsFuture::from(datei.text()).await.ok().and_then(|t| t.as_string()) {
            Some(t) => t,
            None => return fehler("Die Datei ließ sich nicht lesen."),
        };
        return Datei { text: Some(text.chars().take(400_000).collect()), ..basis };
    }
    fehler("Diesen Dateityp kann Dieter nicht lesen.")
}

thread_local! {
    static NUMMER: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Dateien lesen und an eine Liste haengen: sofort als "liest …", dann fertig (oder mit Fehler).
pub async fn hinzufuegen_zu(mut liste: Signal<Vec<Datei>>, dateien: Vec<web_sys::File>) {
    let mut auftraege = Vec::new();
    for d in dateien {
        let nr = NUMMER.with(|n| {
            n.set(n.get() + 1);
            n.get()
        });
        let typ = if d.type_().starts_with("image/") { "bild" } else { "text" };
        liste.write().push(Datei { name: d.name(), typ: typ.into(), groesse: d.size() as u64, lesen: true, nr, ..Default::default() });
        auftraege.push(async move {
            let mut fertig = datei_lesen(&d).await;
            fertig.nr = nr;
            // Inzwischen entfernt? Dann nichts mehr tun.
            if let Some(eintrag) = liste.write().iter_mut().find(|x| x.nr == nr) {
                *eintrag = fertig;
            }
        });
    }
    futures::future::join_all(auftraege).await;
}

/// Dateien lesen (Server: PDF/Office ueber /api/datei, Browser: Text; Bilder verkleinert) und an
/// `ANHAENGE` haengen.
pub async fn hinzufuegen(dateien: Vec<web_sys::File>) {
    hinzufuegen_zu(ANHAENGE.signal(), dateien).await;
}

/// Warten, bis keine Datei der Liste mehr gelesen wird.
pub async fn warten_bis_gelesen(liste: Signal<Vec<Datei>>) {
    while liste.peek().iter().any(|d| d.lesen) {
        gloo_timers::future::TimeoutFuture::new(100).await;
    }
}

/// Fuer Dieter: Texte als { name, text }, Bilder als { name, daten }.
pub fn fuer_dieter(dateien: &[Datei]) -> Value {
    let texte: Vec<Value> = dateien.iter().filter(|d| d.typ != "bild" && d.text.as_deref().is_some_and(|t| !t.is_empty())).map(|d| json!({ "name": d.name, "text": d.text })).collect();
    let bilder: Vec<Value> = dateien.iter().filter(|d| d.typ == "bild" && d.daten.is_some()).map(|d| json!({ "name": d.name, "daten": d.daten })).collect();
    json!({ "dateien": texte, "bilder": bilder })
}

/// Die Dateien aus einem Ablegen (`ondrop`) oder Ziehen.
pub fn ablage_dateien(e: &DragEvent) -> Vec<web_sys::File> {
    let liste = e.data().downcast::<web_sys::DragEvent>().and_then(|d| d.data_transfer()).and_then(|t| t.files());
    datei_liste(liste)
}

/// Werden gerade Dateien gezogen?
pub fn zieht_dateien(e: &DragEvent) -> bool {
    e.data().downcast::<web_sys::DragEvent>().and_then(|d| d.data_transfer()).is_some_and(|t| t.types().iter().any(|x| x.as_string().as_deref() == Some("Files")))
}

/// Die Dateien aus einem `<input type=file>` (onchange) - danach wird das Feld geleert.
pub fn feld_dateien(e: &FormEvent) -> Vec<web_sys::File> {
    let feld = e.data().downcast::<web_sys::Event>().and_then(|ev| ev.target()).and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
    let Some(feld) = feld else { return Vec::new() };
    let dateien = datei_liste(feld.files());
    feld.set_value("");
    dateien
}

fn datei_liste(liste: Option<web_sys::FileList>) -> Vec<web_sys::File> {
    liste.map(|l| (0..l.length()).filter_map(|i| l.get(i)).collect()).unwrap_or_default()
}

/// Ein Chip fuer eine Datei; mit `weg` gibt es ein × zum Entfernen.
#[component]
pub fn DateiChip(datei: Datei, weg: Option<EventHandler<()>>) -> Element {
    let mut klasse = String::from("datei-chip");
    if datei.fehler.is_some() {
        klasse.push_str(" fehler");
    }
    if datei.lesen {
        klasse.push_str(" liest");
    }
    let titel = datei.fehler.clone().unwrap_or_else(|| datei.name.clone());
    let klein = if datei.lesen {
        "liest …".to_string()
    } else if datei.fehler.is_some() {
        "nicht lesbar".to_string()
    } else if let Some(s) = datei.seiten.filter(|s| *s > 0) {
        format!("{s} S.")
    } else {
        groesse_text(datei.groesse)
    };
    rsx! {
        span { class: "{klasse}", title: "{titel}",
            if let (true, Some(src)) = (datei.typ == "bild", datei.daten.as_ref()) {
                img { src: "{src}", alt: "" }
            } else {
                span { class: "datei-art", "{datei_art(&datei.name, &datei.typ)}" }
            }
            span { class: "datei-name", "{datei.name}" }
            small { "{klein}" }
            if let Some(weg) = weg {
                button {
                    r#type: "button",
                    class: "datei-weg",
                    "aria-label": "{datei.name} entfernen",
                    onclick: move |_| weg.call(()),
                    "×"
                }
            }
        }
    }
}

/// Die Chips einer Liste mit × (`div.anhaenge`, leer = nichts).
#[component]
pub fn ChipListe(liste: Signal<Vec<Datei>>, #[props(default)] class: String) -> Element {
    let mut liste = liste;
    let dateien = liste.read().clone();
    if dateien.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "anhaenge {class}",
            for d in dateien {
                DateiChip {
                    key: "{d.nr}-{d.name}",
                    datei: d.clone(),
                    weg: move |_| liste.write().retain(|x| !(x.nr == d.nr && x.name == d.name)),
                }
            }
        }
    }
}

/// Die Chips aus `ANHAENGE` (Suchleiste) mit × zum Entfernen.
#[component]
pub fn AnhangChips() -> Element {
    rsx! { ChipListe { liste: ANHAENGE.signal() } }
}

/// Die Bueroklammer: oeffnet die Dateiauswahl und haengt die Dateien an `liste` (Standard:
/// `ANHAENGE`).
#[component]
pub fn AnhangKnopf(liste: Option<Signal<Vec<Datei>>>) -> Element {
    let liste = liste.unwrap_or_else(|| ANHAENGE.signal());
    let mut feld: Signal<Option<web_sys::HtmlInputElement>> = use_signal(|| None);
    rsx! {
        button {
            r#type: "button",
            class: "anhang-knopf",
            "aria-label": "Dateien anhängen",
            title: "Dateien anhängen - oder einfach hierher ziehen",
            onclick: move |_| {
                if let Some(f) = feld.peek().as_ref() {
                    f.click();
                }
            },
            svg { view_box: "0 0 24 24", "aria-hidden": "true", path { fill: "currentColor", d: KLAMMER } }
        }
        input {
            r#type: "file",
            multiple: true,
            hidden: true,
            onmounted: move |e| feld.set(e.data().downcast::<web_sys::Element>().and_then(|el| el.clone().dyn_into().ok())),
            onchange: move |e| {
                let dateien = feld_dateien(&e);
                if !dateien.is_empty() {
                    spawn(hinzufuegen_zu(liste, dateien));
                }
            },
        }
    }
}
