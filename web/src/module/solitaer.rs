//! Modul "solitaer": Solitaer (Klondike) - Karte anklicken, dann das Ziel; Doppelklick legt sie
//! auf die Ablage. Eine Karte vom Stapel je Klick. Portierung von public/module/solitaer.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, zufall};
use dioxus::prelude::*;
use serde_json::Value;

const CSS: &str = r#"
.so-tisch { background: #0b6b3a; border-radius: var(--r); padding: 12px; user-select: none; }
.so-oben, .so-reihen { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 8px; align-items: start; }
.so-reihen { margin-top: 14px; min-height: 380px; }
.so-platz, .so-reihe { position: relative; aspect-ratio: 5 / 7; border-radius: 6px; }
.so-reihe { align-self: start; }
.so-platz { border: 1px dashed #ffffff55; display: grid; place-items: center; }
.so-reihe { border: 1px dashed #ffffff33; }
.so-leer { color: #ffffff88; font-size: 22px; }
.so-karte { position: absolute; left: 0; right: 0; top: 0; aspect-ratio: 5 / 7; border-radius: 6px; cursor: pointer; box-shadow: 0 1px 2px #0006; }
.so-karte.zu { background: repeating-linear-gradient(45deg, #1a5ad7 0 6px, #1749b0 6px 12px); border: 2px solid #fff; }
.so-karte.offen { background: #fff; color: #202124; border: 1px solid #ccc; padding: 2px 4px; font: 600 14px Arial; }
.so-karte.offen b { position: absolute; right: 6px; bottom: 2px; font-size: 26px; }
.so-karte.rot { color: #d93025; }
.so-karte.gewaehlt { outline: 3px solid #fbbc04; }
@media (max-width: 600px) { .so-karte.offen { font-size: 11px; } .so-karte.offen b { font-size: 16px; } }
"#;

const FARBEN: [&str; 4] = ["♠", "♥", "♦", "♣"];
const WERTE: [&str; 13] = ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "B", "D", "K"];

#[derive(Clone, Copy, PartialEq)]
struct Karte {
    f: usize,
    w: usize,
    offen: bool,
}

impl Karte {
    fn rot(&self) -> bool {
        self.f == 1 || self.f == 2
    }
}

/// Ein Ort auf dem Tisch.
#[derive(Clone, Copy, PartialEq)]
enum Ort {
    Stapel,
    Ablage,
    Ziel(usize),
    Reihe(usize),
}

/// Der Spielstand.
#[derive(Clone)]
struct Stand {
    stapel: Vec<Karte>,
    ablage: Vec<Karte>,
    ziel: [Vec<Karte>; 4],
    reihen: [Vec<Karte>; 7],
    zuege: u32,
}

impl Stand {
    fn neu() -> Self {
        let mut deck: Vec<Karte> = (0..4).flat_map(|f| (0..13).map(move |w| Karte { f, w, offen: false })).collect();
        for i in (1..deck.len()).rev() {
            let j = zufall(i as u32 + 1) as usize;
            deck.swap(i, j);
        }
        let mut reihen: [Vec<Karte>; 7] = Default::default();
        for (r, reihe) in reihen.iter_mut().enumerate() {
            *reihe = deck.drain(..=r).collect();
            reihe[r].offen = true;
        }
        Stand { stapel: deck, ablage: Vec::new(), ziel: Default::default(), reihen, zuege: 0 }
    }
    fn liste(&mut self, ort: Ort) -> &mut Vec<Karte> {
        match ort {
            Ort::Stapel => &mut self.stapel,
            Ort::Ablage => &mut self.ablage,
            Ort::Ziel(n) => &mut self.ziel[n],
            Ort::Reihe(n) => &mut self.reihen[n],
        }
    }
    /// Darf die Karte (oder der Stapel ab ihr) dorthin?
    fn passt(&mut self, k: Karte, ziel: Ort, anzahl: usize) -> bool {
        match ziel {
            Ort::Ziel(_) => {
                if anzahl != 1 {
                    return false;
                }
                match self.liste(ziel).last() {
                    Some(o) => o.f == k.f && o.w + 1 == k.w,
                    None => k.w == 0,
                }
            }
            Ort::Reihe(_) => match self.liste(ziel).last() {
                Some(o) => o.offen && o.rot() != k.rot() && o.w == k.w + 1,
                None => k.w == 12,
            },
            _ => false,
        }
    }
    fn ziehen(&mut self, von: Ort, i: usize, nach: Ort) -> bool {
        let quelle = self.liste(von);
        if i >= quelle.len() {
            return false;
        }
        let (k, anzahl) = (quelle[i], quelle.len() - i);
        if !self.passt(k, nach, anzahl) {
            return false;
        }
        let teil: Vec<Karte> = self.liste(von).drain(i..).collect();
        self.liste(nach).extend(teil);
        if let Ort::Reihe(_) = von {
            if let Some(o) = self.liste(von).last_mut() {
                o.offen = true;
            }
        }
        self.zuege += 1;
        true
    }
    fn auf_ablage(&mut self, ort: Ort, i: usize) -> bool {
        if i + 1 != self.liste(ort).len() {
            return false;
        }
        (0..4).any(|n| self.ziehen(ort, i, Ort::Ziel(n)))
    }
    fn vom_stapel(&mut self) {
        if let Some(mut k) = self.stapel.pop() {
            k.offen = true;
            self.ablage.push(k);
        } else {
            self.stapel = self.ablage.drain(..).rev().map(|k| Karte { offen: false, ..k }).collect();
        }
    }
}

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = (&ctx, &daten);
    let mut stand = use_signal(Stand::neu);
    let mut wahl = use_signal(|| None::<(Ort, usize)>);

    // Ein Klick auf einen Ort (i = Karte darin, None = leere Stelle).
    let mut klick = move |ort: Ort, i: Option<usize>| {
        if ort == Ort::Stapel {
            stand.write().vom_stapel();
            wahl.set(None);
            return;
        }
        if let Some((w_ort, w_i)) = wahl() {
            if w_ort != ort {
                stand.write().ziehen(w_ort, w_i, ort);
            }
            wahl.set(None);
        } else if let Some(i) = i {
            if stand.write().liste(ort).get(i).is_some_and(|k| k.offen) {
                wahl.set(Some((ort, i)));
            }
        }
    };
    let mut doppelt = move |ort: Ort, i: usize| {
        wahl.set(None);
        stand.write().auf_ablage(ort, i);
    };

    let s = stand.read();
    let fertig = s.ziel.iter().all(|z| z.len() == 13);
    let status = if fertig { format!("Gewonnen in {} Zügen! 🎉", s.zuege) } else { format!("{} Züge", s.zuege) };
    let gewaehlt = move |ort: Ort, i: usize| wahl().is_some_and(|(o, w)| o == ort && i >= w);

    // Eine Karte zeichnen.
    let karte = move |k: Karte, ort: Ort, i: usize, oben: Option<usize>| {
        let stil = oben.map(|t| format!("top:{}px", t * 22)).unwrap_or_default();
        if k.offen {
            let klasse = format!("so-karte offen{}{}", if k.rot() { " rot" } else { "" }, if gewaehlt(ort, i) { " gewaehlt" } else { "" });
            rsx! {
                div {
                    key: "{i}",
                    class: "{klasse}",
                    style: "{stil}",
                    onclick: move |e| {
                        e.stop_propagation();
                        klick(ort, Some(i));
                    },
                    ondoubleclick: move |e| {
                        e.stop_propagation();
                        doppelt(ort, i);
                    },
                    span { "{WERTE[k.w]}{FARBEN[k.f]}" }
                    b { "{FARBEN[k.f]}" }
                }
            }
        } else {
            rsx! {
                div {
                    key: "{i}",
                    class: "so-karte zu",
                    style: "{stil}",
                    onclick: move |e| {
                        e.stop_propagation();
                        klick(ort, Some(i));
                    },
                }
            }
        }
    };

    let neu_knopf = rsx! {
        button {
            r#type: "button",
            class: "w-knopf klein",
            onclick: move |e| {
                e.stop_propagation();
                stand.set(Stand::neu());
                wahl.set(None);
            },
            "Neues Spiel"
        }
    };

    rsx! {
        ModulCss { name: "solitaer", css: CSS }
        div { onclick: move |_| wahl.set(None),
            Kopf { titel: "Solitär", rechts: neu_knopf }
            div { class: "so-tisch",
                div { class: "so-oben",
                    div {
                        class: "so-platz",
                        onclick: move |e| {
                            e.stop_propagation();
                            klick(Ort::Stapel, None);
                        },
                        if s.stapel.is_empty() {
                            span { class: "so-leer", "↺" }
                        } else {
                            div { class: "so-karte zu" }
                        }
                    }
                    div { class: "so-platz",
                        if let Some(&k) = s.ablage.last() {
                            {karte(k, Ort::Ablage, s.ablage.len() - 1, None)}
                        }
                    }
                    div {}
                    for n in 0..4 {
                        div {
                            key: "{n}",
                            class: "so-platz",
                            onclick: move |e| {
                                e.stop_propagation();
                                klick(Ort::Ziel(n), None);
                            },
                            if let Some(&k) = s.ziel[n].last() {
                                {karte(k, Ort::Ziel(n), s.ziel[n].len() - 1, None)}
                            } else {
                                span { class: "so-leer", "{FARBEN[n]}" }
                            }
                        }
                    }
                }
                div { class: "so-reihen",
                    for n in 0..7 {
                        div {
                            key: "{n}",
                            class: "so-reihe",
                            onclick: move |e| {
                                e.stop_propagation();
                                klick(Ort::Reihe(n), None);
                            },
                            for (i, &k) in s.reihen[n].iter().enumerate() {
                                {karte(k, Ort::Reihe(n), i, Some(i))}
                            }
                        }
                    }
                }
            }
            div { class: "w-text so-status", "{status}" }
        }
    }
}
