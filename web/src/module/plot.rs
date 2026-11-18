//! Modul "plot": "plot y=x^2", "graph sin(x)", "f(x)=x^3-2x" - zeichnet die Funktion als Graph
//! auf ein Canvas (web-sys). Ziehen verschiebt, das Mausrad zoomt.

use super::ModulCtx;
use crate::bib::mathe::{Formel, Werte, auf_stellen, js_zahl, uebersetzen};
use crate::werkzeug::{Kopf, ModulCss};
use dioxus::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

const FARBEN: [&str; 4] = ["#4285f4", "#ea4335", "#fbbc04", "#34a853"];

/// Mitte (x, y) und sichtbare Breite in x.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sicht {
    x: f64,
    y: f64,
    breite: f64,
}

const START: Sicht = Sicht { x: 0.0, y: 0.0, breite: 20.0 };

/// "y = " bzw. "f(x) = " vorne weg.
fn ohne_prefix(t: &str) -> &str {
    let s = t.trim_start();
    let klein = s.to_ascii_lowercase();
    let rest = if klein.starts_with("f(x)") {
        &s[4..]
    } else if klein.starts_with('y') {
        &s[1..]
    } else {
        return t;
    };
    match rest.trim_start().strip_prefix('=') {
        Some(r) => r.trim_start(),
        None => t,
    }
}

/// Teilt an "," und ";" - aber nicht innerhalb von Klammern (wie `(?![^(]*\))`).
fn teilen(t: &str) -> Vec<String> {
    let z: Vec<char> = t.chars().collect();
    let mut teile = Vec::new();
    let mut akt = String::new();
    for (i, &c) in z.iter().enumerate() {
        if c == ',' || c == ';' {
            let in_klammer = z[i + 1..].iter().find(|&&d| d == '(' || d == ')') == Some(&')');
            if !in_klammer {
                teile.push(std::mem::take(&mut akt));
                continue;
            }
        }
        akt.push(c);
    }
    teile.push(akt);
    teile
}

// Mehrere Funktionen mit Komma: "plot sin(x), cos(x)".
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let roh = ctx.gruppen.get(1)?.as_deref()?;
    let teile: Vec<String> = teilen(ohne_prefix(roh)).iter().map(|t| ohne_prefix(t.trim()).trim().to_string()).filter(|t| !t.is_empty()).collect();
    for t in &teile {
        uebersetzen(t).ok()?;
    }
    Some(Value::from(teile))
}

fn zeichnen(canvas: &HtmlCanvasElement, sicht: Sicht, funktionen: &[Formel]) {
    let Some(fenster) = web_sys::window() else { return };
    let Some(ctx) = canvas.get_context("2d").ok().flatten().and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok()) else { return };
    let dpr = fenster.device_pixel_ratio().max(1.0);
    let b = f64::from(canvas.client_width());
    let h = f64::from(canvas.client_height());
    canvas.set_width((b * dpr) as u32);
    canvas.set_height((h * dpr) as u32);
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    let skala = b / sicht.breite;
    let px = |x: f64| (x - sicht.x) * skala + b / 2.0;
    let py = |y: f64| h / 2.0 - (y - sicht.y) * skala;
    ctx.clear_rect(0.0, 0.0, b, h);
    let stil = fenster.document().and_then(|d| d.document_element()).and_then(|e| fenster.get_computed_style(&e).ok().flatten());
    let farbe = |name: &str| stil.as_ref().and_then(|s| s.get_property_value(name).ok()).unwrap_or_default();
    // Gitter mit runden Schritten.
    let schritt = 10f64.powf((sicht.breite / 4.0).log10().floor());
    ctx.set_stroke_style_str(&farbe("--kante"));
    ctx.set_fill_style_str(&farbe("--gedimmt"));
    ctx.set_font("11px Arial");
    ctx.set_line_width(1.0);
    let beschriftung = |v: f64| js_zahl(auf_stellen(v, 6));
    let mut x = ((sicht.x - sicht.breite / 2.0) / schritt).ceil() * schritt;
    while px(x) < b {
        ctx.begin_path();
        ctx.move_to(px(x), 0.0);
        ctx.line_to(px(x), h);
        ctx.stroke();
        let _ = ctx.fill_text(&beschriftung(x), px(x) + 2.0, (h - 4.0).min((py(0.0) + 12.0).max(12.0)));
        x += schritt;
    }
    let hoehe = h / skala;
    let mut y = ((sicht.y - hoehe / 2.0) / schritt).ceil() * schritt;
    while py(y) > 0.0 {
        ctx.begin_path();
        ctx.move_to(0.0, py(y));
        ctx.line_to(b, py(y));
        ctx.stroke();
        if y.abs() > schritt / 2.0 {
            let _ = ctx.fill_text(&beschriftung(y), (b - 30.0).min((px(0.0) + 3.0).max(2.0)), py(y) - 2.0);
        }
        y += schritt;
    }
    ctx.set_stroke_style_str(&farbe("--gedimmt"));
    ctx.set_line_width(1.5);
    ctx.begin_path();
    ctx.move_to(px(0.0), 0.0);
    ctx.line_to(px(0.0), h);
    ctx.move_to(0.0, py(0.0));
    ctx.line_to(b, py(0.0));
    ctx.stroke();
    // Die Funktionen - Spruenge (tan, 1/x) werden nicht verbunden.
    for (i, f) in funktionen.iter().enumerate() {
        ctx.set_stroke_style_str(FARBEN[i % 4]);
        ctx.set_line_width(2.0);
        ctx.begin_path();
        let mut zuletzt: Option<f64> = None;
        let mut sx = 0.0;
        while sx <= b {
            let x = sicht.x + (sx - b / 2.0) / skala;
            let y = f.wert(&Werte { x, deg: false, ..Werte::default() });
            let sy = py(y);
            if !y.is_finite() || zuletzt.is_some_and(|z| (sy - z).abs() > h) {
                zuletzt = None;
            } else {
                if zuletzt.is_none() {
                    ctx.move_to(sx, sy);
                } else {
                    ctx.line_to(sx, sy);
                }
                zuletzt = Some(sy);
            }
            sx += 1.0;
        }
        ctx.stroke();
    }
}

/// Die Hoerer am Canvas (Mausrad, Groessenaenderung) - werden beim Abbau entfernt.
struct Hoerer {
    canvas: HtmlCanvasElement,
    rad: Closure<dyn FnMut(web_sys::WheelEvent)>,
    beobachter: web_sys::ResizeObserver,
    _groesse: Closure<dyn FnMut()>,
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let texte: Vec<String> = daten.as_array().map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    let funktionen = use_hook({
        let texte = texte.clone();
        move || Rc::new(texte.iter().filter_map(|t| uebersetzen(t).ok()).collect::<Vec<Formel>>())
    });
    let mut sicht = use_signal(|| START);
    let mut canvas = use_signal(|| None::<HtmlCanvasElement>);
    let mut ziehen = use_signal(|| None::<(f64, f64, Sicht)>);
    let hoerer: Rc<RefCell<Option<Hoerer>>> = use_hook(|| Rc::new(RefCell::new(None)));

    // Neu zeichnen, sobald sich die Sicht aendert (oder das Canvas da ist).
    {
        let funktionen = funktionen.clone();
        use_effect(move || {
            let s = sicht();
            if let Some(c) = canvas.read().as_ref() {
                zeichnen(c, s, &funktionen);
            }
        });
    }
    {
        let hoerer = hoerer.clone();
        use_drop(move || {
            if let Some(h) = hoerer.borrow_mut().take() {
                h.beobachter.disconnect();
                let _ = h.canvas.remove_event_listener_with_callback("wheel", h.rad.as_ref().unchecked_ref());
            }
        });
    }

    let gemountet = {
        let (hoerer, funktionen) = (hoerer.clone(), funktionen.clone());
        move |e: MountedEvent| {
            let Some(el) = e.data().downcast::<web_sys::Element>().cloned() else { return };
            let Ok(c) = el.dyn_into::<HtmlCanvasElement>() else { return };
            // Mausrad: zoomen (nicht passiv, damit die Seite nicht scrollt).
            // Aus dem Browser-Callback heraus nur mit Dioxus-Runtime ans Signal (werkzeug::im_scope).
            let zoom = crate::werkzeug::im_scope(move |ev: web_sys::WheelEvent| {
                ev.prevent_default();
                let mut s = sicht;
                s.write().breite *= if ev.delta_y() > 0.0 { 1.15 } else { 1.0 / 1.15 };
            });
            let rad = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |ev: web_sys::WheelEvent| zoom(ev));
            let optionen = web_sys::AddEventListenerOptions::new();
            optionen.set_passive(false);
            let _ = c.add_event_listener_with_callback_and_add_event_listener_options("wheel", rad.as_ref().unchecked_ref(), &optionen);
            // Groesse geaendert: neu zeichnen.
            let (c2, f2) = (c.clone(), funktionen.clone());
            let groesse = Closure::<dyn FnMut()>::new(move || zeichnen(&c2, *sicht.peek(), &f2));
            let Ok(beobachter) = web_sys::ResizeObserver::new(groesse.as_ref().unchecked_ref()) else { return };
            beobachter.observe(&c);
            *hoerer.borrow_mut() = Some(Hoerer { canvas: c.clone(), rad, beobachter, _groesse: groesse });
            canvas.set(Some(c));
        }
    };

    rsx! {
        ModulCss { name: "plot".to_string(), css: CSS }
        Kopf {
            titel: "Graph",
            rechts: rsx! {
                for (i, t) in texte.iter().enumerate() {
                    span { key: "{i}",
                        if i > 0 { " · " }
                        span { style: "color:{FARBEN[i % 4]}", "y = {t}" }
                    }
                }
            },
        }
        canvas {
            class: "pl-flaeche",
            onmounted: gemountet,
            onpointerdown: move |e: PointerEvent| {
                let p = e.client_coordinates();
                ziehen.set(Some((p.x, p.y, *sicht.peek())));
                if let Some(c) = canvas.peek().as_ref() {
                    let _ = c.set_pointer_capture(e.pointer_id());
                }
            },
            onpointermove: move |e: PointerEvent| {
                let Some((x0, y0, s0)) = *ziehen.peek() else { return };
                let breite = canvas.peek().as_ref().map_or(1.0, |c| f64::from(c.client_width()));
                let skala = breite / s0.breite;
                let p = e.client_coordinates();
                sicht.set(Sicht { x: s0.x - (p.x - x0) / skala, y: s0.y + (p.y - y0) / skala, breite: s0.breite });
            },
            onpointerup: move |_| ziehen.set(None),
        }
        div { class: "w-text pl-hinweis",
            "Ziehen zum Verschieben · Mausrad zum Zoomen · "
            button { r#type: "button", class: "w-knopf klein", onclick: move |_| sicht.set(START), "Zurücksetzen" }
        }
    }
}

const CSS: &str = r#"
.pl-flaeche { width: 100%; height: 320px; display: block; border: 1px solid var(--kante); border-radius: var(--r); cursor: grab; touch-action: none; }
.pl-flaeche:active { cursor: grabbing; }
.pl-hinweis { margin-top: 8px; }
"#;
