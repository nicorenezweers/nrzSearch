//! Modul "farbe": "#1a73e8", "rgb(26,115,232)", "hsl(…)", "Color Picker" -
//! ein Farbwaehler, der HEX, RGB, HSL und CMYK gleichzeitig umrechnet.

use super::ModulCtx;
use crate::werkzeug::{Kopf, KopierKnopf, ModulCss};
use dioxus::prelude::*;
use serde_json::{Value, json};

const CSS: &str = r#"
.fa-rumpf { display: flex; gap: 20px; align-items: flex-start; flex-wrap: wrap; }
.fa-feld { position: relative; width: 160px; height: 160px; cursor: pointer; flex-shrink: 0; }
.fa-feld input { position: absolute; inset: 0; opacity: 0; cursor: pointer; width: 100%; height: 100%; }
.fa-muster { display: block; width: 100%; height: 100%; border-radius: var(--r); border: 1px solid var(--kante); }
.fa-werte { flex: 1; min-width: 260px; }
"#;

/// Runden wie JS `Math.round` (.5 nach oben).
fn runden(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// "#1a73e8" oder "#abc" -> [r, g, b]
fn hex_zu_rgb(hex: &str) -> Option<[f64; 3]> {
    let h = hex.trim_start_matches('#');
    let h: String = if h.chars().count() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let n = u32::from_str_radix(&h, 16).ok()?;
    Some([f64::from(n >> 16), f64::from((n >> 8) & 255), f64::from(n & 255)])
}

fn rgb_zu_hex([r, g, b]: [f64; 3]) -> String {
    let teil = |v: f64| format!("{:02x}", runden(v).clamp(0.0, 255.0) as u32);
    format!("#{}{}{}", teil(r), teil(g), teil(b))
}

fn rgb_zu_hsl([r, g, b]: [f64; 3]) -> [f64; 3] {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if max == min {
        return [0.0, 0.0, runden(l * 100.0)];
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [runden(h * 60.0), runden(s * 100.0), runden(l * 100.0)]
}

fn hsl_zu_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let (s, l) = (s / 100.0, l / 100.0);
    let k = |n: f64| (n + h / 30.0) % 12.0;
    let a = s * l.min(1.0 - l);
    let f = |n: f64| l - a * (-1.0f64).max((k(n) - 3.0).min(9.0 - k(n)).min(1.0));
    [runden(f(0.0) * 255.0), runden(f(8.0) * 255.0), runden(f(4.0) * 255.0)]
}

fn rgb_zu_cmyk([r, g, b]: [f64; 3]) -> [f64; 4] {
    let k = 1.0 - r.max(g).max(b) / 255.0;
    if k == 1.0 {
        return [0.0, 0.0, 0.0, 100.0];
    }
    let teil = |v: f64| runden((1.0 - v / 255.0 - k) / (1.0 - k) * 100.0);
    [teil(r), teil(g), teil(b), runden(k * 100.0)]
}

/// Eine Farbe aus der Frage lesen (HEX, rgb(…), hsl(…)).
fn lesen(text: &str) -> Option<[f64; 3]> {
    let t = text.trim().to_lowercase();
    // /^#([0-9a-f]{3}|[0-9a-f]{6})$/
    if let Some(h) = t.strip_prefix('#') {
        if (h.len() == 3 || h.len() == 6) && h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return hex_zu_rgb(&t);
        }
    }
    // /[\d.]+/g
    let mut zahlen: Vec<f64> = Vec::new();
    let mut stueck = String::new();
    for c in t.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() || c == '.' {
            stueck.push(c);
        } else if !stueck.is_empty() {
            zahlen.push(stueck.parse().unwrap_or(f64::NAN));
            stueck.clear();
        }
    }
    if zahlen.len() < 3 || zahlen[..3].iter().any(|z| z.is_nan()) {
        return None;
    }
    if t.starts_with("rgb") {
        return Some([zahlen[0].min(255.0), zahlen[1].min(255.0), zahlen[2].min(255.0)]);
    }
    if t.starts_with("hsl") {
        return Some(hsl_zu_rgb(zahlen[0], zahlen[1], zahlen[2]));
    }
    None
}

/// Prueft die Frage genauer und liefert die Daten fuer [`Zeigen`] - oder None (Modul passt nicht).
pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(json!(ctx.gruppe(1).and_then(lesen).unwrap_or([26.0, 115.0, 232.0])))
}

/// Zeichnet das Modul (steht in `<section class="modul modul-farbe">`).
#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let start = {
        let v: Vec<f64> = daten.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        if v.len() >= 3 { [v[0], v[1], v[2]] } else { [26.0, 115.0, 232.0] }
    };
    let mut farbe = use_signal(|| start);
    // Zaehlt jedes Setzen - die Eingabefelder werden dann neu gezeichnet (wie innerHTML im Original).
    let mut stand = use_signal(|| 0u32);
    let mut setzen = move |rgb: [f64; 3]| {
        farbe.set(rgb);
        stand += 1;
    };

    let rgb = farbe();
    let [r, g, b] = rgb;
    let hex = rgb_zu_hex(rgb);
    let [h, s, l] = rgb_zu_hsl(rgb);
    let [c, m, y, k] = rgb_zu_cmyk(rgb);
    let zeilen = [
        ("HEX", hex.to_uppercase()),
        ("RGB", format!("rgb({r}, {g}, {b})")),
        ("HSL", format!("hsl({h}, {s}%, {l}%)")),
        ("CMYK", format!("{c}%, {m}%, {y}%, {k}%")),
    ];
    let n = stand();

    rsx! {
        ModulCss { name: "farbe".to_string(), css: CSS }
        Kopf { titel: "Farbwähler".to_string() }
        div { class: "fa-rumpf",
            label { class: "fa-feld",
                input {
                    r#type: "color",
                    class: "fa-waehler",
                    value: "{hex}",
                    oninput: move |e| {
                        if let Some(rgb) = hex_zu_rgb(&e.value()) {
                            setzen(rgb);
                        }
                    },
                }
                span { class: "fa-muster", style: "background: {hex}" }
            }
            table { class: "w-tabelle fa-werte",
                tbody {
                    for (name, wert) in zeilen {
                        tr { key: "{name}-{n}",
                            td { "{name}" }
                            td {
                                // Einen Wert eintippen (HEX, RGB, HSL) und mit Enter uebernehmen.
                                input {
                                    class: "w-eingabe",
                                    "data-art": "{name}",
                                    value: "{wert}",
                                    onchange: move |e| {
                                        if name == "CMYK" {
                                            return;
                                        }
                                        let v = e.value();
                                        let text = if name == "HEX" && !v.starts_with('#') { format!("#{v}") } else { v };
                                        if let Some(rgb) = lesen(&text) {
                                            setzen(rgb);
                                        }
                                    },
                                }
                            }
                            td { KopierKnopf { text: wert.clone(), class: "w-knopf klein".to_string() } }
                        }
                    }
                }
            }
        }
    }
}
