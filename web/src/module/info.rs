//! Modul "info" - Portierung von public/module/info.js.
//!
//! Die Info-Karte wie bei Google - fuer Orte (mit Bildern, Karte von OpenStreetMap und Wetter),
//! Personen, Unternehmen und Begriffe. Zuerst kommt die Kurzfassung (/api/kurz, ein Aufruf an
//! die Wikipedia) - so steht der Text gleichzeitig mit den Treffern da. Bilder, Karte, Fakten
//! und Wetter kommen mit /api/ort nach, bis dahin mit Ladeanimation. Der Server zeigt nur die
//! Hauptbedeutung eines Wortes - "Moin" ist der Gruss, nicht die Gemeinde in Tibet.
//!
//! Aufbau: die Leiste (Titel, Bilder, Karte, Wetter bzw. Profile) steht im eigenen Platz
//! ("oben"); die Info rechts (Text, Fakten, Kurs, Links) wird wie im Original an den Anfang von
//! `#seite` gesetzt, "Auch in den Nachrichten" an den Anfang von `#widgets` (Platz "haupt").
//! Fehlt das Ziel, bleibt der Teil im eigenen Platz stehen.
//!
//! Einstellungen: "wetter", "karte", "nachrichten" (true/false), "nachrichtenTage", "nicht"
//! (Woerter, zu denen es keine Info-Karte gibt).

use super::ModulCtx;
use crate::api;
use crate::bib::karte::Karte;
use crate::bib::wetter::{WetterIcon, code_aus, grad, tag_aus, wetter_text, wochentag};
use crate::state::{SUCHE, Treffer};
use crate::werkzeug::{KopierKnopf, ModulCss, NewsListe, zahl};
use dioxus::prelude::*;
use futures::FutureExt;
use futures::future::{LocalBoxFuture, Shared};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Zeigt die Info-Karte gerade ein Unternehmen mit Kurs? Dann blendet sich das Kurs-Modul aus
/// (Kurs nicht doppelt).
pub static FIRMA_SICHTBAR: GlobalSignal<bool> = Signal::global(|| false);

type Voll = Shared<LocalBoxFuture<'static, Option<Value>>>;

thread_local! {
    /// Die laufenden/fertigen Anfragen an /api/ort - erkennen startet sie, Zeigen wartet darauf.
    static VOLL: RefCell<HashMap<String, Voll>> = RefCell::new(HashMap::new());
}

/// Die ganze Info (/api/ort) - einmal je Name und Sprache, sofort gestartet.
fn voll_holen(name: &str, sprache: &str) -> Voll {
    let schluessel = format!("{name}|{sprache}");
    VOLL.with(|v| {
        let mut v = v.borrow_mut();
        if let Some(f) = v.get(&schluessel) {
            return f.clone();
        }
        if v.len() > 30 {
            v.clear();
        }
        let (n, s) = (name.to_string(), sprache.to_string());
        let f: Voll = async move {
            api::get_oder_nichts("/api/ort", &[("q", &n), ("sprache", &s)]).await.and_then(|d| d.get("ort").cloned()).filter(|o| o.is_object())
        }
        .boxed_local()
        .shared();
        // Gleich losschicken - nicht erst, wenn jemand wartet.
        wasm_bindgen_futures::spawn_local(f.clone().map(|_| ()));
        v.insert(schluessel, f.clone());
        f
    })
}

/// Erst die schnelle Kurzfassung; die volle Info laeuft dabei schon mit.
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let name = ctx.gruppe(1)?.to_string();
    // Woerter, die ein anderes Modul meint ("wetter"), sind keine Info.
    let nicht = ctx.cfg("nicht").as_array().into_iter().flatten().filter_map(Value::as_str).any(|w| w.to_lowercase() == name.to_lowercase());
    if name.split_whitespace().count() > 4 || nicht {
        return None;
    }
    let sprache = ctx.einstellungen().sprache;
    let voll = voll_holen(&name, &sprache);
    let kurz = api::get_oder_nichts("/api/kurz", &[("q", &name), ("sprache", &sprache)]).await.and_then(|d| d.get("kurz").cloned()).filter(|k| k.is_object());
    if let Some(kurz) = kurz {
        return Some(json!({ "name": name, "sprache": sprache, "kurz": kurz }));
    }
    let ort = voll.await?;
    Some(json!({ "name": name, "sprache": sprache, "voll": ort }))
}

// ---- kleine Helfer -----------------------------------------------------------------------

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn f(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|t| t.parse().ok()))
}

fn liste<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// Ein Wert als Text (Zahlen ohne Anfuehrungszeichen).
fn text_von(v: &Value) -> String {
    match v {
        Value::String(t) => t.clone(),
        Value::Null => String::new(),
        andere => andere.to_string(),
    }
}

fn zahl_text(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::String(t) => t.clone(),
        _ => String::new(),
    }
}

fn osm_link(ort: &Value) -> String {
    format!("https://www.openstreetmap.org/#map=15/{}/{}", zahl_text(&ort["lat"]), zahl_text(&ort["lon"]))
}

/// Wikipedia-Link aus dem OSM-Tag ("de:Berlin" -> de.wikipedia.org/wiki/Berlin).
fn wikipedia_link(ort: &Value) -> String {
    let tag = s(ort, "wikipedia");
    if tag.is_empty() {
        return String::new();
    }
    match tag.split_once(':') {
        Some((sprache, rest)) => format!("https://{sprache}.wikipedia.org/wiki/{}", urlencoding::encode(rest)),
        None => format!("https://de.wikipedia.org/wiki/{}", urlencoding::encode(tag)),
    }
}

/// Ein Link nach draussen (neuer Tab), wie `link()` in werkzeug.js.
#[component]
fn Verweis(url: String, text: String, #[props(default)] class: String) -> Element {
    rsx! {
        a { class: "{class}", href: "{url}", target: "_blank", rel: "noreferrer", "{text}" }
    }
}

/// Setzt seinen Inhalt an den Anfang des Elements `ziel` (wie `insertAdjacentHTML('afterbegin')`
/// im Original). Dioxus zeichnet den Inhalt weiter; beim Verschwinden wird er mit entfernt.
/// Gibt es das Ziel nicht, bleibt der Inhalt, wo er ist.
#[component]
fn Einsetzen(ziel: &'static str, children: Element) -> Element {
    let zelle = use_hook(|| Rc::new(RefCell::new(None::<web_sys::Element>)));
    let z2 = zelle.clone();
    use_drop(move || {
        if let Some(el) = z2.borrow_mut().take() {
            el.remove();
        }
    });
    rsx! {
        div {
            class: "info-einsatz",
            onmounted: move |e| {
                let Some(el) = e.data().downcast::<web_sys::Element>().cloned() else { return };
                let doc = web_sys::window().and_then(|w| w.document());
                if let Some(ort) = doc.and_then(|d| d.query_selector(ziel).ok().flatten()) {
                    if ort.prepend_with_node_1(&el).is_ok() {
                        *zelle.borrow_mut() = Some(el);
                    }
                }
            },
            {children}
        }
    }
}

// ---- Zeigen ------------------------------------------------------------------------------

/// Zeichnet das Modul (steht in `<section class="modul modul-info">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let name = s(&daten, "name").to_string();
    let sprache = s(&daten, "sprache").to_string();
    let kurz = daten.get("kurz").filter(|k| k.is_object()).cloned();
    let cfg = ctx.cfg.clone();
    let lauf = ctx.lauf;

    let mut ort = use_signal(|| None::<Value>);
    let mut geladen = use_signal(|| false);
    let mut finanz = use_signal(|| None::<Value>);
    let mut news = use_signal(Vec::<Treffer>::new);

    use_drop(move || {
        if *FIRMA_SICHTBAR.peek() {
            *FIRMA_SICHTBAR.write() = false;
        }
    });

    {
        let (daten, cfg, name, sprache) = (daten.clone(), cfg.clone(), name.clone(), sprache.clone());
        use_future(move || {
            let (daten, cfg, name, sprache) = (daten.clone(), cfg.clone(), name.clone(), sprache.clone());
            async move {
                let o = match daten.get("voll") {
                    Some(v) => Some(v.clone()),
                    None => voll_holen(&name, &sprache).await,
                };
                if !crate::state::aktuell(lauf) {
                    return;
                }
                geladen.set(true);
                let Some(o) = o else { return };
                ort.set(Some(o.clone()));
                let art = s(&o, "art").to_string();
                let ort_name = s(&o, "name").to_string();
                if art == "unternehmen" {
                    // Der Kurs steht dann rechts in der Info - nicht noch einmal gross.
                    *FIRMA_SICHTBAR.write() = true;
                    let n = ort_name.clone();
                    spawn(async move {
                        if let Some(d) = finanz_holen(&n).await {
                            if crate::state::aktuell(lauf) {
                                finanz.set(Some(d));
                            }
                        }
                    });
                }
                if cfg.get("nachrichten").and_then(Value::as_bool) != Some(false) && (art == "person" || art == "unternehmen") {
                    let tage = cfg.get("nachrichtenTage").and_then(Value::as_f64).filter(|t| *t > 0.0).unwrap_or(7.0);
                    let t = neuigkeiten_holen(&ort_name, &sprache, tage).await;
                    if crate::state::aktuell(lauf) {
                        news.set(t);
                    }
                }
            }
        });
    }

    // Die Infobox der Instanz ergaenzt die Fakten (z. B. "Art: Gemeinde"), sobald die Suche fertig ist.
    let kasten = use_memo(|| {
        let suche = SUCHE.read();
        if suche.fertig { suche.hinweise.first().cloned().filter(|h| h.is_object()) } else { None }
    });
    let kasten = kasten();

    // Rechts: die ganze Info, sonst die Kurzfassung mit Ladeanimation.
    let rechts = match (ort(), &kurz) {
        (Some(o), _) => Some((o, false)),
        (None, Some(k)) => {
            let mut k = k.clone();
            k["art"] = json!(if k.get("ort").is_some_and(|o| !o.is_null() && o != &json!(false)) { "ort" } else { "begriff" });
            k["fakten"] = json!([]);
            Some((k, !geladen()))
        }
        _ => None,
    };
    let leiste = ort().filter(|o| !matches!(s(o, "art"), "unternehmen" | "begriff"));
    let ohne_wetter = *super::wetter::WETTER_LAUF.read() == Some(lauf);
    let vorab = kurz.as_ref().map(|k| s(k, "bild").to_string()).unwrap_or_default();
    let neu = news();

    rsx! {
        ModulCss { name: "info".to_string(), css: CSS }
        ModulCss { name: "kurs".to_string(), css: super::kurs::CSS }
        if let Some(o) = leiste {
            OrtLeiste { ort: o, cfg: cfg.clone(), vorab, sprache: sprache.clone(), ohne_wetter }
        }
        if let Some((o, laedt)) = rechts {
            Einsetzen { ziel: "#seite",
                InfoRechts { ort: o, kasten, laedt, finanz: finanz() }
            }
        }
        if !neu.is_empty() {
            Einsetzen { ziel: "#widgets",
                section { class: "ort-news",
                    h3 { "Auch in den Nachrichten" }
                    NewsListe { treffer: neu, frage: ort().map(|o| s(&o, "name").to_string()).unwrap_or_default() }
                }
            }
        }
    }
}

/// Der Finanzteil: gibt es die Firma an der Boerse, steht der Kurs darunter.
async fn finanz_holen(name: &str) -> Option<Value> {
    let anfrage = super::kurs::kurs_anfrage(&format!("{name} aktie"))?;
    let d = super::kurs::kurs_holen(&anfrage.symbol, "1d").await?;
    f(&d["kurs"]).filter(|k| *k != 0.0)?;
    Some(d)
}

/// Die Nachrichten zu einer Person oder Firma: nur Frisches (`tage`), das Neueste zuerst.
async fn neuigkeiten_holen(name: &str, sprache: &str, tage: f64) -> Vec<Treffer> {
    // Ohne Zeitraum: DuckDuckGo News kennt ihn nicht und liefert sonst nichts.
    let Some(d) = api::get_oder_nichts("/api/search", &[("q", name), ("kategorie", "news"), ("sprache", sprache)]).await else { return Vec::new() };
    let grenze = js_sys::Date::now() - tage * 864e5;
    let zeit = |t: &Treffer| t.datum.as_deref().map_or(f64::NAN, js_sys::Date::parse);
    let mut treffer: Vec<Treffer> = serde_json::from_value::<Vec<Treffer>>(d.get("treffer").cloned().unwrap_or(json!([])))
        .unwrap_or_default()
        .into_iter()
        .filter(|t| zeit(t) > grenze)
        .collect();
    treffer.sort_by(|a, b| zeit(b).total_cmp(&zeit(a)));
    treffer.truncate(6);
    treffer
}

// ---- Die Leiste oben ---------------------------------------------------------------------

fn hat_wetter(art: &str) -> bool {
    matches!(art, "stadt" | "land" | "bundesland" | "gemeinde")
}

/// Die Leiste oben: Titel, Untertitel, Bilder, Karte und Wetter - bei einer Person statt der
/// Karte die Profile und der Wikipedia-Text.
#[component]
fn OrtLeiste(ort: Value, cfg: Value, vorab: String, sprache: String, ohne_wetter: bool) -> Element {
    let mut menue = use_signal(|| false);
    let art = s(&ort, "art").to_string();
    let person = art == "person";
    let hat_punkt = f(&ort["lat"]).is_some() && f(&ort["lon"]).is_some();
    let osm = if !s(&ort, "osmTyp").is_empty() && !text_von(&ort["osmId"]).is_empty() {
        format!("https://www.openstreetmap.org/{}/{}", s(&ort, "osmTyp"), text_von(&ort["osmId"]))
    } else {
        String::new()
    };
    let wiki = s(&ort, "wikiUrl").to_string();
    let wikidata = s(&ort, "wikidata").to_string();
    let karte = !person && cfg.get("karte").and_then(Value::as_bool) != Some(false);
    let wetter = !person && hat_wetter(&art) && cfg.get("wetter").and_then(Value::as_bool) != Some(false) && !ohne_wetter;
    let name = s(&ort, "name").to_string();
    let untertitel = s(&ort, "untertitel").to_string();
    let koordinaten = format!("{}, {}", zahl_text(&ort["lat"]), zahl_text(&ort["lon"]));

    rsx! {
        section { class: "ort ort-{art}", "aria-label": "Angaben zu {name}",
            div { class: "ort-kopf",
                div { class: "ort-namen",
                    h2 { class: "ort-titel", "{name}" }
                    if !untertitel.is_empty() {
                        span { class: "ort-untertitel", "{untertitel}" }
                    }
                }
                button {
                    r#type: "button",
                    class: "ort-menue-knopf",
                    "aria-expanded": "{menue()}",
                    "aria-controls": "ort-menue-liste",
                    "aria-label": "Weitere Angaben",
                    title: "Weitere Angaben",
                    onclick: move |_| menue.toggle(),
                    "⋮"
                }
                div {
                    class: "ort-menue-liste",
                    id: "ort-menue-liste",
                    hidden: !menue(),
                    onclick: move |e| {
                        // Ein Link schliesst das Menue, der Kopierknopf nicht (er zeigt "Kopiert ✓").
                        let link = e
                            .data()
                            .downcast::<web_sys::MouseEvent>()
                            .and_then(|m| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(m.target()?).ok())
                            .is_some_and(|t| t.closest("a").ok().flatten().is_some());
                        if link {
                            menue.set(false);
                        }
                    },
                    if hat_punkt {
                        Verweis { url: osm_link(&ort), text: "Auf OpenStreetMap öffnen" }
                    }
                    if !osm.is_empty() {
                        Verweis { url: osm.clone(), text: "OSM-Objekt ansehen" }
                    }
                    if !wiki.is_empty() {
                        Verweis { url: wiki.clone(), text: "Wikipedia" }
                    }
                    if !wikidata.is_empty() {
                        Verweis { url: "https://www.wikidata.org/wiki/{wikidata}", text: "Wikidata" }
                    }
                    if hat_punkt {
                        KopierKnopf { text: koordinaten, beschriftung: "Koordinaten kopieren" }
                    }
                }
            }
            div { class: "ort-rumpf",
                OrtBilder { ort: ort.clone(), vorab, sprache }
                if karte {
                    div { class: "ort-karte",
                        div { class: "ort-map",
                            if hat_punkt {
                                Karte {
                                    lat: f(&ort["lat"]).unwrap_or(0.0),
                                    lon: f(&ort["lon"]).unwrap_or(0.0),
                                    zoom: 14,
                                    name: name.clone(),
                                    grenze: ort.get("grenze").filter(|g| g.is_object()).cloned(),
                                    rad_zoom: false,
                                }
                            } else {
                                div { class: "ort-karte-hinweis", "Für diesen Ort gibt es keine Koordinaten." }
                            }
                        }
                    }
                }
                div { class: "ort-spalte",
                    if person {
                        PersonKarte { ort: ort.clone() }
                    } else if wetter && hat_punkt {
                        section { class: "karte ort-wetter",
                            OrtWetter { ort: ort.clone() }
                        }
                    }
                }
            }
        }
    }
}

/// Ein Bild der Leiste: Vorschau, Ziel, Titel.
#[derive(Clone, PartialEq)]
struct Bild {
    bild: String,
    url: String,
    titel: String,
}

/// Bilder fuer die Leiste: bei Personen die Fotos aus dem Wikipedia-Artikel, sonst die
/// Bildsuche (mit dem Bild aus Wikimedia vorn).
#[component]
fn OrtBilder(ort: Value, vorab: String, sprache: String) -> Element {
    let name = s(&ort, "name").to_string();
    let eigene: Vec<Bild> = liste(&ort, "bilder")
        .iter()
        .map(|b| Bild {
            bild: s(b, "url").to_string(),
            url: Some(s(b, "voll")).filter(|v| !v.is_empty()).unwrap_or(s(b, "url")).to_string(),
            titel: Some(s(b, "titel")).filter(|v| !v.is_empty()).unwrap_or(&name).to_string(),
        })
        .collect();
    // Das Bild aus der Wikipedia steht sofort gross da, die Bildsuche laeuft nach.
    let erstes = Some(s(&ort, "bild")).filter(|b| !b.is_empty()).map(String::from).unwrap_or(vorab);
    let bilder = {
        let (eigene, erstes, name, sprache, wiki) = (eigene.clone(), erstes.clone(), name.clone(), sprache.clone(), s(&ort, "wikiUrl").to_string());
        use_resource(move || {
            let (eigene, erstes, name, sprache, wiki) = (eigene.clone(), erstes.clone(), name.clone(), sprache.clone(), wiki.clone());
            async move {
                if !eigene.is_empty() {
                    return eigene;
                }
                let d = api::get_oder_nichts("/api/search", &[("q", &name), ("kategorie", "images"), ("sprache", &sprache)]).await;
                let mut b: Vec<Bild> = d
                    .as_ref()
                    .map(|d| liste(d, "treffer"))
                    .unwrap_or(&[])
                    .iter()
                    .filter(|t| !s(t, "bild").is_empty())
                    .map(|t| Bild { bild: s(t, "bild").to_string(), url: s(t, "url").to_string(), titel: s(t, "titel").to_string() })
                    .collect();
                if !erstes.is_empty() {
                    let url = if wiki.is_empty() { "https://commons.wikimedia.org/".to_string() } else { wiki };
                    b.insert(0, Bild { bild: erstes, url, titel: name });
                }
                b
            }
        })
    };

    let leer_rahmen = rsx! { span { class: "rahmen schimmer" } };
    let inhalt = match &*bilder.read() {
        // Ladeanimation: Platzhalter, die schimmern, bis die echten Bilder da sind.
        None => rsx! {
            div { class: "bild ort-gross",
                if erstes.is_empty() {
                    {leer_rahmen.clone()}
                } else {
                    span { class: "rahmen schimmer", img { src: "{erstes}", alt: "{name}" } }
                }
            }
            div { class: "ort-bilder-klein",
                div { class: "bild ort-breit", {leer_rahmen.clone()} }
                div { class: "bild", {leer_rahmen.clone()} }
                div { class: "bild", {leer_rahmen.clone()} }
            }
        },
        Some(b) if b.is_empty() => rsx! {
            div { class: "ort-leer",
                "Keine Bilder gefunden · "
                a { class: "ort-link", href: "/?q={urlencoding::encode(&name)}&kategorie=images", "Bilder ansehen" }
            }
        },
        // Oben ein grosses Bild, daneben eine breite und zwei kleinere Kacheln.
        Some(b) => {
            let klein: Vec<(usize, Bild)> = b.iter().skip(1).take(3).cloned().enumerate().collect();
            rsx! {
                BildKachel { b: b[0].clone(), klasse: "ort-gross" }
                if !klein.is_empty() {
                    div { class: "ort-bilder-klein",
                        for (i, k, klasse) in klein.into_iter().map(|(i, k)| (i, k, if i == 0 { "ort-breit" } else { "" })) {
                            BildKachel { key: "{i}", b: k, klasse }
                        }
                    }
                }
            }
        }
    };
    rsx! {
        div { class: "ort-bilder", id: "ort-bilder", {inhalt} }
    }
}

#[component]
fn BildKachel(b: Bild, klasse: &'static str) -> Element {
    rsx! {
        a { class: "bild {klasse}", href: "{b.url}", target: "_blank", rel: "noreferrer", title: "{b.titel}",
            span { class: "rahmen", img { src: "{b.bild}", alt: "{b.titel}", decoding: "async" } }
        }
    }
}

/// Die Profile in den sozialen Netzen - runde Zeichen in den Farben der Netze, wie bei Google.
fn netz(n: &str) -> (String, &'static str) {
    let (z, farbe) = match n {
        "X" => ("𝕏", "#000"),
        "Instagram" => ("IG", "linear-gradient(45deg,#f9a825,#e91e63,#7b1fa2)"),
        "Facebook" => ("f", "#1877f2"),
        "YouTube" => ("▶", "#ff0000"),
        "TikTok" => ("♪", "#111"),
        "Twitch" => ("T", "#9146ff"),
        "Pinterest" => ("P", "#e60023"),
        "Tumblr" => ("t", "#36465d"),
        _ => return (n.chars().next().map(String::from).unwrap_or_default(), "var(--flaeche3)"),
    };
    (z.to_string(), farbe)
}

/// Personen: die Profile und der Wikipedia-Text.
#[component]
fn PersonKarte(ort: Value) -> Element {
    let profile: Vec<(String, String, String, &'static str, String)> = liste(&ort, "profile")
        .iter()
        .map(|p| {
            let (z, farbe) = netz(s(p, "netz"));
            (s(p, "url").to_string(), s(p, "netz").to_string(), s(p, "name").to_string(), farbe, z)
        })
        .collect();
    let wiki = s(&ort, "wikiUrl").to_string();
    let extract = s(&ort, "extract").to_string();
    rsx! {
        section { class: "karte person-karte",
            if !profile.is_empty() {
                div { class: "person-profile",
                    h3 { "Profile" }
                    div { class: "profil-liste",
                        for (url, netz, konto, farbe, zeichen) in profile {
                            a { class: "profil", href: "{url}", target: "_blank", rel: "noreferrer",
                                span { class: "profil-zeichen", style: "background:{farbe}", "{zeichen}" }
                                span { class: "profil-name", "{netz}" }
                                span { class: "profil-konto", "@{konto}" }
                            }
                        }
                    }
                }
            }
            div { class: "person-wiki",
                div { class: "person-wiki-kopf",
                    span { class: "person-w", "W" }
                    if wiki.is_empty() {
                        "Wikipedia"
                    } else {
                        Verweis { url: wiki.clone(), text: "Wikipedia", class: "ort-wiki-link" }
                    }
                }
                if !extract.is_empty() {
                    p { class: "ort-wiki-text", "{extract}" }
                }
                if !wiki.is_empty() {
                    Verweis { url: wiki.clone(), text: "Mehr lesen", class: "ort-link person-mehr" }
                }
            }
        }
    }
}

/// Das Wetter in der Leiste: jetzt gross, dazu vier Tage. Ein Klick oeffnet das ganze Wetter.
#[component]
fn OrtWetter(ort: Value) -> Element {
    let (lat, lon) = (zahl_text(&ort["lat"]), zahl_text(&ort["lon"]));
    let wetter = use_resource(move || {
        let (lat, lon) = (lat.clone(), lon.clone());
        async move { api::get_oder_nichts("/api/wetter", &[("lat", &lat), ("lon", &lon)]).await.and_then(|d| d.get("wetter").cloned()) }
    });
    let name = s(&ort, "name").to_string();
    let Some(w) = wetter.read().clone() else {
        return rsx! { div { class: "skelett-rahmen", style: "aspect-ratio:auto;height:100%" } };
    };
    let Some(w) = w.filter(|w| !liste(w, "tage").is_empty()) else {
        return rsx! { div { class: "ort-leer", "Keine Wetterdaten" } };
    };
    let j = &w["aktuell"];
    let code = code_aus(&j["code"]);
    let tage: Vec<Value> = liste(&w, "tage").iter().take(4).cloned().collect();
    let link = Some(s(&w, "link")).filter(|l| !l.is_empty()).unwrap_or("https://open-meteo.com/").to_string();
    let frage = format!("Wetter {name}");
    rsx! {
        div { class: "ow", title: "Ganzes Wetter anzeigen", onclick: move |_| crate::state::suchen(&frage),
            div { class: "ow-jetzt",
                WetterIcon { code, tag: tag_aus(&j["tag"]), groesse: 56 }
                span { class: "ow-temp", "{grad(f(&j[\"temperatur\"]))}" }
                div { class: "ow-text",
                    "{wetter_text(code)}"
                    small { "gefühlt {grad(f(&j[\"gefuehlt\"]))} · {f(&j[\"wind\"]).unwrap_or(0.0).round()} km/h" }
                }
            }
            div { class: "ow-tage",
                for (i, t) in tage.iter().enumerate() {
                    div { key: "{i}",
                        span { "{wochentag(s(t, \"zeit\"), false)}" }
                        WetterIcon { code: code_aus(&t["code"]), tag: true, groesse: 30 }
                        span {
                            b { "{grad(f(&t[\"max\"]))}" }
                            " {grad(f(&t[\"min\"]))}"
                        }
                    }
                }
            }
        }
        div { class: "ow-fuss",
            Verweis { url: link, text: "Open-Meteo", class: "ort-link" }
        }
    }
}

// ---- Die Info rechts ---------------------------------------------------------------------

/// Die Felder der Infobox von SearXNG - sie kommen dazu, wenn Wikidata sie nicht hat.
fn infobox_feld(name: &str) -> Option<&'static str> {
    Some(match name {
        "bevölkerung" | "einwohner" | "einwohnerzahl" => "Einwohnerzahl",
        "fläche" | "fläche (km²)" => "Fläche",
        "postleitzahl" => "Postleitzahl",
        "bundesland" => "Bundesland",
        "land" | "staat" => "Staat",
        "bürgermeister" | "oberbürgermeister" => "Leiter der Regierung oder Verwaltung",
        "landkreis" => "Landkreis",
        "art" => "Art",
        "hauptstadt" => "Hauptstadt",
        "website" => "Offizielle Website",
        _ => return None,
    })
}

/// Ein Fakt: Name und die Teile des Werts (Text, Link).
#[derive(Clone, PartialEq)]
struct Fakt {
    name: String,
    teile: Vec<(String, String)>,
}

/// Die Fakten aus Wikidata, ergaenzt um die Felder der Infobox.
fn fakten(ort: &Value, kasten: Option<&Value>) -> Vec<Fakt> {
    let mut aus: Vec<Fakt> = liste(ort, "fakten")
        .iter()
        .map(|f| {
            // Mehrere Werte (Kinder, Ehepartner …) - jeder mit seinem eigenen Link.
            let teile = liste(f, "teile");
            let teile: Vec<(String, String)> = if teile.is_empty() {
                vec![(text_von(&f["wert"]), s(f, "url").to_string())]
            } else {
                teile.iter().map(|t| (text_von(&t["text"]), s(t, "url").to_string())).collect()
            };
            Fakt { name: s(f, "name").to_string(), teile }
        })
        .collect();
    for feld in kasten.map(|k| liste(k, "infobox")).unwrap_or(&[]) {
        let Some(name) = infobox_feld(&text_von(&feld["name"]).trim().to_lowercase()) else { continue };
        let wert = text_von(&feld["value"]);
        if wert.is_empty() || aus.iter().any(|f| f.name == name) {
            continue;
        }
        aus.push(Fakt { name: name.to_string(), teile: vec![(wert, s(feld, "entity").to_string())] });
    }
    aus
}

/// Die Info rechts neben den Treffern: der Wikipedia-Text, dann die Fakten, der Finanzteil und
/// die Links. Unternehmen wie bei Google mit Logo, Bildern und Quelle.
#[component]
fn InfoRechts(ort: Value, kasten: Option<Value>, laedt: bool, finanz: Option<Value>) -> Element {
    let wiki = Some(s(&ort, "wikiUrl")).filter(|w| !w.is_empty()).map(String::from).unwrap_or_else(|| wikipedia_link(&ort));
    let text = Some(s(&ort, "extract"))
        .filter(|t| !t.is_empty())
        .or_else(|| kasten.as_ref().map(|k| s(k, "content")))
        .unwrap_or("")
        .to_string();
    let fakten = fakten(&ort, kasten.as_ref());
    if text.is_empty() && fakten.is_empty() && !laedt {
        return rsx! {};
    }
    let name = s(&ort, "name").to_string();
    let art = s(&ort, "art").to_string();
    let untertitel = s(&ort, "untertitel").to_string();
    let bild = s(&ort, "bild").to_string();

    // Die Links: Wikipedia, Website, englische Wikipedia, Profile, OSM, Wikidata.
    let mut links: Vec<(String, String)> = vec![
        (wiki.clone(), format!("{name} - Wikipedia")),
        (s(&ort, "website").to_string(), "Offizielle Website".into()),
        (s(&ort, "wikiUrlEn").to_string(), "Wikipedia (en)".into()),
    ];
    links.extend(liste(&ort, "profile").iter().map(|p| (s(p, "url").to_string(), s(p, "netz").to_string())));
    if !s(&ort, "osmTyp").is_empty() && !text_von(&ort["osmId"]).is_empty() {
        links.push((format!("https://www.openstreetmap.org/{}/{}", s(&ort, "osmTyp"), text_von(&ort["osmId"])), "OpenStreetMap".into()));
    }
    if !s(&ort, "wikidata").is_empty() {
        links.push((format!("https://www.wikidata.org/wiki/{}", s(&ort, "wikidata")), "Wikidata".into()));
    }
    links.retain(|(url, _)| !url.is_empty());

    let rest = rsx! {
        if laedt {
            div { class: "fakten-laedt",
                for i in 0..5 {
                    div { key: "{i}", class: "skelett-zeile" }
                }
            }
        }
        if !fakten.is_empty() {
            dl { class: "ort-fakten",
                for (i, fakt) in fakten.into_iter().enumerate() {
                    div { key: "{i}", class: "ort-fakt",
                        dt { "{fakt.name}" }
                        dd {
                            for (j, (t, url)) in fakt.teile.into_iter().enumerate() {
                                if j > 0 {
                                    ", "
                                }
                                if url.is_empty() {
                                    "{t}"
                                } else {
                                    Verweis { url, text: t }
                                }
                            }
                        }
                    }
                }
            }
        }
        div { id: "ort-finanz",
            if let Some(d) = finanz {
                FinanzKarte { daten: d }
            }
        }
        if !links.is_empty() {
            div { class: "ort-links",
                h4 { "Links" }
                for (url, text) in links {
                    Verweis { url, text, class: "ort-link" }
                }
            }
        }
    };

    if art == "unternehmen" {
        let b: Vec<Value> = liste(&ort, "bilder").iter().take(4).cloned().collect();
        return rsx! {
            section { class: "ort-info ort-firma", "aria-label": "Info zu {name}",
                div { class: "firma-kopf",
                    if !bild.is_empty() {
                        img { class: "firma-logo", src: "{bild}", alt: "", loading: "lazy" }
                    }
                    div { class: "firma-titel",
                        h3 { "{name}" }
                        if !untertitel.is_empty() {
                            p { "{untertitel}" }
                        }
                    }
                    button { r#type: "button", class: "firma-menue", "aria-label": "Weitere Angaben", title: "Weitere Angaben", "⋮" }
                }
                div { class: "firma-aktionen",
                    button { r#type: "button", class: "firma-folgen", "Folgen" }
                }
                if !b.is_empty() {
                    div { class: "firma-bilder",
                        FirmaBild { b: b[0].clone(), klasse: " gross" }
                        div { class: "firma-bilder-klein",
                            if let Some(x) = b.get(1) {
                                FirmaBild { b: x.clone(), klasse: "" }
                            }
                            if let Some(x) = b.get(2) {
                                FirmaBild { b: x.clone(), klasse: "" }
                            }
                            if let Some(x) = b.get(3) {
                                FirmaBild { b: x.clone(), klasse: " breit" }
                            }
                        }
                    }
                }
                if !text.is_empty() {
                    p { class: "ort-text firma-text", "{text}" }
                }
                if !wiki.is_empty() {
                    div { class: "firma-quelle",
                        "Quelle: "
                        Verweis { url: wiki.clone(), text: "Wikipedia" }
                    }
                }
                {rest}
            }
        };
    }

    let wappen = kasten.as_ref().map(|k| s(k, "img_src").to_string()).unwrap_or_default();
    rsx! {
        section { class: "ort-info", "aria-label": "Info zu {name}",
            if art == "begriff" {
                div { class: "begriff-kopf",
                    if !bild.is_empty() {
                        BegriffBild { src: bild.clone() }
                    }
                    h3 { "{name}" }
                    if !untertitel.is_empty() {
                        p { "{untertitel}" }
                    }
                }
            } else {
                h3 { "Info" }
            }
            if !text.is_empty() {
                p { class: "ort-text",
                    if !wappen.is_empty() {
                        img { class: "ort-wappen", src: "{wappen}", alt: "", loading: "lazy" }
                    }
                    "{text}"
                    if !wiki.is_empty() {
                        " "
                        Verweis { url: wiki.clone(), text: "Wikipedia" }
                    }
                }
            }
            {rest}
        }
    }
}

/// Das grosse Bild bei Begriffen - schimmert, bis es geladen ist.
#[component]
fn BegriffBild(src: String) -> Element {
    let mut da = use_signal(|| false);
    rsx! {
        div { class: if da() { "begriff-bild" } else { "begriff-bild schimmer" },
            img { src: "{src}", alt: "", onload: move |_| da.set(true) }
        }
    }
}

#[component]
fn FirmaBild(b: Value, klasse: &'static str) -> Element {
    let url = s(&b, "url").to_string();
    let ziel = Some(s(&b, "voll")).filter(|v| !v.is_empty()).unwrap_or(&url).to_string();
    let titel = s(&b, "titel").to_string();
    rsx! {
        a { class: "firma-bild schimmer{klasse}", href: "{ziel}", target: "_blank", rel: "noreferrer",
            img { src: "{url}", alt: "{titel}", loading: "lazy" }
        }
    }
}

/// Der Kurs eines Unternehmens rechts in der Info (mit dem Verlauf des Tages).
#[component]
fn FinanzKarte(daten: Value) -> Element {
    let symbol = s(&daten, "symbol").to_string();
    let kurs = f(&daten["kurs"]).unwrap_or(0.0);
    let stellen = super::kurs::kurs_stellen(kurs, &symbol);
    let vorher = f(&daten["vorher"]).filter(|v| *v != 0.0);
    let waehrung = s(&daten, "waehrung").to_string();
    let link = s(&daten, "link").to_string();
    let quelle = s(&daten, "quelle").to_string();
    rsx! {
        div { class: "ort-finanz-karte",
            div { class: "of-kopf",
                span { class: "of-symbol", "{symbol}" }
                span { class: "of-wert", "{zahl(kurs, stellen)} {waehrung}" }
            }
            if let Some(v) = vorher {
                {
                    let p = (kurs / v - 1.0) * 100.0;
                    let hoch = p >= 0.0;
                    let plus = if hoch { "+" } else { "" };
                    rsx! {
                        div { class: if hoch { "of-anderung hoch" } else { "of-anderung runter" },
                            span { class: "of-pfeil", "aria-hidden": "true", if hoch { "▲" } else { "▼" } }
                            " {plus}{zahl(p, 2)} %"
                        }
                    }
                }
            }
            div { class: "of-chart",
                super::kurs::KursChart { daten: daten.clone() }
            }
            div { class: "of-links",
                if !link.is_empty() {
                    Verweis { url: link, text: "Quelle: {quelle}" }
                }
            }
        }
    }
}

const CSS: &str = r#"
.schimmer { background: var(--flaeche2) linear-gradient(90deg, transparent, rgba(255, 255, 255, .06), transparent) no-repeat !important; background-size: 220% 100% !important; animation: schimmern 1.4s ease-in-out infinite; }
.ort-bilder .bild .rahmen.schimmer { display: block; height: 100%; }
.fakten-laedt { border: 1px solid var(--kante); border-radius: var(--r); padding: 6px 14px 14px; }
.fakten-laedt .skelett-zeile:nth-child(odd) { width: 70%; }
.begriff-bild { border-radius: var(--r); overflow: hidden; margin-bottom: 12px; min-height: 160px; }
.begriff-bild img { display: block; width: 100%; max-height: 240px; object-fit: cover; margin: 0 !important; }
.ow { cursor: pointer; }
.ow-jetzt { display: flex; align-items: center; gap: 6px; }
.ow-temp { font-size: 40px; line-height: 1; }
.ow-text { display: flex; flex-direction: column; margin-left: 6px; font-size: 14px; }
.ow-text small { color: var(--gedimmt); font-size: 12px; }
.ow-tage { display: grid; grid-template-columns: repeat(4, 1fr); gap: 2px; margin-top: 10px; text-align: center; font-size: 12px; color: var(--gedimmt); }
.ow-tage > div { display: flex; flex-direction: column; align-items: center; gap: 1px; padding: 4px 0; border-radius: var(--r); }
.ow:hover .ow-tage > div:first-child, .ow-tage > div:hover { background: var(--flaeche2); }
.ow-tage b { font-weight: 400; color: var(--schrift); }
.ow-fuss { margin-top: 6px; text-align: right; }
.ort-wetter { padding: 14px 16px !important; }
/* ---- Ortskarte (Städte, Länder, Sehenswürdigkeiten) --------------------- */
.ort { width: 100%; margin-bottom: 2rem; padding-bottom: 1.75rem; border-bottom: 1px solid var(--kante); }
.ort-kopf { display: flex; align-items: flex-start; gap: .75rem; margin-bottom: 1.1rem; position: relative; }
.ort-namen { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.ort-titel { margin: 0; font-size: 2rem; font-weight: 400; color: var(--schrift); line-height: 1.2; }
.ort-untertitel { font-size: 14px; color: var(--gedimmt); }
.ort-menue-knopf { margin-top: 6px; border: 0; background: transparent; color: var(--gedimmt); font-size: 1.4rem; line-height: 1; padding: 0 .3rem; border-radius: var(--r2); cursor: pointer; }
.ort-menue-knopf:hover { background: var(--flaeche3); color: var(--schrift); }
.ort-menue-liste { position: absolute; left: 0; top: 130%; z-index: 25; min-width: 15rem; padding: .35rem; background: var(--menue); border: 1px solid var(--kante); border-radius: var(--r); box-shadow: var(--schatten); }
.ort-menue-liste[hidden] { display: none; }
.ort-menue-liste a, .ort-menue-liste button { display: block; width: 100%; text-align: left; padding: .5rem .7rem; font: inherit; font-size: .88rem; color: var(--schrift); text-decoration: none; background: none; border: 0; border-radius: var(--r2); cursor: pointer; }
.ort-menue-liste a:hover, .ort-menue-liste button:hover { background: var(--flaeche3); }
/* Die Leiste: Bilder | Karte | Wetter */
.ort-rumpf { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(0, 1fr) 17rem; gap: 1.25rem; align-items: stretch; }
.ort-bilder { display: grid; grid-template-columns: 1.55fr 1fr; grid-template-rows: minmax(0, 1fr); gap: .4rem; height: 17rem; }
/* Personen: Bilder links, rechts eine Karte mit Profilen und Wikipedia. */
.ort-person .ort-rumpf { grid-template-columns: minmax(0, 1.45fr) minmax(0, 1fr); }
.ort-person .ort-bilder { height: auto; min-height: 18rem; contain: size; }
.ort-person .ort-bilder .bild, .ort-person .ort-bilder .rahmen { border-radius: 12px; }
.person-karte { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px !important; height: 100%; }
.person-profile h3 { margin: 0 0 10px !important; }
.profil-liste { display: flex; flex-wrap: wrap; gap: 14px 18px; }
.profil { display: flex; flex-direction: column; align-items: center; gap: 4px; width: 72px; text-align: center; text-decoration: none !important; }
.profil-zeichen { width: 44px; height: 44px; border-radius: 50%; display: grid; place-items: center; color: #fff; font: 600 17px Arial; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, .15); }
.profil:hover .profil-zeichen { filter: brightness(1.15); }
.profil-name { font-size: 13px; color: var(--schrift); }
.profil-konto { font-size: 11px; color: var(--gedimmt); max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.person-profile + .person-wiki { border-top: 1px solid var(--kante); padding-top: 14px; }
.person-wiki { flex: 1; display: flex; flex-direction: column; }
.person-wiki-kopf { display: flex; align-items: center; gap: 8px; font-size: 15px; }
.person-w { width: 24px; height: 24px; border-radius: 50%; background: #fff; color: #000; display: grid; place-items: center; font: 400 15px "Times New Roman", serif; }
.person-wiki .ort-wiki-text { -webkit-line-clamp: 7; font-size: 14px; line-height: 1.55; margin-top: 8px; }
.person-mehr { margin-top: auto; padding-top: 8px; }
.ort-bilder-klein { display: grid; grid-template-columns: 1fr 1fr; grid-template-rows: 1.2fr 1fr; gap: .4rem; min-height: 0; }
.ort-bilder .bild { height: 100%; min-height: 0; }
.ort-bilder .rahmen { aspect-ratio: auto; height: 100%; }
.ort-bilder .rahmen img { width: 100%; height: 100%; object-fit: cover; display: block; }
.ort-bilder .ort-breit { grid-column: span 2; }
.ort-karte { height: 17rem; border: 1px solid var(--kante); border-radius: var(--r); overflow: hidden; isolation: isolate; position: relative; z-index: 0; }
.ort-map { width: 100%; height: 100%; background: var(--flaeche2); }
.ort-karte-hinweis { padding: 1rem; color: var(--gedimmt); font-size: .85rem; }
.ort-karte-hinweis a { color: var(--hoch); }
.ort-punkt { background: var(--hoch); border: 2px solid #fff; border-radius: 50%; box-shadow: 0 0 6px rgba(0, 0, 0, .5); }
/* Wetter rechts in der Leiste */
.ort-spalte { display: flex; flex-direction: column; gap: .75rem; }
.ort-spalte .karte { margin: 0; }
.ort-spalte .karte h3, .ort-wiki h3 { margin: 0 0 .6rem; font-size: 1rem; font-weight: 500; color: var(--schrift); }
.ort-leer { color: var(--gedimmt); font-size: .85rem; }
.ort-link { font-size: .8rem; color: var(--hoch); text-decoration: none; }
.ort-link:hover { text-decoration: underline; }
.ort-wiki-link { font-size: .9rem; color: var(--hoch); text-decoration: none; }
.ort-wiki-link:hover { text-decoration: underline; }
.ort-wiki-text { margin: .4rem 0 0; font-size: .82rem; line-height: 1.5; color: var(--snippet); display: -webkit-box; -webkit-line-clamp: 5; -webkit-box-orient: vertical; overflow: hidden; }
/* Die Nachrichten zu einer Person. */
.ort-news { margin-bottom: 2rem; padding-bottom: 1rem; border-bottom: 1px solid var(--kante); }
.ort-news h3 { margin: 0 0 1rem; font-size: 1.25rem; font-weight: 400; color: var(--schrift); }
.ort-news .news { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 1.25rem 1.75rem; }
@media (max-width: 900px) { .ort-news .news { grid-template-columns: minmax(0, 1fr); } }
@media (max-width: 1160px) {
  .ort-rumpf { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .ort-rumpf > .ort-spalte { grid-column: 1 / -1; }
  .ort-person .ort-rumpf > .ort-spalte { grid-column: auto; }
}
@media (max-width: 760px) {
  .ort-rumpf, .ort-person .ort-rumpf { grid-template-columns: minmax(0, 1fr); }
  .ort-person .ort-rumpf > .ort-spalte { grid-column: 1; }
  .ort-bilder, .ort-karte { height: 14rem; }
  .ort-person .ort-bilder { min-height: 14rem; }
  .ort-titel { font-size: 1.6rem; }
}
/* ---- Links und Finanzteil (bei Unternehmen) ---------------------------- */
.ort-links { margin-top: 1rem; padding-top: .85rem; border-top: 1px solid var(--kante); }
.ort-links h4 { margin: 0 0 .5rem; font-size: .95rem; font-weight: 500; color: var(--schrift); }
.ort-links .ort-link { display: block; margin-bottom: .3rem; }
.ort-finanz-karte { margin-top: 1rem; padding: .7rem .85rem; border: 1px solid var(--kante); border-radius: var(--r); background: var(--flaeche2); }
.of-kopf { display: flex; align-items: baseline; gap: .6rem; }
.of-symbol { font-size: .78rem; color: var(--gedimmt); border: 1px solid var(--kante); border-radius: var(--r2); padding: .05rem .35rem; }
.of-wert { font-size: 1.3rem; color: var(--schrift); }
.of-anderung { margin-top: .2rem; font-size: .85rem; }
.of-anderung.hoch { color: var(--gruen); }
.of-anderung.runter { color: var(--rot); }
.of-pfeil { font-size: .75rem; }
.of-chart { margin-top: .5rem; }
.of-chart:empty { display: none; }
.of-links { margin-top: .35rem; font-size: .75rem; color: var(--gedimmt); }
.of-links a { color: var(--hoch); text-decoration: none; }
.of-links a:hover { text-decoration: underline; }
/* ---- Info-Teil rechts neben den Treffern --------------------------------- */
.ort-info { border-bottom: 1px solid var(--kante); padding-bottom: 1rem; margin-bottom: 1rem; }
.ort-info h3 { margin: 0 0 .6rem; font-size: 1.15rem; font-weight: 400; color: var(--schrift); }
.ort-text { margin: 0 0 .9rem; font-size: .92rem; line-height: 1.6; color: var(--snippet); }
.ort-wappen { float: right; max-width: 7rem; margin: 0 0 .5rem .8rem; border-radius: var(--r2); outline: 1px solid var(--ring); }
.ort-text a { color: var(--hoch); text-decoration: none; }
.ort-text a:hover { text-decoration: underline; }
.ort-fakten { margin: 0; border: 1px solid var(--kante); border-radius: var(--r); overflow: hidden; }
.ort-fakt { display: grid; grid-template-columns: 40% 1fr; gap: .6rem; padding: .55rem .8rem; border-bottom: 1px solid var(--ring); }
.ort-fakt:last-child { border-bottom: 0; }
.ort-fakt dt { margin: 0; font-size: .88rem; color: var(--gedimmt); }
.ort-fakt dd { margin: 0; font-size: .92rem; color: var(--schrift); }
.ort-fakt dd a { color: var(--hoch); text-decoration: none; }
.ort-fakt dd a:hover { text-decoration: underline; }
/* ---- Unternehmen: die Ansicht wie bei Google ------------------------------ */
.ort-firma { border-bottom: 0; padding-bottom: 0; }
.firma-kopf { display: flex; align-items: center; gap: .8rem; margin-bottom: 1rem; }
.firma-logo { width: 2.6rem; height: 2.6rem; border-radius: var(--r2); object-fit: contain; background: var(--flaeche2); padding: .2rem; flex: none; }
.firma-titel { flex: 1; min-width: 0; }
.firma-titel h3 { margin: 0; font-size: 1.3rem; font-weight: 400; color: var(--schrift); }
.firma-titel p { margin: .1rem 0 0; font-size: .88rem; color: var(--gedimmt); }
.firma-menue { flex: none; width: 2rem; height: 2rem; border: 0; border-radius: 50%; background: transparent; color: var(--gedimmt); font-size: 1.2rem; line-height: 1; cursor: pointer; }
.firma-menue:hover { background: var(--flaeche2); color: var(--schrift); }
.firma-folgen { flex: none; border: 1px solid var(--kante); border-radius: var(--r2); background: var(--flaeche2); color: var(--schrift); font: inherit; font-size: .85rem; padding: .35rem .8rem; cursor: pointer; }
.firma-folgen:hover { background: var(--flaeche3); }
.firma-aktionen { display: flex; justify-content: flex-end; gap: .5rem; margin: -.6rem 0 1rem; }
.firma-bilder { display: grid; grid-template-columns: 2fr 1fr; grid-template-rows: 1fr; gap: .3rem; height: 11rem; border-radius: var(--r); overflow: hidden; margin-bottom: 1rem; }
.firma-bilder-klein { display: grid; grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; grid-auto-rows: 1fr; gap: .3rem; min-height: 0; }
.firma-bild { position: relative; display: block; overflow: hidden; min-height: 0; background: var(--flaeche2); }
.firma-bild img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; display: block; }
.firma-bild.breit { grid-column: 1 / -1; }
.firma-text { margin-bottom: .5rem; }
.firma-quelle { font-size: .82rem; color: var(--gedimmt); margin-bottom: 1rem; }
.firma-quelle a { color: var(--hoch); text-decoration: none; }
.firma-quelle a:hover { text-decoration: underline; }
.begriff-kopf { margin-bottom: 12px; }
.begriff-kopf img { width: 100%; max-height: 220px; object-fit: cover; border-radius: var(--r); margin-bottom: 12px; }
.begriff-kopf h3 { font-size: 28px !important; margin: 0 !important; }
.begriff-kopf p { margin: 2px 0 0; color: var(--gedimmt); }
"#;
