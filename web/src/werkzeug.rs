//! Werkzeuge fuer den Kern und die Module (Nachfolger von public/werkzeug.js).
//!
//! Komponenten: [`Kopf`] (Widget-Kopf), [`FaviconKreis`], [`Fett`] (Suchwoerter fett),
//! [`KopierKnopf`], [`NewsListe`].
//! Hooks: [`use_intervall`], [`use_tasten`], [`use_animation`].
//! Funktionen: [`zahl`], [`datum_text`], [`quelle_name`], [`kopieren`], [`ton`], [`zufall`],
//! [`speicher_lesen`]/[`speicher_schreiben`], [`frage_url`], [`re_treffer`].

use dioxus::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure};

// ---- Speicher (localStorage, JSON) -------------------------------------------------------

fn speicher() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// Ein Wert aus dem localStorage (als JSON gelesen; kein JSON -> als Text).
pub fn speicher_lesen(schluessel: &str) -> Option<Value> {
    let roh = speicher()?.get_item(schluessel).ok()??;
    Some(serde_json::from_str(&roh).unwrap_or(Value::String(roh)))
}

/// Einen Wert als JSON in den localStorage schreiben (privater Modus: still nichts).
pub fn speicher_schreiben(schluessel: &str, wert: &Value) {
    if let Some(s) = speicher() {
        let _ = s.set_item(schluessel, &wert.to_string());
    }
}

#[allow(dead_code)]
pub fn speicher_loeschen(schluessel: &str) {
    if let Some(s) = speicher() {
        let _ = s.remove_item(schluessel);
    }
}

// ---- Zahlen und Daten --------------------------------------------------------------------

/// Eine Zahl in deutscher Schreibweise mit `stellen` Nachkommastellen ("1.234,50"). NaN -> "-".
pub fn zahl(n: f64, stellen: usize) -> String {
    if !n.is_finite() {
        return "-".into();
    }
    let roh = format!("{:.*}", stellen, n.abs());
    let (ganz, rest) = roh.split_once('.').unwrap_or((&roh, ""));
    let mut aus = String::with_capacity(roh.len() + 4);
    for (i, c) in ganz.chars().enumerate() {
        if i > 0 && (ganz.len() - i) % 3 == 0 {
            aus.push('.');
        }
        aus.push(c);
    }
    if !rest.is_empty() {
        aus.push(',');
        aus.push_str(rest);
    }
    let null = aus.chars().all(|c| c == '0' || c == '.' || c == ',');
    if n < 0.0 && !null { format!("-{aus}") } else { aus }
}

/// Jetzt in Millisekunden seit 1970.
pub fn jetzt_ms() -> f64 {
    js_sys::Date::now()
}

/// Wie bei Google: Frisches relativ ("vor 3 Std."), Aelteres mit Datum ("12. Okt. 2026").
pub fn datum_text(wann: &str) -> String {
    if wann.is_empty() {
        return String::new();
    }
    let d = js_sys::Date::new(&wann.into());
    let t = d.get_time();
    if t.is_nan() {
        return String::new();
    }
    let min = ((jetzt_ms() - t) / 60000.0).round();
    if !(0.0..7.0 * 1440.0).contains(&min) {
        const MONATE: [&str; 12] = ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."];
        return format!("{}. {} {}", d.get_date(), MONATE[d.get_month() as usize % 12], d.get_full_year());
    }
    if min < 60.0 {
        return format!("vor {} Min.", min.max(1.0));
    }
    if min < 1440.0 {
        return format!("vor {} Std.", (min / 60.0).floor());
    }
    let tage = (min / 1440.0).floor();
    if tage == 1.0 { "vor 1 Tag".into() } else { format!("vor {tage} Tagen") }
}

/// Der Name einer Quelle wie bei Google: "YouTube" statt "www.youtube.com".
pub fn quelle_name(host: &str) -> String {
    let h = host.trim_start_matches("www.").trim_start_matches("m.");
    let basis = h.rsplitn(3, '.').take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(".");
    let name = |x: &str| match x {
        "youtube.com" | "youtu.be" => Some("YouTube"),
        "reddit.com" => Some("Reddit"),
        "wikipedia.org" => Some("Wikipedia"),
        "fandom.com" => Some("Fandom"),
        _ => None,
    };
    name(h).or_else(|| name(&basis)).map_or_else(|| h.to_string(), String::from)
}

/// Die Adresse einer Suche nach `q` (fuer Links, die eine neue Suche starten).
pub fn frage_url(q: &str) -> String {
    format!("/?q={}", urlencoding::encode(q))
}

/// Ein JS-Regex (Browser-Engine, Flags z. B. "iu") auf `text`: die Gruppen (0 = alles) oder None.
/// So bleibt der Regex-Code aus dem WASM (kein `regex`-Crate).
pub fn re_treffer(muster: &str, flags: &str, text: &str) -> Option<Vec<Option<String>>> {
    let re = js_sys::RegExp::new(muster, flags);
    let m = re.exec(text)?;
    Some((0..m.length()).map(|i| m.get(i).as_string()).collect())
}

// ---- Zwischenablage, Ton, Zufall ---------------------------------------------------------

/// Text in die Zwischenablage (ohne HTTPS ueber ein unsichtbares Feld). true = geklappt.
pub async fn kopieren(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let Some(fenster) = web_sys::window() else { return false };
    let ablage = fenster.navigator().clipboard();
    if wasm_bindgen_futures::JsFuture::from(ablage.write_text(text)).await.is_ok() {
        return true;
    }
    // Der alte Weg: execCommand('copy') auf einem unsichtbaren Feld.
    let Some(doc) = fenster.document() else { return false };
    let Ok(feld) = doc.create_element("textarea") else { return false };
    let feld: web_sys::HtmlTextAreaElement = feld.unchecked_into();
    feld.set_value(text);
    let _ = feld.set_attribute("style", "position:fixed;opacity:0;top:0;left:0");
    let _ = doc.body().map(|b| b.append_child(&feld));
    feld.select();
    let html: web_sys::HtmlDocument = doc.clone().unchecked_into();
    let ok = html.exec_command("copy").unwrap_or(false);
    feld.remove();
    ok
}

thread_local! {
    static AUDIO: RefCell<Option<web_sys::AudioContext>> = const { RefCell::new(None) };
}

/// Ein kurzer Ton (Timer, Metronom) - ohne Audiodatei. `mal` Wiederholungen.
pub fn ton(hz: f32, dauer: f64, mal: u32) {
    AUDIO.with(|a| {
        let mut a = a.borrow_mut();
        if a.is_none() {
            *a = web_sys::AudioContext::new().ok();
        }
        let Some(ctx) = a.as_ref() else { return };
        for i in 0..mal {
            let start = ctx.current_time() + f64::from(i) * (dauer + 0.15);
            let (Ok(osz), Ok(laut)) = (ctx.create_oscillator(), ctx.create_gain()) else { return };
            osz.frequency().set_value(hz);
            let _ = laut.gain().set_value_at_time(0.2, start);
            let _ = laut.gain().exponential_ramp_to_value_at_time(0.001, start + dauer);
            let _ = osz.connect_with_audio_node(&laut).and_then(|_| laut.connect_with_audio_node(&ctx.destination()));
            let _ = osz.start_with_when(start);
            let _ = osz.stop_with_when(start + dauer);
        }
    });
}

/// Gleichverteilter Zufall 0..n aus der Krypto-Schnittstelle (Passwoerter, Wuerfel, Muenze).
pub fn zufall(n: u32) -> u32 {
    if n <= 1 {
        return 0;
    }
    let grenze = u32::MAX - (u32::MAX % n);
    let krypto = web_sys::window().and_then(|w| w.crypto().ok());
    loop {
        let mut feld = [0u8; 4];
        match &krypto {
            Some(k) => {
                let _ = k.get_random_values_with_u8_array(&mut feld);
            }
            None => feld = ((js_sys::Math::random() * f64::from(u32::MAX)) as u32).to_le_bytes(),
        }
        let x = u32::from_le_bytes(feld);
        if x < grenze {
            return x % n;
        }
    }
}

// ---- Hooks -------------------------------------------------------------------------------

/// Macht aus `f` eine Funktion, die auch aus Browser-Callbacks (web-sys-Closures, EventSource …)
/// heraus GlobalSignals lesen/schreiben und `spawn` aufrufen darf - ohne Runtime gibt Dioxus dort
/// sonst einen Panic. In einer Komponente (oder einem ihrer Hooks/Effekte) aufrufen.
pub fn im_scope<A: 'static>(f: impl FnMut(A) + 'static) -> impl Fn(A) + Clone + 'static {
    let rt = dioxus::core::Runtime::current();
    let scope = dioxus::core::current_scope_id();
    let f = Rc::new(RefCell::new(f));
    move |a: A| {
        let _w = dioxus::core::RuntimeGuard::new(rt.clone());
        rt.in_scope(scope, || {
            if let Ok(mut f) = f.try_borrow_mut() {
                f(a);
            }
        });
    }
}

/// Alle `ms` Millisekunden `f` aufrufen, solange die Komponente steht (immer die neueste
/// Fassung von `f`). Signale sind `Copy` - einfach in die Closure nehmen.
pub fn use_intervall(ms: u32, f: impl FnMut() + 'static) {
    let zelle = use_hook(|| Rc::new(RefCell::new(None::<Box<dyn FnMut()>>)));
    *zelle.borrow_mut() = Some(Box::new(f));
    let z = zelle.clone();
    use_future(move || {
        let z = z.clone();
        async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(ms).await;
                if let Some(f) = z.borrow_mut().as_mut() {
                    f();
                }
            }
        }
    });
}

/// Tasten fuer ein Widget (Spiele): nur solange es auf der Seite steht und kein Eingabefeld
/// den Fokus hat. `f` bekommt das KeyboardEvent (`e.key()`, `e.prevent_default()`).
pub fn use_tasten(f: impl FnMut(web_sys::KeyboardEvent) + 'static) {
    let zelle = use_hook(|| Rc::new(RefCell::new(None::<Box<dyn FnMut(web_sys::KeyboardEvent)>>)));
    *zelle.borrow_mut() = Some(Box::new(f));
    let hoerer = use_hook(|| {
        let z = zelle.clone();
        let aufruf = im_scope(move |e: web_sys::KeyboardEvent| {
            if let Some(f) = z.borrow_mut().as_mut() {
                f(e);
            }
        });
        let c = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
            let in_feld = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .and_then(|el| el.closest("input, textarea, select").ok().flatten())
                .is_some();
            if in_feld {
                return;
            }
            aufruf(e);
        });
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let _ = doc.add_event_listener_with_callback("keydown", c.as_ref().unchecked_ref());
        }
        Rc::new(c)
    });
    use_drop(move || {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let _ = doc.remove_event_listener_with_callback("keydown", (*hoerer).as_ref().unchecked_ref());
        }
    });
}

/// Eine Bildschleife (Spiele, Animationen) mit requestAnimationFrame, solange die Komponente
/// steht: `f(dt_ms, jetzt_ms)`.
#[allow(dead_code)]
pub fn use_animation(f: impl FnMut(f64, f64) + 'static) {
    let zelle = use_hook(|| Rc::new(RefCell::new(None::<Box<dyn FnMut(f64, f64)>>)));
    *zelle.borrow_mut() = Some(Box::new(f));
    let laeuft = use_hook(|| {
        let laeuft = Rc::new(std::cell::Cell::new(true));
        let schleife: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
        let (s2, l2, z) = (schleife.clone(), laeuft.clone(), zelle.clone());
        let aufruf = im_scope(move |(dt, jetzt): (f64, f64)| {
            if let Some(f) = z.borrow_mut().as_mut() {
                f(dt, jetzt);
            }
        });
        let vorher = Rc::new(std::cell::Cell::new(-1.0f64));
        *schleife.borrow_mut() = Some(Closure::new(move |jetzt: f64| {
            if !l2.get() {
                s2.borrow_mut().take();
                return;
            }
            let dt = if vorher.get() < 0.0 { 0.0 } else { jetzt - vorher.get() };
            vorher.set(jetzt);
            aufruf((dt, jetzt));
            if let (Some(w), Some(c)) = (web_sys::window(), s2.borrow().as_ref()) {
                let _ = w.request_animation_frame(c.as_ref().unchecked_ref());
            }
        }));
        if let (Some(w), Some(c)) = (web_sys::window(), schleife.borrow().as_ref()) {
            let _ = w.request_animation_frame(c.as_ref().unchecked_ref());
        }
        laeuft
    });
    use_drop(move || laeuft.set(false));
}

// ---- Komponenten -------------------------------------------------------------------------

/// Der Kopf eines Widgets: Titel links, Zusatz rechts (`.w-kopf`).
#[component]
pub fn Kopf(titel: String, rechts: Option<Element>) -> Element {
    rsx! {
        div { class: "w-kopf",
            span { class: "w-titel", "{titel}" }
            if let Some(r) = rechts {
                span { class: "w-rechts", {r} }
            }
        }
    }
}

/// Das Favicon im Kreis - faellt es aus, bleibt der erste Buchstabe stehen.
#[component]
pub fn FaviconKreis(host: String) -> Element {
    let buchstabe = host.chars().next().unwrap_or('?').to_string();
    let mut kaputt = use_signal(|| false);
    rsx! {
        span { class: "favicon",
            if !host.is_empty() && !kaputt() {
                img {
                    src: "https://icons.duckduckgo.com/ip3/{host}.ico",
                    alt: "",
                    loading: "lazy",
                    onerror: move |_| kaputt.set(true),
                }
            }
            span { class: "buchstabe", "{buchstabe}" }
        }
    }
}

/// Text mit fett gesetzten Suchwoertern (ab drei Zeichen, ohne Gross/klein).
#[component]
pub fn Fett(text: String, frage: String) -> Element {
    let teile = fett_teile(&text, &frage);
    rsx! {
        for (i, (stueck, fett)) in teile.into_iter().enumerate() {
            if fett {
                b { key: "{i}", "{stueck}" }
            } else {
                span { key: "{i}", "{stueck}" }
            }
        }
    }
}

/// Zerlegt `text` in (Stueck, fett?) - fett, wo ein Suchwort (>= 3 Zeichen) steht.
pub fn fett_teile(text: &str, frage: &str) -> Vec<(String, bool)> {
    let woerter: Vec<String> = frage.split_whitespace().filter(|w| w.chars().count() >= 3).map(str::to_lowercase).collect();
    if woerter.is_empty() || text.is_empty() {
        return vec![(text.to_string(), false)];
    }
    let klein: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let zeichen: Vec<char> = text.chars().collect();
    // Nur wenn die Kleinschreibung gleich lang ist, lassen sich Positionen uebertragen.
    if klein.len() != zeichen.len() {
        return vec![(text.to_string(), false)];
    }
    let mut fett = vec![false; zeichen.len()];
    for w in &woerter {
        let w: Vec<char> = w.chars().collect();
        let mut i = 0;
        while i + w.len() <= klein.len() {
            if klein[i..i + w.len()] == w[..] {
                fett[i..i + w.len()].iter_mut().for_each(|f| *f = true);
                i += w.len();
            } else {
                i += 1;
            }
        }
    }
    let mut aus: Vec<(String, bool)> = Vec::new();
    for (c, f) in zeichen.into_iter().zip(fett) {
        match aus.last_mut() {
            Some((s, g)) if *g == f => s.push(c),
            _ => aus.push((c.to_string(), f)),
        }
    }
    aus
}

/// Ein Knopf, der `text` kopiert und kurz "Kopiert ✓" zeigt.
#[component]
pub fn KopierKnopf(text: String, #[props(default = "Kopieren".to_string())] beschriftung: String, #[props(default)] class: String) -> Element {
    let mut kopiert = use_signal(|| false);
    rsx! {
        button {
            r#type: "button",
            class: "{class}",
            onclick: move |_| {
                let text = text.clone();
                spawn(async move {
                    if kopieren(&text).await {
                        kopiert.set(true);
                        gloo_timers::future::TimeoutFuture::new(1300).await;
                        kopiert.set(false);
                    }
                });
            },
            if kopiert() { "Kopiert ✓" } else { "{beschriftung}" }
        }
    }
}

/// Eine Trefferliste im Nachrichten-Stil (Info-Karte, Reiter "News").
#[component]
pub fn NewsListe(treffer: Vec<crate::state::Treffer>, frage: String) -> Element {
    rsx! {
        div { class: "news",
            for t in treffer {
                article { key: "{t.url}",
                    div { class: "innen",
                        div { class: "quelle-zeile",
                            FaviconKreis { host: t.host.clone() }
                            "{t.host}"
                            if let Some(d) = t.datum.as_deref().filter(|d| !d.is_empty()) {
                                " · {datum_text(d)}"
                            }
                        }
                        h3 {
                            a { href: "{t.url}", target: "_blank", rel: "noreferrer",
                                Fett { text: t.titel.clone(), frage: frage.clone() }
                            }
                        }
                        p { Fett { text: t.text.clone(), frage: frage.clone() } }
                    }
                    if !t.bild.is_empty() {
                        a { class: "bild", href: "{t.url}", target: "_blank", rel: "noreferrer",
                            span { class: "rahmen", img { src: "{t.bild}", alt: "", loading: "lazy" } }
                        }
                    }
                }
            }
        }
    }
}

/// Ein CSS-Block fuer ein Modul (einmal je Modul in die Seite; bleibt beim Neuaufbau stehen).
#[component]
pub fn ModulCss(name: String, css: &'static str) -> Element {
    rsx! { document::Style { "data-modul": "{name}", {css} } }
}
