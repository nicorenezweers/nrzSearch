//! Eine kleine Kartenansicht aus OpenStreetMap-Kacheln in reinem Rust (ersetzt Leaflet).
//!
//! Nur das, was die Info-Karte braucht: Kacheln im Raster (Web-Mercator), ein Punkt mit
//! Namensschild oder eine Grenze als Umriss (GeoJSON, eingepasst wie `fitBounds`), Ziehen mit
//! Maus/Touch (Pointer-Events), Zoom mit +/- und - wenn erlaubt - mit dem Mausrad. Im dunklen
//! Theme werden die Kacheln ueber die CSS-Variable `--kachel-filter` umgedreht.

use dioxus::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure};

const KACHEL: f64 = 256.0;
const ZOOM_MAX: i32 = 19;
const ZOOM_MIN: i32 = 1;

/// Laenge/Breite -> Weltpixel bei Zoom 0 (0..256).
fn welt(lon: f64, lat: f64) -> (f64, f64) {
    let lat = lat.clamp(-85.051_128, 85.051_128).to_radians();
    let x = (lon + 180.0) / 360.0 * KACHEL;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * KACHEL;
    (x, y)
}

/// Alle Ringe (Laenge, Breite) einer GeoJSON-Geometrie (Polygon, MultiPolygon, Linien,
/// Feature, FeatureCollection, GeometryCollection).
fn ringe(g: &Value, aus: &mut Vec<Vec<(f64, f64)>>) {
    let punkte = |v: &Value| -> Vec<(f64, f64)> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
            .collect()
    };
    let c = &g["coordinates"];
    match g["type"].as_str().unwrap_or("") {
        "Feature" => ringe(&g["geometry"], aus),
        "FeatureCollection" => g["features"].as_array().into_iter().flatten().for_each(|f| ringe(f, aus)),
        "GeometryCollection" => g["geometries"].as_array().into_iter().flatten().for_each(|f| ringe(f, aus)),
        "LineString" => aus.push(punkte(c)),
        "Polygon" | "MultiLineString" => c.as_array().into_iter().flatten().for_each(|r| aus.push(punkte(r))),
        "MultiPolygon" => c.as_array().into_iter().flatten().flat_map(|p| p.as_array().into_iter().flatten()).for_each(|r| aus.push(punkte(r))),
        _ => {}
    }
}

/// Der Umriss als SVG-Pfad in Weltpixeln bei Zoom 0 und sein Rahmen (min, max).
fn umriss(grenze: &Value) -> Option<(String, (f64, f64), (f64, f64))> {
    let mut alle = Vec::new();
    ringe(grenze, &mut alle);
    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    let mut d = String::new();
    for ring in alle.iter().filter(|r| r.len() > 1) {
        for (i, (lon, lat)) in ring.iter().enumerate() {
            let (x, y) = welt(*lon, *lat);
            min = (min.0.min(x), min.1.min(y));
            max = (max.0.max(x), max.1.max(y));
            d.push_str(&format!("{}{x:.6} {y:.6}", if i == 0 { "M" } else { "L" }));
        }
        d.push('Z');
    }
    (!d.is_empty()).then_some((d, min, max))
}

/// Zoom und Mitte, damit der Rahmen (min, max) mit 18 px Rand passt - wie `fitBounds`.
fn einpassen(min: (f64, f64), max: (f64, f64), w: f64, h: f64) -> (i32, (f64, f64)) {
    let (bw, bh) = ((max.0 - min.0).max(1e-9), (max.1 - min.1).max(1e-9));
    let passend = ((w - 36.0).max(1.0) / bw).min((h - 36.0).max(1.0) / bh).log2().floor() as i32;
    (passend.clamp(ZOOM_MIN, ZOOM_MAX), ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0))
}

/// Die Karte. Ohne `grenze` steht ein Punkt in der Mitte (mit `name` als Schild beim Klick).
#[component]
pub fn Karte(
    lat: f64,
    lon: f64,
    #[props(default = 14)] zoom: i32,
    #[props(default)] name: Option<String>,
    #[props(default)] grenze: Option<Value>,
    /// Darf das Mausrad zoomen? (In der Info-Karte nicht - da scrollt das Rad die Seite.)
    #[props(default = true)]
    rad_zoom: bool,
    #[props(default)] class: String,
) -> Element {
    let form = use_hook(|| grenze.as_ref().and_then(umriss));
    let mut z = use_signal(|| if form.is_some() { 10 } else { zoom.clamp(ZOOM_MIN, ZOOM_MAX) });
    // Die Mitte in Weltpixeln bei Zoom 0.
    let mut mitte = use_signal(|| welt(lon, lat));
    let mut groesse = use_signal(|| (0.0f64, 0.0f64));
    let mut eingepasst = use_signal(|| false);
    let rahmen = form.as_ref().map(|(_, a, b)| (*a, *b));
    // Neue Groesse merken; beim ersten Mal die Grenze einpassen.
    let mut neue_groesse = move |w: f64, h: f64| {
        if (w, h) == *groesse.peek() {
            return;
        }
        groesse.set((w, h));
        if w > 0.0 && !*eingepasst.peek() {
            eingepasst.set(true);
            if let Some((a, b)) = rahmen {
                let (zz, m) = einpassen(a, b, w, h);
                z.set(zz);
                mitte.set(m);
            }
        }
    };
    let mut wurzel = use_signal(|| None::<web_sys::Element>);
    let mut ziehen = use_signal(|| None::<(f64, f64, (f64, f64), bool)>);
    let mut schild = use_signal(|| false);
    let mut rad_zuletzt = use_signal(|| 0.0f64);

    // Die Groesse beobachten (die Bilder daneben kommen spaeter, das Fenster aendert sich).
    let beobachter: Rc<RefCell<Option<(web_sys::ResizeObserver, Closure<dyn FnMut(js_sys::Array)>)>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let b2 = beobachter.clone();
    use_drop(move || {
        if let Some((o, _)) = b2.borrow_mut().take() {
            o.disconnect();
        }
    });

    let (w, h) = groesse();

    let zoom_jetzt = z();
    let faktor = 2f64.powi(zoom_jetzt);
    let (mx, my) = mitte();
    let (cx, cy) = (mx * faktor, my * faktor);
    let (links, oben) = (cx - w / 2.0, cy - h / 2.0);
    let anzahl = 1i64 << zoom_jetzt;

    // Die sichtbaren Kacheln.
    let mut kacheln = Vec::new();
    if w > 0.0 {
        let (x0, x1) = ((links / KACHEL).floor() as i64, ((links + w) / KACHEL).floor() as i64);
        let (y0, y1) = (((oben / KACHEL).floor() as i64).max(0), (((oben + h) / KACHEL).floor() as i64).min(anzahl - 1));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let x = tx.rem_euclid(anzahl);
                kacheln.push((
                    format!("https://tile.openstreetmap.org/{zoom_jetzt}/{x}/{ty}.png"),
                    tx as f64 * KACHEL - links,
                    ty as f64 * KACHEL - oben,
                ));
            }
        }
    }
    let (px, py) = welt(lon, lat);
    let punkt = (px * faktor - links, py * faktor - oben);

    // Zoomen um einen Punkt (Bildschirm relativ zur Karte), der dabei stehen bleibt.
    let mut zoomen = move |stufe: i32, um: Option<(f64, f64)>| {
        let alt = z();
        let neu = (alt + stufe).clamp(ZOOM_MIN, ZOOM_MAX);
        if neu == alt {
            return;
        }
        let (w, h) = groesse();
        let (ux, uy) = um.unwrap_or((w / 2.0, h / 2.0));
        let (mx, my) = mitte();
        let f_alt = 2f64.powi(alt);
        // Der Punkt unter dem Zeiger in Welt-0-Pixeln.
        let (wx, wy) = (mx + (ux - w / 2.0) / f_alt, my + (uy - h / 2.0) / f_alt);
        let f_neu = 2f64.powi(neu);
        mitte.set((wx - (ux - w / 2.0) / f_neu, (wy - (uy - h / 2.0) / f_neu).clamp(0.0, KACHEL)));
        z.set(neu);
    };
    let zeiger = move |e: &web_sys::MouseEvent| -> Option<(f64, f64)> {
        let r = wurzel.peek().as_ref()?.get_bounding_client_rect();
        Some((f64::from(e.client_x()) - r.left(), f64::from(e.client_y()) - r.top()))
    };

    let farbe = "var(--hoch, #8ab4f8)";
    rsx! {
        crate::werkzeug::ModulCss { name: "karte".to_string(), css: KARTE_CSS }
        div {
            class: "nrz-karte {class}",
            onmounted: move |e| {
                let Some(el) = e.data().downcast::<web_sys::Element>().cloned() else { return };
                let r = el.get_bounding_client_rect();
                neue_groesse(r.width(), r.height());
                // Browser-Callback: nur mit Dioxus-Runtime an die Signale (werkzeug::im_scope).
                let groesse = crate::werkzeug::im_scope(move |(b, h): (f64, f64)| neue_groesse(b, h));
                let c = Closure::<dyn FnMut(js_sys::Array)>::new(move |eintraege: js_sys::Array| {
                    if let Ok(e) = eintraege.get(0).dyn_into::<web_sys::ResizeObserverEntry>() {
                        let r = e.content_rect();
                        groesse((r.width(), r.height()));
                    }
                });
                if let Ok(o) = web_sys::ResizeObserver::new(c.as_ref().unchecked_ref()) {
                    o.observe(&el);
                    *beobachter.borrow_mut() = Some((o, c));
                }
                wurzel.set(Some(el));
            },
            onpointerdown: move |e| {
                let Some(p) = e.data().downcast::<web_sys::PointerEvent>().cloned() else { return };
                if p.button() != 0 {
                    return;
                }
                // Knoepfe und Links in der Karte nicht als Ziehen werten.
                let auf_knopf = p
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                    .and_then(|t| t.closest("button, a").ok().flatten())
                    .is_some();
                if auf_knopf {
                    return;
                }
                if let Some(el) = wurzel.peek().as_ref() {
                    let _ = el.set_pointer_capture(p.pointer_id());
                }
                ziehen.set(Some((f64::from(p.client_x()), f64::from(p.client_y()), mitte(), false)));
            },
            onpointermove: move |e| {
                let Some((sx, sy, start, _)) = ziehen() else { return };
                let Some(p) = e.data().downcast::<web_sys::PointerEvent>().cloned() else { return };
                let (dx, dy) = (f64::from(p.client_x()) - sx, f64::from(p.client_y()) - sy);
                let f = 2f64.powi(z());
                mitte.set((start.0 - dx / f, (start.1 - dy / f).clamp(0.0, KACHEL)));
                if dx.abs() + dy.abs() > 4.0 {
                    ziehen.set(Some((sx, sy, start, true)));
                }
            },
            onpointerup: move |_| ziehen.set(None),
            onpointercancel: move |_| ziehen.set(None),
            onwheel: move |e| {
                if !rad_zoom {
                    return;
                }
                e.prevent_default();
                let Some(r) = e.data().downcast::<web_sys::WheelEvent>().cloned() else { return };
                // Das Touchpad schickt viele kleine Schritte - hoechstens alle 250 ms eine Stufe.
                let jetzt = js_sys::Date::now();
                if r.delta_y() == 0.0 || jetzt - rad_zuletzt() < 250.0 {
                    return;
                }
                rad_zuletzt.set(jetzt);
                let um = zeiger(r.unchecked_ref());
                zoomen(if r.delta_y() < 0.0 { 1 } else { -1 }, um);
            },
            ondoubleclick: move |e| {
                let um = e.data().downcast::<web_sys::MouseEvent>().and_then(|m| zeiger(m));
                zoomen(1, um);
            },
            div { class: "nrz-karte-kacheln",
                for (src, x, y) in kacheln {
                    img {
                        key: "{src}|{x}",
                        src: "{src}",
                        alt: "",
                        draggable: "false",
                        style: "left:{x}px;top:{y}px",
                    }
                }
            }
            if let Some((d, _, _)) = &form {
                svg { class: "nrz-karte-form", width: "{w}", height: "{h}",
                    path {
                        d: "{d}",
                        transform: "translate({-links} {-oben}) scale({faktor})",
                        fill_rule: "evenodd",
                        vector_effect: "non-scaling-stroke",
                        style: "stroke:{farbe};fill:{farbe};fill-opacity:.12;stroke-width:2",
                    }
                }
            } else if w > 0.0 {
                div {
                    class: "ort-punkt nrz-karte-punkt",
                    style: "left:{punkt.0}px;top:{punkt.1}px",
                    onclick: move |_| {
                        if !ziehen.peek().is_some_and(|z| z.3) {
                            schild.toggle();
                        }
                    },
                }
                if schild() {
                    if let Some(n) = name.as_ref().filter(|n| !n.is_empty()) {
                        div { class: "nrz-karte-schild", style: "left:{punkt.0}px;top:{punkt.1}px",
                            "{n}"
                            button {
                                r#type: "button",
                                "aria-label": "Schließen",
                                onclick: move |_| schild.set(false),
                                "×"
                            }
                        }
                    }
                }
            }
            div { class: "nrz-karte-zoom",
                button {
                    r#type: "button",
                    title: "Hineinzoomen",
                    "aria-label": "Hineinzoomen",
                    disabled: zoom_jetzt >= ZOOM_MAX,
                    onclick: move |_| zoomen(1, None),
                    "+"
                }
                button {
                    r#type: "button",
                    title: "Herauszoomen",
                    "aria-label": "Herauszoomen",
                    disabled: zoom_jetzt <= ZOOM_MIN,
                    onclick: move |_| zoomen(-1, None),
                    "−"
                }
            }
            div { class: "nrz-karte-quelle",
                "© "
                a { href: "https://www.openstreetmap.org/copyright", target: "_blank", rel: "noreferrer", "OpenStreetMap" }
                "-Mitwirkende"
            }
        }
    }
}

const KARTE_CSS: &str = r#"
.nrz-karte { position: relative; width: 100%; height: 100%; overflow: hidden; background: var(--flaeche2); touch-action: none; cursor: grab; user-select: none; -webkit-user-select: none; }
.nrz-karte:active { cursor: grabbing; }
.nrz-karte-kacheln { position: absolute; inset: 0; filter: var(--kachel-filter); }
.nrz-karte-kacheln img { position: absolute; width: 256px; height: 256px; max-width: none; pointer-events: none; }
.nrz-karte-form { position: absolute; left: 0; top: 0; pointer-events: none; overflow: visible; }
.nrz-karte-punkt { position: absolute; width: 16px; height: 16px; margin: -8px 0 0 -8px; box-sizing: border-box; cursor: pointer; }
.nrz-karte-schild { position: absolute; transform: translate(-50%, calc(-100% - 14px)); background: var(--menue, var(--flaeche2)); color: var(--schrift); border: 1px solid var(--kante); border-radius: var(--r2, 6px); box-shadow: var(--schatten); padding: 6px 26px 6px 10px; font-size: 13px; white-space: nowrap; z-index: 3; }
.nrz-karte-schild button { position: absolute; top: 2px; right: 4px; border: 0; background: none; color: var(--gedimmt); font-size: 15px; line-height: 1; cursor: pointer; padding: 2px; }
.nrz-karte-zoom { position: absolute; top: 10px; left: 10px; display: flex; flex-direction: column; border: 2px solid rgba(0, 0, 0, .2); border-radius: 4px; overflow: hidden; z-index: 2; }
.nrz-karte-zoom button { width: 30px; height: 30px; border: 0; border-bottom: 1px solid var(--kante); background: var(--flaeche2); color: var(--schrift); font: bold 18px/30px Arial, sans-serif; cursor: pointer; padding: 0; }
.nrz-karte-zoom button:last-child { border-bottom: 0; }
.nrz-karte-zoom button:hover { background: var(--flaeche3); }
.nrz-karte-zoom button:disabled { color: var(--gedimmt); cursor: default; }
.nrz-karte-quelle { position: absolute; right: 0; bottom: 0; z-index: 2; padding: 0 5px; background: rgba(32, 33, 36, .82); color: var(--gedimmt); font-size: .62rem; line-height: 1.4; }
.nrz-karte-quelle a { color: var(--hoch); text-decoration: none; }
"#;
