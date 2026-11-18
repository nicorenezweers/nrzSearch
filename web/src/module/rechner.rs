//! Modul "rechner": Fragen wie "13134+134134" bekommen eine Rechentafel wie bei Google. Sie
//! rechnet selbst (ohne neue Suche), mit Winkelfunktionen, Fakultaet, Prozent, pi und e - und
//! rechnet beim Tippen im Suchfeld mit (`state::EINGABE`).
//! "rechner", "taschenrechner", "calculator" … oeffnen eine leere Tafel - auch mit Tippfehlern
//! ("recher", "taschenrechenr"). Die Woerter stehen in module.json (woerter), ebenso die
//! Fuellwoerter, die nicht zaehlen (fuell).

use super::ModulCtx;
use crate::bib::mathe::{Werte, anzeige, rechne, zahl_text};
use crate::state::EINGABE;
use crate::werkzeug::{ModulCss, kopieren, re_treffer};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::cell::RefCell;

const TASTEN: [&str; 33] = [
    "x!", "(", ")", "%", "AC", "Inv", "sin", "ln", "7", "8", "9", "÷", "π", "cos", "log", "4", "5", "6", "×", "e", "tan", "√", "1", "2", "3", "−",
    "Ans", "EXP", "x^y", "0", ",", "=", "+",
];

fn titel(t: &str) -> Option<&'static str> {
    match t {
        "x!" => Some("Fakultät"),
        "x^y" => Some("Potenz"),
        "," => Some("Dezimaltrennzeichen"),
        _ => None,
    }
}

/// Was eine Taste an den Ausdruck haengt (sonst ihr eigenes Zeichen).
fn eingabe_von(t: &str) -> &str {
    match t {
        "x^y" => "^",
        "EXP" => "e^",
        "√" => "√(",
        "x!" => "!",
        "Ans" => "ans",
        "sin" => "sin(",
        "cos" => "cos(",
        "tan" => "tan(",
        "ln" => "ln(",
        "log" => "log(",
        t => t,
    }
}

const NAMEN: &str = r"\b(sin|cos|tan|asin|acos|atan|sinh|cosh|tanh|ln|log|sqrt|exp|abs|root|pi|ans)\b";

/// Der Zustand der Tafel - bleibt (wie im Original) zwischen zwei Suchen erhalten.
#[derive(Clone, Debug, PartialEq)]
struct Tafel {
    ausdruck: String,
    deg: bool,
    inv: bool,
    ans: f64,
    /// Nach "=": eine Ziffer beginnt neu, ein Operator rechnet weiter.
    fest: bool,
    verlauf: Vec<(String, f64)>,
}

impl Default for Tafel {
    fn default() -> Self {
        Self { ausdruck: String::new(), deg: true, inv: false, ans: 0.0, fest: false, verlauf: Vec::new() }
    }
}

impl Tafel {
    fn werte(&self) -> Werte {
        Werte { deg: self.deg, inv: self.inv, ans: self.ans, ..Werte::default() }
    }
}

thread_local! {
    static GEDAECHTNIS: RefCell<Tafel> = RefCell::new(Tafel::default());
}

fn merken(t: &Tafel) {
    GEDAECHTNIS.with(|g| *g.borrow_mut() = t.clone());
}

/// Ersetzt alle Treffer eines JS-Regex (Flags mit "g") durch `durch`.
fn ersetzen(text: &str, muster: &str, flags: &str, durch: &str) -> String {
    let re = js_sys::RegExp::new(muster, flags);
    js_sys::JsString::from(text).replace_by_pattern(&re, durch).into()
}

/// Ist die Frage eine Rechenaufgabe? Ziffern und ein Operator (oder eine Funktion), sonst nur
/// Funktionsnamen - und sie muss sich rechnen lassen.
fn ist_rechenfrage(t: &str) -> bool {
    if t.encode_utf16().count() > 120 || !t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if !t.chars().any(|c| "+-*/^%!√π×÷".contains(c)) && re_treffer(r"\b(sin|cos|tan|ln|log|sqrt|exp|abs|root)\s*\(", "i", t).is_none() {
        return false;
    }
    let rest = ersetzen(&ersetzen(t, NAMEN, "gi", ""), "[0-9]e[+-]?[0-9]+", "gi", "");
    if rest.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let werte = GEDAECHTNIS.with(|g| g.borrow().werte());
    rechne(t, &werte).ok
}

/// Wie viele Tippfehler trennen a und b? (Vertauschte Buchstaben zaehlen einmal.)
fn abstand(a: &[char], b: &[char]) -> usize {
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, zeile) in d.iter_mut().enumerate() {
        zeile[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let anders = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + anders);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// Meint die Frage den Rechner selbst? Kurze Woerter muessen stimmen, bis 7 Buchstaben darf
/// einer falsch sein, laengere zwei.
fn ist_rechner_wort(frage: &str, ctx: &ModulCtx) -> bool {
    let liste = |k: &str| -> Vec<String> {
        ctx.cfg(k).as_array().map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default()
    };
    let fuell = liste("fuell");
    let mut woerter = liste("woerter");
    if ctx.cfg("woerter").as_array().is_none() {
        woerter = vec!["rechner".into()];
    }
    let klein = frage.to_lowercase();
    let t: String = klein.split(|c: char| c.is_whitespace() || c == '-').filter(|w| !w.is_empty() && !fuell.iter().any(|f| f == w)).collect();
    let t: Vec<char> = t.chars().collect();
    if t.len() < 3 || t.len() > 30 {
        return false;
    }
    woerter.iter().any(|w| {
        let w: Vec<char> = w.chars().collect();
        let erlaubt = if w.len() <= 4 { 0 } else if w.len() <= 7 { 1 } else { 2 };
        abstand(&t, &w) <= erlaubt
    })
}

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    let frage = ctx.frage.as_str();
    if ist_rechenfrage(frage) {
        return Some(json!({ "ausdruck": frage }));
    }
    // "rechner 5+3" - das Wort vorne, die Aufgabe dahinter.
    let teile: Vec<&str> = frage.split_whitespace().collect();
    if teile.len() > 1 {
        let rest = teile[1..].join(" ");
        if ist_rechner_wort(teile[0], ctx) && ist_rechenfrage(&rest) {
            return Some(json!({ "ausdruck": rest }));
        }
    }
    ist_rechner_wort(frage, ctx).then(|| json!({ "ausdruck": "" }))
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &ctx;
    let mut z = use_signal(|| {
        let mut t = GEDAECHTNIS.with(|g| g.borrow().clone());
        t.ausdruck = daten.get("ausdruck").and_then(Value::as_str).unwrap_or_default().to_string();
        t.fest = false;
        merken(&t);
        t
    });
    // Tippt man im Suchfeld, rechnet die Tafel sofort mit (erst nach dem Aufbau).
    let mut gesehen = use_signal(|| EINGABE.peek().clone());
    use_effect(move || {
        let text = EINGABE();
        if text == *gesehen.peek() {
            return;
        }
        gesehen.set(text.clone());
        let mut t = z.write();
        t.ausdruck = if text.chars().any(|c| c.is_ascii_digit()) { text.trim().to_string() } else { String::new() };
        t.fest = false;
        merken(&t);
    });

    let taste = move |k: &str| {
        // Kopien der Signale - so bleibt die Closure `Fn` und `Copy`.
        let (mut z, mut gesehen) = (z, gesehen);
        let mut t = z.write();
        match k {
            "Deg" | "Rad" => t.deg = k == "Deg",
            "Inv" => t.inv = !t.inv,
            "AC" => {
                t.ausdruck.clear();
                t.fest = false;
            }
            "=" => {
                let e = rechne(&t.ausdruck, &t.werte());
                if !e.ok {
                    return;
                }
                t.ans = e.wert;
                let eintrag = (t.ausdruck.clone(), e.wert);
                t.verlauf.insert(0, eintrag);
                t.verlauf.truncate(6);
                t.fest = true;
            }
            _ => {
                if t.fest {
                    if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) {
                        t.ausdruck.clear();
                    } else if matches!(k, "-" | "+" | "−" | "×" | "÷" | "%" | "x^y") {
                        t.ausdruck = zahl_text(t.ans);
                    }
                    t.fest = false;
                }
                t.ausdruck.push_str(eingabe_von(k));
            }
        }
        merken(&t);
        // Den Ausdruck ins Suchfeld schreiben - ohne dass die Tafel ihn neu einliest.
        if !t.ausdruck.is_empty() {
            let a = t.ausdruck.clone();
            drop(t);
            gesehen.set(a.clone());
            *EINGABE.write() = a;
        }
    };

    let t = z();
    let ergebnis = (!t.ausdruck.is_empty()).then(|| rechne(&t.ausdruck, &t.werte()));
    let zeile = if t.ausdruck.is_empty() { String::new() } else { format!("{} =", anzeige(&t.ausdruck)) };
    let wert = match ergebnis {
        Some(e) if e.ok => zahl_text(e.wert),
        Some(_) => "Fehler".to_string(),
        None => "0".to_string(),
    };
    let fehler = ergebnis.is_some_and(|e| !e.ok);
    let aktiv = |an: bool| if an { "aktiv" } else { "" };

    rsx! {
        ModulCss { name: "rechner".to_string(), css: CSS }
        div { class: "r-anzeige",
            div { class: "r-verlauf",
                for (i, (a, w)) in t.verlauf.iter().cloned().enumerate() {
                    button {
                        key: "{i}",
                        r#type: "button",
                        onclick: move |_| {
                            let mut t = z.write();
                            t.ausdruck = a.clone();
                            t.fest = false;
                            merken(&t);
                        },
                        "{anzeige(&a)} = "
                        b { "{zahl_text(w)}" }
                    }
                }
            }
            div { class: "r-zeile", "{zeile}" }
            div {
                class: if fehler { "r-wert fehler" } else { "r-wert" },
                title: "Zum Kopieren klicken",
                onclick: {
                    let wert = wert.clone();
                    move |_| {
                        let wert = wert.clone();
                        spawn(async move {
                            kopieren(&wert).await;
                        });
                    }
                },
                "{wert}"
            }
        }
        div { class: "r-tasten",
            span { class: "r-modus",
                button { r#type: "button", class: aktiv(!t.deg), onclick: move |_| taste("Rad"), "Rad" }
                button { r#type: "button", class: aktiv(t.deg), onclick: move |_| taste("Deg"), "Deg" }
            }
            for k in TASTEN {
                button {
                    key: "{k}",
                    r#type: "button",
                    class: if k == "=" { "gleich" } else if k == "Inv" { aktiv(t.inv) } else if k.len() == 1 && (k == "," || k.chars().all(|c| c.is_ascii_digit())) { "ziffer" } else { "" },
                    title: titel(k),
                    onclick: move |_| taste(k),
                    "{k}"
                }
            }
        }
    }
}

const CSS: &str = r#"
.r-anzeige { border: 1px solid var(--kante); border-radius: var(--r); padding: 10px 14px; margin-bottom: 12px; text-align: right; min-height: 88px; position: relative; }
.r-verlauf { position: absolute; left: 10px; top: 8px; display: flex; flex-direction: column; align-items: flex-start; gap: 2px; max-width: 50%; }
.r-verlauf button { font: inherit; font-size: 12px; color: var(--gedimmt); background: none; border: 0; padding: 0; cursor: pointer; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
.r-verlauf b { color: var(--schrift); font-weight: 400; }
.r-zeile { font-size: 14px; color: var(--gedimmt); min-height: 20px; }
.r-wert { font-size: 30px; line-height: 1.3; cursor: copy; overflow: hidden; text-overflow: ellipsis; }
.r-wert.fehler { color: var(--rot); font-size: 20px; }
.r-tasten { display: grid; grid-template-columns: repeat(7, 1fr); gap: 8px; }
.r-tasten button { font: inherit; font-size: 14px; height: 36px; border: 0; border-radius: var(--r2); background: var(--flaeche3); color: var(--schrift); cursor: pointer; }
.r-tasten button.ziffer { background: var(--flaeche2); }
.r-tasten button:hover { filter: brightness(1.12); }
.r-tasten button.gleich { background: var(--hoch); color: var(--auf-hoch); }
.r-tasten button.aktiv { color: var(--hoch); }
.r-modus { grid-column: span 2; display: flex; }
.r-modus button { flex: 1; border-radius: 0; }
.r-modus button:first-child { border-radius: var(--r2) 0 0 var(--r2); border-right: 1px solid var(--kante); }
.r-modus button:last-child { border-radius: 0 var(--r2) var(--r2) 0; }
@media (max-width: 600px) { .r-tasten { grid-template-columns: repeat(4, 1fr); } }
"#;
