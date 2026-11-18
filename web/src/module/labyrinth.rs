//! Modul "labyrinth": "Labyrinth-Jagd" - ein eigenes Spiel. Ein Rabe sammelt Sterne in einem
//! Irrgarten, der fuer jedes Level neu entsteht; Waechter-Drohnen patrouillieren und jagen ihn.
//! Nebelfedern machen den Raben fuer kurze Zeit unsichtbar (Drohnen irren umher, Sterne zaehlen
//! doppelt). Pfeiltasten oder WASD, Leertaste Start/Pause, am Handy wischen oder Knoepfe.
//! Ersetzt das alte public/module/pacman.js. Der Rekord bleibt im Browser ("nrz_labyrinth").

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, speicher_lesen, speicher_schreiben, use_intervall, use_tasten, zufall};
use dioxus::prelude::*;
use serde_json::{Value, json};

const CSS: &str = r#"
.lj-flaeche .lj-figur { transition: transform 150ms linear; }
.lj-flaeche .lj-drohne { transition: transform 210ms linear; }
.lj-flaeche .lj-feder { animation: lj-schweben 1.6s ease-in-out infinite alternate; }
@keyframes lj-schweben { from { opacity: .55; } to { opacity: 1; } }
.lj-knoepfe { display: none; grid-template-columns: repeat(3, 52px); grid-template-rows: repeat(2, 44px); gap: 6px; justify-content: center; margin: 4px 0 10px; }
.lj-knoepfe button { font: 500 20px Arial; border: 1px solid var(--kante); border-radius: var(--r); background: var(--flaeche2); color: var(--schrift); cursor: pointer; touch-action: manipulation; }
.lj-knoepfe .hoch { grid-column: 2; }
.lj-knoepfe .links { grid-column: 1; grid-row: 2; }
.lj-knoepfe .runter { grid-column: 2; grid-row: 2; }
.lj-knoepfe .rechts { grid-column: 3; grid-row: 2; }
@media (hover: none), (pointer: coarse) { .lj-knoepfe { display: grid; } }
"#;

/// Groesse des Irrgartens in Kacheln (ungerade) und Kachelgroesse in px.
const W: i32 = 25;
const H: i32 = 17;
const K: i32 = 20;
/// Ein Takt der Spielschleife in ms.
const TAKT: u32 = 40;
/// Der Rabe zieht alle 4 Takte ein Feld weiter.
const RABE_TAKTE: u32 = 4;
/// So lange haelt der Nebel einer Feder (in Takten, = 6 s).
const NEBEL_TAKTE: u32 = 150;
const SPEICHER: &str = "nrz_labyrinth";
/// Ein Stern (5 Zacken, Mittelpunkt 0,0).
const STERN: &str = "0,-6 1.8,-2 6,-1.9 2.8,1 3.7,5.2 0,3 -3.7,5.2 -2.8,1 -6,-1.9 -1.8,-2";

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

type Pos = (i32, i32);

#[derive(Clone, Copy, PartialEq)]
enum Zustand {
    Bereit,
    Laeuft,
    Pause,
    Erwischt,
    Geschafft,
    Vorbei,
}

#[derive(Clone)]
struct Drohne {
    pos: Pos,
    start: Pos,
    richtung: Pos,
}

/// Der Spielstand.
#[derive(Clone)]
struct Spiel {
    wand: Vec<bool>,
    wand_pfad: String,
    sterne: Vec<bool>,
    sterne_uebrig: u32,
    federn: Vec<Pos>,
    rabe: Pos,
    richtung: Pos,
    wunsch: Pos,
    blick: Pos,
    drohnen: Vec<Drohne>,
    punkte: u32,
    leben: u32,
    level: u32,
    takt: u32,
    level_takte: u32,
    nebel: u32,
    bonus: u32,
    zustand: Zustand,
}

const RABE_START: Pos = (1, 1);

fn idx((x, y): Pos) -> usize {
    (y * W + x) as usize
}

impl Spiel {
    fn neu() -> Self {
        let mut s = Spiel {
            wand: Vec::new(),
            wand_pfad: String::new(),
            sterne: Vec::new(),
            sterne_uebrig: 0,
            federn: Vec::new(),
            rabe: RABE_START,
            richtung: (0, 0),
            wunsch: (0, 0),
            blick: (1, 0),
            drohnen: Vec::new(),
            punkte: 0,
            leben: 3,
            level: 1,
            takt: 0,
            level_takte: 0,
            nebel: 0,
            bonus: 0,
            zustand: Zustand::Bereit,
        };
        s.level_bauen();
        s
    }

    fn frei(&self, p: Pos) -> bool {
        p.0 >= 0 && p.1 >= 0 && p.0 < W && p.1 < H && !self.wand[idx(p)]
    }

    /// Ein neuer Irrgarten: Tiefensuche ueber die Zellen, danach werden Sackgassen meist
    /// geoeffnet, damit es Rundwege zum Ausweichen gibt.
    fn level_bauen(&mut self) {
        let mut wand = vec![true; (W * H) as usize];
        let mut stapel = vec![RABE_START];
        wand[idx(RABE_START)] = false;
        const SCHRITTE: [Pos; 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
        let in_feld = |(x, y): Pos| x > 0 && y > 0 && x < W - 1 && y < H - 1;
        while let Some(&(x, y)) = stapel.last() {
            let nachbarn: Vec<Pos> = SCHRITTE.iter().map(|&(dx, dy)| (x + 2 * dx, y + 2 * dy)).filter(|&p| in_feld(p) && wand[idx(p)]).collect();
            if nachbarn.is_empty() {
                stapel.pop();
                continue;
            }
            let n = nachbarn[zufall(nachbarn.len() as u32) as usize];
            wand[idx(((x + n.0) / 2, (y + n.1) / 2))] = false;
            wand[idx(n)] = false;
            stapel.push(n);
        }
        // Sackgassen oeffnen (zu 75 %).
        for y in (1..H - 1).step_by(2) {
            for x in (1..W - 1).step_by(2) {
                let offen = SCHRITTE.iter().filter(|&&(dx, dy)| !wand[idx((x + dx, y + dy))]).count();
                if offen == 1 && zufall(4) > 0 {
                    let moeglich: Vec<Pos> = SCHRITTE.iter().copied().filter(|&(dx, dy)| wand[idx((x + dx, y + dy))] && in_feld((x + 2 * dx, y + 2 * dy))).collect();
                    if !moeglich.is_empty() {
                        let (dx, dy) = moeglich[zufall(moeglich.len() as u32) as usize];
                        wand[idx((x + dx, y + dy))] = false;
                    }
                }
            }
        }
        let mut pfad = String::new();
        for y in 0..H {
            for x in 0..W {
                if wand[idx((x, y))] {
                    pfad.push_str(&format!("M{} {}h{K}v{K}h-{K}z", x * K, y * K));
                }
            }
        }
        self.wand = wand;
        self.wand_pfad = pfad;

        // Drohnen in den anderen Ecken und in der Mitte; je Level eine mehr (hoechstens 4).
        let plaetze = [(W - 2, H - 2), (W - 2, 1), (1, H - 2), (W / 2 | 1, H / 2 | 1)];
        let anzahl = (self.level as usize + 1).min(plaetze.len());
        self.drohnen = plaetze[..anzahl].iter().map(|&p| Drohne { pos: p, start: p, richtung: (0, 0) }).collect();

        // Zwei Nebelfedern auf zufaelligen freien Feldern weit weg vom Start.
        let freie: Vec<Pos> = (0..H).flat_map(|y| (0..W).map(move |x| (x, y))).filter(|&p| self.frei(p) && p.0 + p.1 > 8).collect();
        self.federn.clear();
        while self.federn.len() < 2 && freie.len() > 2 {
            let p = freie[zufall(freie.len() as u32) as usize];
            if !self.federn.contains(&p) && !plaetze.contains(&p) {
                self.federn.push(p);
            }
        }
        // Sterne auf allen anderen freien Feldern.
        self.sterne = (0..W * H).map(|i| {
            let p = (i % W, i / W);
            self.frei(p) && p != RABE_START && !self.federn.contains(&p) && !self.drohnen.iter().any(|d| d.start == p)
        }).collect();
        self.sterne_uebrig = self.sterne.iter().filter(|&&s| s).count() as u32;
        self.level_takte = 0;
        self.figuren_zurueck();
    }

    fn figuren_zurueck(&mut self) {
        self.rabe = RABE_START;
        self.richtung = (0, 0);
        self.wunsch = (0, 0);
        self.blick = (1, 0);
        self.nebel = 0;
        for d in &mut self.drohnen {
            d.pos = d.start;
            d.richtung = (0, 0);
        }
    }

    /// Takte zwischen zwei Schritten einer Drohne (wird mit dem Level schneller).
    fn drohnen_takte(&self) -> u32 {
        7u32.saturating_sub(self.level).max(4)
    }

    /// Wie oft (in %) eine Drohne an einer Kreuzung den Weg zum Raben waehlt.
    fn jagd_prozent(&self) -> u32 {
        (25 + 10 * self.level).min(75)
    }

    /// Ein Takt der Spielschleife; gibt die Punkte zurueck, wenn das Spiel gerade endet.
    fn takt(&mut self) -> Option<u32> {
        if self.zustand != Zustand::Laeuft {
            return None;
        }
        self.takt += 1;
        self.level_takte += 1;
        self.nebel = self.nebel.saturating_sub(1);
        if self.takt % RABE_TAKTE == 0 {
            self.rabe_ziehen();
            if self.erwischt() {
                return self.verloren();
            }
            if self.sterne_uebrig == 0 {
                let sek = self.level_takte * TAKT / 1000;
                self.bonus = 50 * self.level + 2 * 120u32.saturating_sub(sek);
                self.punkte += self.bonus;
                self.zustand = Zustand::Geschafft;
                return None;
            }
        }
        if self.takt % self.drohnen_takte() == 0 {
            self.drohnen_ziehen();
            if self.erwischt() {
                return self.verloren();
            }
        }
        None
    }

    fn rabe_ziehen(&mut self) {
        let (x, y) = self.rabe;
        if self.wunsch != (0, 0) && self.frei((x + self.wunsch.0, y + self.wunsch.1)) {
            self.richtung = self.wunsch;
        }
        let ziel = (x + self.richtung.0, y + self.richtung.1);
        if self.richtung == (0, 0) || !self.frei(ziel) {
            return;
        }
        self.rabe = ziel;
        self.blick = self.richtung;
        let i = idx(ziel);
        if self.sterne[i] {
            self.sterne[i] = false;
            self.sterne_uebrig -= 1;
            self.punkte += if self.nebel > 0 { 20 } else { 10 };
        }
        if let Some(n) = self.federn.iter().position(|&f| f == ziel) {
            self.federn.remove(n);
            self.punkte += 50;
            self.nebel = NEBEL_TAKTE;
        }
    }

    fn drohnen_ziehen(&mut self) {
        let jagd = if self.nebel > 0 { 0 } else { self.jagd_prozent() };
        let rabe = self.rabe;
        for i in 0..self.drohnen.len() {
            let Drohne { pos: (x, y), richtung: r, .. } = self.drohnen[i];
            let mut wege: Vec<Pos> = [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter().filter(|&(dx, dy)| (dx, dy) != (-r.0, -r.1) && self.frei((x + dx, y + dy))).collect();
            if wege.is_empty() {
                wege.push((-r.0, -r.1));
            }
            let w = if zufall(100) < jagd {
                *wege.iter().min_by_key(|&&(dx, dy)| (x + dx - rabe.0).abs() + (y + dy - rabe.1).abs()).unwrap_or(&wege[0])
            } else {
                wege[zufall(wege.len() as u32) as usize]
            };
            let d = &mut self.drohnen[i];
            d.richtung = w;
            d.pos = (x + w.0, y + w.1);
        }
    }

    fn erwischt(&self) -> bool {
        self.nebel == 0 && self.drohnen.iter().any(|d| d.pos == self.rabe)
    }

    fn verloren(&mut self) -> Option<u32> {
        self.leben -= 1;
        if self.leben == 0 {
            self.zustand = Zustand::Vorbei;
            Some(self.punkte)
        } else {
            self.zustand = Zustand::Erwischt;
            None
        }
    }

    /// Leertaste / Klick: starten, pausieren, weiter.
    fn umschalten(&mut self) {
        self.zustand = match self.zustand {
            Zustand::Bereit | Zustand::Pause => Zustand::Laeuft,
            Zustand::Laeuft => Zustand::Pause,
            Zustand::Erwischt => {
                self.figuren_zurueck();
                Zustand::Laeuft
            }
            Zustand::Geschafft => {
                self.level += 1;
                self.level_bauen();
                Zustand::Laeuft
            }
            Zustand::Vorbei => {
                *self = Spiel::neu();
                Zustand::Laeuft
            }
        };
    }

    fn steuern(&mut self, r: Pos) {
        self.wunsch = r;
        if self.zustand == Zustand::Bereit {
            self.zustand = Zustand::Laeuft;
        }
    }

    fn meldung(&self) -> String {
        match self.zustand {
            Zustand::Bereit => "Klicken oder Leertaste zum Starten".into(),
            Zustand::Pause => "Pause · Leertaste: weiter".into(),
            Zustand::Erwischt => "Erwischt! Leertaste: weiter".into(),
            Zustand::Geschafft => format!("Level {} geschafft! +{} Bonus · Leertaste", self.level, self.bonus),
            Zustand::Vorbei => format!("Spiel vorbei – {} Punkte · Leertaste", self.punkte),
            Zustand::Laeuft => String::new(),
        }
    }
}

/// Richtung zu einer Taste (Pfeile, WASD).
fn richtung(taste: &str) -> Option<Pos> {
    Some(match taste {
        "ArrowUp" | "w" | "W" => (0, -1),
        "ArrowDown" | "s" | "S" => (0, 1),
        "ArrowLeft" | "a" | "A" => (-1, 0),
        "ArrowRight" | "d" | "D" => (1, 0),
        _ => return None,
    })
}

/// Mitte einer Kachel in px.
fn mitte(p: Pos) -> (i32, i32) {
    (p.0 * K + K / 2, p.1 * K + K / 2)
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = (&ctx, &daten);
    let mut spiel = use_signal(Spiel::neu);
    let mut rekord = use_signal(|| speicher_lesen(SPEICHER).and_then(|v| v.as_u64()).unwrap_or(0));
    let mut beruehrung = use_signal(|| None::<(f64, f64)>);

    use_intervall(TAKT, move || {
        if spiel.peek().zustand != Zustand::Laeuft {
            return;
        }
        let ende = spiel.write().takt();
        let punkte = u64::from(ende.unwrap_or(spiel.peek().punkte));
        if punkte > *rekord.peek() && (ende.is_some() || spiel.peek().zustand == Zustand::Geschafft) {
            rekord.set(punkte);
            speicher_schreiben(SPEICHER, &json!(punkte));
        }
    });
    use_tasten(move |e| {
        if e.code() == "Space" {
            e.prevent_default();
            spiel.write().umschalten();
            return;
        }
        if let Some(r) = richtung(&e.key()) {
            if spiel.peek().zustand == Zustand::Laeuft {
                e.prevent_default();
                spiel.write().steuern(r);
            }
        }
    });
    let mut knopf = move |r: Pos| {
        let z = spiel.peek().zustand;
        if matches!(z, Zustand::Laeuft | Zustand::Bereit) {
            spiel.write().steuern(r);
        } else {
            spiel.write().umschalten();
        }
    };

    let s = spiel.read();
    let rechts = rsx! { "Punkte: " b { "{s.punkte}" } " · Leben: " b { "{s.leben}" } " · Level " b { "{s.level}" } };
    let (rx, ry) = mitte(s.rabe);
    let winkel = match s.blick {
        (0, -1) => -90,
        (0, 1) => 90,
        (-1, 0) => 180,
        _ => 0,
    };
    // Nach links schauend spiegeln statt auf den Kopf stellen.
    let drehung = if winkel == 180 { "scale(-1,1)".to_string() } else { format!("rotate({winkel})") };
    let nebel = s.nebel > 0;
    let nebel_breite = s.nebel * (W * K) as u32 / NEBEL_TAKTE;
    rsx! {
        ModulCss { name: "labyrinth", css: CSS }
        Kopf { titel: "Labyrinth-Jagd", rechts }
        svg {
            class: "spiel-flaeche lj-flaeche",
            width: "{W * K}",
            height: "{H * K}",
            view_box: "0 0 {W * K} {H * K}",
            onclick: move |_| {
                if spiel.peek().zustand != Zustand::Laeuft {
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
                    if spiel.peek().zustand != Zustand::Laeuft {
                        spiel.write().umschalten();
                    }
                    return;
                }
                let r = if dx.abs() > dy.abs() { (dx.signum() as i32, 0) } else { (0, dy.signum() as i32) };
                spiel.write().steuern(r);
            },
            rect { width: "100%", height: "100%", fill: "#17181b" }
            path { d: "{s.wand_pfad}", fill: "#2f3a4a", stroke: "#3f4f66", stroke_width: "1" }
            // Sterne
            for (i, _) in s.sterne.iter().enumerate().filter(|(_, st)| **st) {
                polygon {
                    key: "{i}",
                    points: STERN,
                    transform: "translate({mitte((i as i32 % W, i as i32 / W)).0} {mitte((i as i32 % W, i as i32 / W)).1}) scale(.55)",
                    fill: if nebel { "#c58af9" } else { "#fdd663" },
                }
            }
            // Nebelfedern
            for (n, f) in s.federn.iter().enumerate() {
                g {
                    key: "f{n}",
                    class: "lj-feder",
                    transform: "translate({mitte(*f).0} {mitte(*f).1}) rotate(-35)",
                    path { d: "M0 -8 C5 -4 5 4 0 8 C-5 4 -5 -4 0 -8 Z", fill: "#8ab4f8" }
                    path { d: "M0 -8 L0 9", stroke: "#e8f0fe", stroke_width: "1" }
                }
            }
            // Waechter-Drohnen
            for (n, d) in s.drohnen.iter().enumerate() {
                g {
                    key: "d{n}",
                    class: "lj-drohne",
                    style: "transform: translate({mitte(d.pos).0}px, {mitte(d.pos).1}px)",
                    for (ox, oy) in [(-7, -7), (7, -7), (-7, 7), (7, 7)] {
                        circle { cx: "{ox}", cy: "{oy}", r: "3", fill: "none", stroke: "#9aa0a6", stroke_width: "1.2" }
                    }
                    rect { x: "-6", y: "-6", width: "12", height: "12", rx: "3", fill: "#5f6368" }
                    circle { r: "3", fill: if nebel { "#9aa0a6" } else { "#ff6d5e" } }
                }
            }
            // Der Rabe
            g {
                class: "lj-figur",
                style: "transform: translate({rx}px, {ry}px)",
                g { transform: "{drehung}", opacity: if nebel { ".35" } else { "1" },
                    polygon { points: "-6,-3 -11,-6 -10,0 -11,6 -6,3", fill: "#2b2d33" }
                    ellipse { rx: "7.5", ry: "5.5", fill: "#2b2d33", stroke: "#8ab4f8", stroke_width: "1" }
                    path { d: "M-4 -1 Q0 -7 4 -1", fill: "none", stroke: "#5f6b7d", stroke_width: "1.4" }
                    polygon { points: "6,-2 11,0 6,2", fill: "#fbbc04" }
                    circle { cx: "3.5", cy: "-2", r: "1.4", fill: "#fff" }
                }
            }
            if nebel {
                rect { x: "0", y: "0", width: "{nebel_breite}", height: "3", fill: "#8ab4f8" }
            }
            if s.zustand != Zustand::Laeuft {
                rect { width: "100%", height: "100%", fill: "rgba(0,0,0,.55)" }
                text { x: "{W * K / 2}", y: "{H * K / 2}", fill: "#fff", font_size: "20", font_family: "Arial", text_anchor: "middle", dominant_baseline: "middle", "{s.meldung()}" }
            }
        }
        div { class: "lj-knoepfe",
            button { r#type: "button", class: "hoch", onclick: move |_| knopf((0, -1)), "↑" }
            button { r#type: "button", class: "links", onclick: move |_| knopf((-1, 0)), "←" }
            button { r#type: "button", class: "runter", onclick: move |_| knopf((0, 1)), "↓" }
            button { r#type: "button", class: "rechts", onclick: move |_| knopf((1, 0)), "→" }
        }
        div { class: "w-text",
            "Sammle alle Sterne, weiche den Drohnen aus. Feder = Nebel: unsichtbar, Sterne doppelt. Rekord: "
            b { "{rekord}" }
        }
        div { class: "w-text", "Pfeiltasten oder WASD · Leertaste: Start/Pause · am Handy wischen oder Knöpfe" }
    }
}
