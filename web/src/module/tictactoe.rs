//! Modul "tictactoe": Tic-Tac-Toe gegen den Computer (unschlagbar oder leicht) oder zu zweit.
//! Portierung von public/module/tictactoe.js.

use super::ModulCtx;
use crate::werkzeug::{Kopf, ModulCss, zufall};
use dioxus::prelude::*;
use serde_json::Value;

const CSS: &str = r#"
.ttt-feld { display: grid; grid-template-columns: repeat(3, 84px); gap: 6px; margin: 12px 0; }
.ttt-feld button { width: 84px; height: 84px; font: 500 44px Arial; border: 0; border-radius: var(--r); background: var(--flaeche2); color: var(--schrift); cursor: pointer; }
.ttt-feld button.X { color: #4285f4; }
.ttt-feld button.O { color: #ea4335; }
.ttt-feld button.sieg { background: var(--flaeche3); }
"#;

const LINIEN: [[usize; 3]; 8] = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [0, 3, 6], [1, 4, 7], [2, 5, 8], [0, 4, 8], [2, 4, 6]];

/// Ein Feld: ' ' = leer, sonst 'X' oder 'O'.
type Feld = [char; 9];

fn gewinner(f: &Feld) -> Option<[usize; 3]> {
    LINIEN.into_iter().find(|&[a, b, c]| f[a] != ' ' && f[a] == f[b] && f[a] == f[c])
}

fn voll(f: &Feld) -> bool {
    f.iter().all(|&c| c != ' ')
}

/// Minimax - der Computer spielt O.
fn bewerten(f: &mut Feld, zug: char, tiefe: i32) -> i32 {
    if let Some(g) = gewinner(f) {
        return if f[g[0]] == 'O' { 10 - tiefe } else { tiefe - 10 };
    }
    if voll(f) {
        return 0;
    }
    let mut best = if zug == 'O' { i32::MIN } else { i32::MAX };
    for i in 0..9 {
        if f[i] != ' ' {
            continue;
        }
        f[i] = zug;
        let w = bewerten(f, if zug == 'O' { 'X' } else { 'O' }, tiefe + 1);
        f[i] = ' ';
        best = if zug == 'O' { best.max(w) } else { best.min(w) };
    }
    best
}

fn bester_zug(f: &Feld) -> usize {
    let mut feld = *f;
    let (mut best, mut zug) = (i32::MIN, 0);
    for i in 0..9 {
        if feld[i] != ' ' {
            continue;
        }
        feld[i] = 'O';
        let w = bewerten(&mut feld, 'X', 0);
        feld[i] = ' ';
        if w > best {
            best = w;
            zug = i;
        }
    }
    zug
}

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = (&ctx, &daten);
    let mut feld = use_signal(|| [' '; 9]);
    let mut dran = use_signal(|| 'X');
    let mut modus = use_signal(|| "schwer".to_string());

    let mut neu = move || {
        feld.set([' '; 9]);
        dran.set('X');
    };
    let mut setzen = move |i: usize| {
        let mut f = feld();
        if f[i] != ' ' || gewinner(&f).is_some() {
            return;
        }
        f[i] = dran();
        let mut d = if dran() == 'X' { 'O' } else { 'X' };
        let m = modus();
        if m != "zwei" && d == 'O' && gewinner(&f).is_none() && !voll(&f) {
            let frei: Vec<usize> = (0..9).filter(|&j| f[j] == ' ').collect();
            let ziel = if m == "leicht" && zufall(2) == 1 { frei[zufall(frei.len() as u32) as usize] } else { bester_zug(&f) };
            f[ziel] = 'O';
            d = 'X';
        }
        feld.set(f);
        dran.set(d);
    };

    let f = feld();
    let g = gewinner(&f);
    let status = match g {
        Some(l) => format!("{} gewinnt!", f[l[0]]),
        None if voll(&f) => "Unentschieden".into(),
        None => format!("{} ist dran", dran()),
    };
    rsx! {
        ModulCss { name: "tictactoe", css: CSS }
        Kopf { titel: "Tic-Tac-Toe" }
        div { class: "w-zeile",
            select {
                class: "w-eingabe ttt-modus",
                value: "{modus}",
                onchange: move |e| {
                    modus.set(e.value());
                    neu();
                },
                option { value: "schwer", "gegen Computer (unschlagbar)" }
                option { value: "leicht", "gegen Computer (leicht)" }
                option { value: "zwei", "zu zweit" }
            }
            button { r#type: "button", class: "w-knopf", onclick: move |_| neu(), "Neues Spiel" }
        }
        div { class: "ttt-feld",
            for i in 0..9 {
                button {
                    key: "{i}",
                    r#type: "button",
                    class: format!("{}{}", if f[i] == ' ' { String::new() } else { f[i].to_string() }, if g.is_some_and(|l| l.contains(&i)) { " sieg" } else { "" }),
                    onclick: move |_| setzen(i),
                    if f[i] != ' ' { "{f[i]}" }
                }
            }
        }
        div { class: "ttt-status w-gross", "{status}" }
    }
}
