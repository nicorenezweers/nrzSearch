//! Modul "snake": das Spiel Snake - Pfeiltasten oder WASD, auf dem Handy wischen.
//! Der Rekord bleibt nur im eigenen Browser (localStorage "nrz_snake").
//! Portierung von public/module/snake.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, speicher_lesen, speicher_schreiben, use_intervall, use_tasten, zufall};
use dioxus::prelude::*;
use serde_json::{Value, json};

/// Breite und Hoehe des Felds in Kaestchen (je 20 px).
const B: i32 = 24;
const H: i32 = 16;
const SPEICHER: &str = "nrz_snake";

/// Das Modul hat kein eigenes CSS (die Flaeche nutzt `.spiel-flaeche` aus style.css).
const CSS: &str = "";

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

/// Der Spielstand.
#[derive(Clone)]
struct Spiel {
    schlange: Vec<(i32, i32)>,
    richtung: (i32, i32),
    naechste: (i32, i32),
    futter: (i32, i32),
    punkte: u32,
    laeuft: bool,
    vorbei: bool,
}

impl Spiel {
    fn neu() -> Self {
        let mut s = Spiel { schlange: vec![(8, 8), (7, 8), (6, 8)], richtung: (1, 0), naechste: (1, 0), futter: (0, 0), punkte: 0, laeuft: false, vorbei: false };
        s.futter_legen();
        s
    }
    fn futter_legen(&mut self) {
        loop {
            let f = (zufall(B as u32) as i32, zufall(H as u32) as i32);
            if !self.schlange.contains(&f) {
                self.futter = f;
                return;
            }
        }
    }
    /// Ein Schritt; gibt die Punkte zurueck, wenn das Spiel gerade endet.
    fn schritt(&mut self) -> Option<u32> {
        self.richtung = self.naechste;
        let (x, y) = (self.schlange[0].0 + self.richtung.0, self.schlange[0].1 + self.richtung.1);
        if x < 0 || y < 0 || x >= B || y >= H || self.schlange.contains(&(x, y)) {
            self.laeuft = false;
            self.vorbei = true;
            return Some(self.punkte);
        }
        self.schlange.insert(0, (x, y));
        if (x, y) == self.futter {
            self.punkte += 1;
            self.futter_legen();
        } else {
            self.schlange.pop();
        }
        None
    }
    fn steuern(&mut self, r: (i32, i32)) {
        if r.0 != -self.richtung.0 || r.1 != -self.richtung.1 {
            self.naechste = r;
        }
    }
    fn umschalten(&mut self) {
        if self.vorbei {
            *self = Spiel::neu();
        }
        self.laeuft = !self.laeuft;
    }
}

/// Richtung zu einer Taste (Pfeile, WASD).
fn richtung(taste: &str) -> Option<(i32, i32)> {
    Some(match taste {
        "ArrowUp" | "w" | "W" => (0, -1),
        "ArrowDown" | "s" | "S" => (0, 1),
        "ArrowLeft" | "a" | "A" => (-1, 0),
        "ArrowRight" | "d" | "D" => (1, 0),
        _ => return None,
    })
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = (&ctx, &daten);
    let mut spiel = use_signal(Spiel::neu);
    let mut rekord = use_signal(|| speicher_lesen(SPEICHER).and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok())).unwrap_or(0));
    let mut beruehrung = use_signal(|| None::<(f64, f64)>);

    use_intervall(110, move || {
        if !spiel.peek().laeuft {
            return;
        }
        if let Some(p) = spiel.write().schritt() {
            if u64::from(p) > *rekord.peek() {
                rekord.set(u64::from(p));
                speicher_schreiben(SPEICHER, &json!(p));
            }
        }
    });
    use_tasten(move |e| {
        if e.code() == "Space" {
            e.prevent_default();
            spiel.write().umschalten();
            return;
        }
        if let Some(r) = richtung(&e.key()) {
            if spiel.peek().laeuft {
                e.prevent_default();
                spiel.write().steuern(r);
            }
        }
    });

    let s = spiel.read();
    let rechts = rsx! { "Punkte: " b { "{s.punkte}" } " · Rekord: " b { "{rekord}" } };
    let meldung = if s.vorbei { format!("Vorbei – {} Punkte · Leertaste", s.punkte) } else { "Klicken oder Leertaste zum Starten".into() };
    rsx! {
        ModulCss { name: "snake", css: CSS }
        Kopf { titel: "Snake", rechts }
        svg {
            class: "spiel-flaeche",
            width: "{B * 20}",
            height: "{H * 20}",
            view_box: "0 0 {B * 20} {H * 20}",
            onclick: move |_| {
                if !spiel.peek().laeuft {
                    spiel.write().umschalten();
                }
            },
            ontouchstart: move |e| {
                if let Some(t) = e.touches().first() {
                    let p = t.client_coordinates();
                    beruehrung.set(Some((p.x, p.y)));
                }
            },
            ontouchend: move |e| {
                let Some(p) = e.touches_changed().first().map(|t| t.client_coordinates()) else { return };
                let Some((x0, y0)) = beruehrung() else { return };
                let (dx, dy) = (p.x - x0, p.y - y0);
                if dx.abs().max(dy.abs()) < 20.0 {
                    if !spiel.peek().laeuft {
                        spiel.write().umschalten();
                    }
                    return;
                }
                let r = if dx.abs() > dy.abs() { (dx.signum() as i32, 0) } else { (0, dy.signum() as i32) };
                spiel.write().steuern(r);
            },
            rect { width: "100%", height: "100%", style: "fill: var(--flaeche2)" }
            circle { cx: "{s.futter.0 * 20 + 10}", cy: "{s.futter.1 * 20 + 10}", r: "8", fill: "#ea4335" }
            for (i, (x, y)) in s.schlange.iter().enumerate() {
                rect { x: "{x * 20 + 1}", y: "{y * 20 + 1}", width: "18", height: "18", fill: if i == 0 { "#1a5ad7" } else { "#4285f4" } }
            }
            if !s.laeuft {
                rect { width: "100%", height: "100%", fill: "rgba(0,0,0,.5)" }
                text { x: "{B * 10}", y: "{H * 10}", fill: "#fff", font_size: "22", font_family: "Arial", text_anchor: "middle", dominant_baseline: "middle", "{meldung}" }
            }
        }
        div { class: "w-text", "Pfeiltasten oder WASD · Leertaste: Start/Pause · am Handy wischen" }
    }
}
