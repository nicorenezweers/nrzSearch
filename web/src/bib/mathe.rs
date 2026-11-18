//! Der Formel-Rechner fuer Rechner und Funktions-Plotter - Portierung von public/lib/mathe.js.
//! Rekursiver Abstieg: + - * / ^ ! %, Klammern, Funktionen, pi, e, ans und x.
//! Reines Rust ohne web-sys (laeuft auch nativ, siehe Tests unten).

use std::f64::consts::{E, PI};

const GRAD: f64 = PI / 180.0;

/// Die Werte, mit denen eine Formel rechnet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Werte {
    /// Die Variable x (fehlt sie, ist sie NaN - wie `undefined` im Original).
    pub x: f64,
    /// Winkel in Grad (true) oder im Bogenmass (false).
    pub deg: bool,
    /// Umkehrfunktionen (Taste "Inv").
    pub inv: bool,
    /// Das letzte Ergebnis ("ans").
    pub ans: f64,
}

impl Default for Werte {
    fn default() -> Self {
        Self { x: f64::NAN, deg: true, inv: false, ans: 0.0 }
    }
}

/// Die eingebauten Funktionen.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fun {
    Sin, Cos, Tan, Asin, Acos, Atan, Sinh, Cosh, Tanh, Ln, Log, Sqrt, Exp, Abs, Floor, Ceil, Round, Ncr,
}

impl Fun {
    fn finden(name: &str) -> Option<Fun> {
        Some(match name {
            "sin" => Fun::Sin,
            "cos" => Fun::Cos,
            "tan" => Fun::Tan,
            "asin" => Fun::Asin,
            "acos" => Fun::Acos,
            "atan" => Fun::Atan,
            "sinh" => Fun::Sinh,
            "cosh" => Fun::Cosh,
            "tanh" => Fun::Tanh,
            "ln" => Fun::Ln,
            "log" => Fun::Log,
            "sqrt" | "root" => Fun::Sqrt,
            "exp" => Fun::Exp,
            "abs" => Fun::Abs,
            "floor" => Fun::Floor,
            "ceil" => Fun::Ceil,
            "round" => Fun::Round,
            "ncr" => Fun::Ncr,
            _ => return None,
        })
    }

    fn rechnen(self, args: &[f64], w: &Werte) -> f64 {
        let x = args.first().copied().unwrap_or(f64::NAN);
        let deg = w.deg;
        let rein = |x: f64| if deg { x * GRAD } else { x };
        let raus = |x: f64| if deg { x / GRAD } else { x };
        match (self, w.inv) {
            (Fun::Sin, false) => rein(x).sin(),
            (Fun::Sin, true) | (Fun::Asin, false) => raus(x.asin()),
            (Fun::Cos, false) => rein(x).cos(),
            (Fun::Cos, true) | (Fun::Acos, false) => raus(x.acos()),
            (Fun::Tan, false) => rein(x).tan(),
            (Fun::Tan, true) | (Fun::Atan, false) => raus(x.atan()),
            (Fun::Asin, true) => rein(x).sin(),
            (Fun::Acos, true) => rein(x).cos(),
            (Fun::Atan, true) => rein(x).tan(),
            (Fun::Sinh, false) => x.sinh(),
            (Fun::Sinh, true) => x.asinh(),
            (Fun::Cosh, false) => x.cosh(),
            (Fun::Cosh, true) => x.acosh(),
            (Fun::Tanh, false) => x.tanh(),
            (Fun::Tanh, true) => x.atanh(),
            (Fun::Ln, false) | (Fun::Exp, true) => x.ln(),
            (Fun::Ln, true) | (Fun::Exp, false) => x.exp(),
            (Fun::Log, false) => x.log10(),
            (Fun::Log, true) => 10f64.powf(x),
            (Fun::Sqrt, false) => x.sqrt(),
            (Fun::Sqrt, true) => x * x,
            (Fun::Abs, _) => x.abs(),
            (Fun::Floor, false) | (Fun::Ceil, true) => x.floor(),
            (Fun::Ceil, false) | (Fun::Floor, true) => x.ceil(),
            (Fun::Round, _) => js_round(x),
            // Ohne zweites Argument rechnet das Original mit "deg" als k (true = 1).
            (Fun::Ncr, _) => binomial(x, args.get(1).copied().unwrap_or(if deg { 1.0 } else { 0.0 })),
        }
    }
}

/// Math.round wie in JavaScript (halbe Werte nach oben).
fn js_round(x: f64) -> f64 {
    if !x.is_finite() || x.fract() == 0.0 { x } else { (x + 0.5).floor() }
}

fn binomial(n: f64, k: f64) -> f64 {
    if k < 0.0 || k > n {
        return f64::NAN;
    }
    let mut e = 1.0;
    let mut i = 1.0;
    while i <= k {
        e = (e * (n - k + i)) / i;
        i += 1.0;
    }
    js_round(e)
}

fn fakultaet(n: f64) -> f64 {
    if n < 0.0 {
        return f64::NAN;
    }
    let mut e = 1.0;
    let mut i = 2.0;
    while i <= n {
        e *= i;
        i += 1.0;
    }
    e
}

/// Ein uebersetzter Ausdruck - einmal uebersetzen, oft rechnen (Graph).
#[derive(Clone, Debug, PartialEq)]
pub enum Formel {
    Zahl(f64),
    X,
    Ans,
    Neg(Box<Formel>),
    Plus(Box<Formel>, Box<Formel>),
    Minus(Box<Formel>, Box<Formel>),
    Mal(Box<Formel>, Box<Formel>),
    Durch(Box<Formel>, Box<Formel>),
    Hoch(Box<Formel>, Box<Formel>),
    Fakultaet(Box<Formel>),
    Prozent(Box<Formel>),
    Funktion(Fun2, Vec<Formel>),
}

/// Eine Funktion in einer [`Formel`] (nach aussen undurchsichtig).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fun2(Fun);

impl Formel {
    /// Den Ausdruck mit den Werten rechnen.
    pub fn wert(&self, w: &Werte) -> f64 {
        match self {
            Formel::Zahl(z) => *z,
            Formel::X => w.x,
            Formel::Ans => w.ans,
            Formel::Neg(a) => -a.wert(w),
            Formel::Plus(a, b) => a.wert(w) + b.wert(w),
            Formel::Minus(a, b) => a.wert(w) - b.wert(w),
            Formel::Mal(a, b) => a.wert(w) * b.wert(w),
            Formel::Durch(a, b) => {
                let r = b.wert(w);
                if r == 0.0 { f64::NAN } else { a.wert(w) / r }
            }
            Formel::Hoch(a, b) => js_pow(a.wert(w), b.wert(w)),
            Formel::Fakultaet(a) => fakultaet(a.wert(w)),
            Formel::Prozent(a) => a.wert(w) / 100.0,
            Formel::Funktion(f, args) => {
                let werte: Vec<f64> = args.iter().map(|a| a.wert(w)).collect();
                f.0.rechnen(&werte, w)
            }
        }
    }
}

/// `a ** b` wie in JavaScript (1 ** NaN ist dort NaN, in Rust 1).
fn js_pow(a: f64, b: f64) -> f64 {
    if b.is_nan() || ((a == 1.0 || a == -1.0) && b.is_infinite()) { f64::NAN } else { a.powf(b) }
}

/// Die Anzeige in die Syntax des Rechners uebersetzen.
fn syntax(text: &str) -> Vec<char> {
    let mut aus = Vec::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '×' => aus.push('*'),
            '÷' => aus.push('/'),
            '−' => aus.push('-'),
            '√' => aus.extend("sqrt".chars()),
            'π' => aus.extend("pi".chars()),
            ',' => aus.push('.'),
            c if c.is_whitespace() => {}
            c => aus.push(c),
        }
    }
    aus
}

struct Leser {
    t: Vec<char>,
    pos: usize,
}

type Erg = Result<Formel, String>;

fn b(f: Formel) -> Box<Formel> {
    Box::new(f)
}

impl Leser {
    fn an(&self, i: usize) -> Option<char> {
        self.t.get(i).copied()
    }

    fn nimm(&mut self, c: char) -> bool {
        if self.an(self.pos) == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Steht an `i` eine Ziffernfolge? Liefert deren Ende.
    fn ziffern(&self, i: usize) -> usize {
        let mut j = i;
        while self.an(j).is_some_and(|c| c.is_ascii_digit()) {
            j += 1;
        }
        j
    }

    /// Steht an `i` ein Exponent "e+3"/"E5"? Liefert dessen Ende.
    fn exponent(&self, i: usize) -> Option<usize> {
        if !matches!(self.an(i), Some('e' | 'E')) {
            return None;
        }
        let mut j = i + 1;
        if matches!(self.an(j), Some('+' | '-')) {
            j += 1;
        }
        let ende = self.ziffern(j);
        (ende > j).then_some(ende)
    }

    fn ausdruck(&mut self) -> Erg {
        let mut w = self.term()?;
        loop {
            if self.nimm('+') {
                w = Formel::Plus(b(w), b(self.term()?));
            } else if self.nimm('-') {
                w = Formel::Minus(b(w), b(self.term()?));
            } else {
                return Ok(w);
            }
        }
    }

    fn term(&mut self) -> Erg {
        let mut w = self.potenz()?;
        loop {
            if self.nimm('*') {
                w = Formel::Mal(b(w), b(self.potenz()?));
            } else if self.nimm('/') {
                w = Formel::Durch(b(w), b(self.potenz()?));
            } else if self.an(self.pos).is_some_and(|c| c.is_ascii_alphabetic() || c == '(') && self.exponent(self.pos).is_none() {
                // "2x", "3(x+1)", "2pi" - Malpunkt weglassen wie auf Papier.
                w = Formel::Mal(b(w), b(self.potenz()?));
            } else {
                return Ok(w);
            }
        }
    }

    fn potenz(&mut self) -> Erg {
        let w = self.vorzeichen()?;
        if !self.nimm('^') {
            return Ok(w);
        }
        Ok(Formel::Hoch(b(w), b(self.potenz()?)))
    }

    fn vorzeichen(&mut self) -> Erg {
        if self.nimm('-') {
            return Ok(Formel::Neg(b(self.vorzeichen()?)));
        }
        if self.nimm('+') {
            return self.vorzeichen();
        }
        let mut w = self.grund()?;
        loop {
            if self.nimm('!') {
                w = Formel::Fakultaet(b(w));
            } else if self.nimm('%') {
                w = Formel::Prozent(b(w));
            } else {
                return Ok(w);
            }
        }
    }

    fn grund(&mut self) -> Erg {
        if self.nimm('(') {
            let w = self.ausdruck()?;
            self.nimm(')');
            return Ok(w);
        }
        // Zahl: 12, 1.5, 2e-3, .5
        let start = self.pos;
        let mut ende = self.ziffern(start);
        if ende > start {
            if self.an(ende) == Some('.') {
                let nach = self.ziffern(ende + 1);
                if nach > ende + 1 {
                    ende = nach;
                }
            }
            if let Some(e) = self.exponent(ende) {
                ende = e;
            }
        } else if self.an(start) == Some('.') && self.ziffern(start + 1) > start + 1 {
            ende = self.ziffern(start + 1);
        }
        if ende > start {
            let text: String = self.t[start..ende].iter().collect();
            self.pos = ende;
            return Ok(Formel::Zahl(text.parse().unwrap_or(f64::NAN)));
        }
        // Name: pi, ans oder Buchstaben.
        let mut j = start;
        while self.an(j).is_some_and(|c| c.is_ascii_alphabetic()) {
            j += 1;
        }
        if j == start {
            return Err("unvollständig".into());
        }
        let name: String = self.t[start..j].iter().collect::<String>().to_ascii_lowercase();
        // "xsin" ist "x * sin" - x ist immer ein eigener Name.
        let wort = if name.starts_with('x') {
            "x"
        } else if name.starts_with("pi") {
            "pi"
        } else if name.starts_with("ans") {
            "ans"
        } else {
            name.as_str()
        };
        self.pos += wort.len();
        match wort {
            "x" => return Ok(Formel::X),
            "pi" => return Ok(Formel::Zahl(PI)),
            "e" => return Ok(Formel::Zahl(E)),
            "ans" => return Ok(Formel::Ans),
            _ => {}
        }
        let f = Fun::finden(wort).ok_or_else(|| format!("unbekannt: {wort}"))?;
        let mut args = Vec::new();
        if self.nimm('(') {
            args.push(self.ausdruck()?);
            if self.nimm(';') {
                // nCr: "n über k"
                args.push(self.ausdruck()?);
            }
            self.nimm(')');
        } else {
            args.push(self.grund()?);
        }
        Ok(Formel::Funktion(Fun2(f), args))
    }
}

/// Einen Ausdruck uebersetzen. Fehler als Text.
pub fn uebersetzen(text: &str) -> Result<Formel, String> {
    let mut l = Leser { t: syntax(text), pos: 0 };
    let f = l.ausdruck()?;
    if l.pos != l.t.len() {
        return Err("unbekanntes Zeichen".into());
    }
    Ok(f)
}

/// Das Ergebnis von [`rechne`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ergebnis {
    pub ok: bool,
    pub wert: f64,
}

/// Einen Ausdruck auswerten.
pub fn rechne(text: &str, werte: &Werte) -> Ergebnis {
    match uebersetzen(text) {
        Ok(f) => {
            let wert = f.wert(werte);
            Ergebnis { ok: wert.is_finite(), wert }
        }
        Err(_) => Ergebnis { ok: false, wert: f64::NAN },
    }
}

/// `String(n)` wie in JavaScript (kuerzeste Darstellung, Exponent ab 1e21 und unter 1e-6).
pub fn js_zahl(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n == 0.0 {
        return "0".into();
    }
    let a = n.abs();
    if (1e-6..1e21).contains(&a) {
        return format!("{n}");
    }
    let s = format!("{n:e}");
    match s.split_once('e') {
        Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
        _ => s,
    }
}

/// `Number(n.toPrecision(stellen))` - auf signifikante Stellen runden.
pub fn auf_stellen(n: f64, stellen: usize) -> f64 {
    if !n.is_finite() || n == 0.0 {
        return n;
    }
    format!("{:.*e}", stellen.saturating_sub(1), n).parse().unwrap_or(n)
}

/// Die Zahl fuer die Anzeige: ohne Tausenderpunkt, wie bei Google.
pub fn zahl_text(n: f64) -> String {
    if !n.is_finite() {
        return "–".into();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return js_zahl(n);
    }
    js_zahl(auf_stellen(n, 12)).replacen('.', ",", 1)
}

/// Den Ausdruck huebsch schreiben: "13134+134134" -> "13 134 + 134 134".
pub fn anzeige(text: &str) -> String {
    // Ziffernfolgen in Dreiergruppen (von rechts).
    let zeichen: Vec<char> = text.chars().collect();
    let mut s = String::with_capacity(text.len() * 2);
    let mut i = 0;
    while i < zeichen.len() {
        if zeichen[i].is_ascii_digit() {
            let mut j = i;
            while j < zeichen.len() && zeichen[j].is_ascii_digit() {
                j += 1;
            }
            let lauf = j - i;
            for (k, c) in zeichen[i..j].iter().enumerate() {
                if k > 0 && (lauf - k) % 3 == 0 {
                    s.push(' ');
                }
                s.push(*c);
            }
            i = j;
        } else {
            s.push(zeichen[i]);
            i += 1;
        }
    }
    // Operatoren mit Abstand.
    let mut t = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if matches!(c, '+' | '-' | '−' | '×' | '÷' | '^') {
            t.push(' ');
            t.push(c);
            t.push(' ');
        } else {
            t.push(c);
        }
    }
    // Der Reihe nach wie im Original: "!" ohne Abstand, " %" ohne Abstand, ";" mit Abstand,
    // "( " und " )" zusammen, Abstaende zusammenfassen.
    let t = um_zeichen(&t, '!', true, true, false);
    let t = um_zeichen(&t, '%', true, true, true);
    let t = t.replace(';', " ; ");
    let t = um_zeichen(&t, '(', false, true, false);
    let t = um_zeichen(&t, ')', true, false, false);
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Entfernt Leerraum vor (`vor`) und/oder nach (`nach`) jedem `zeichen`. Mit `pflicht` nur,
/// wenn davor wirklich Leerraum stand (wie `\s+%\s*`).
fn um_zeichen(text: &str, zeichen: char, vor: bool, nach: bool, pflicht: bool) -> String {
    let z: Vec<char> = text.chars().collect();
    let mut aus: Vec<char> = Vec::with_capacity(z.len());
    let mut i = 0;
    while i < z.len() {
        if z[i] == zeichen {
            let mut weg = vor;
            if pflicht && !aus.last().is_some_and(|c| c.is_whitespace()) {
                weg = false;
            }
            if weg {
                while aus.last().is_some_and(|c| c.is_whitespace()) {
                    aus.pop();
                }
            }
            aus.push(zeichen);
            i += 1;
            if nach && (weg || !pflicht) {
                while i < z.len() && z[i].is_whitespace() {
                    i += 1;
                }
            }
        } else {
            aus.push(z[i]);
            i += 1;
        }
    }
    aus.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(t: &str) -> f64 {
        rechne(t, &Werte::default()).wert
    }

    #[test]
    fn rechnet() {
        assert_eq!(r("1+2*3"), 7.0);
        assert_eq!(r("2^3^2"), 512.0);
        assert_eq!(r("-2^2"), 4.0);
        assert_eq!(r("5!"), 120.0);
        assert_eq!(r("50%"), 0.5);
        assert_eq!(r("2(3+1)"), 8.0);
        assert_eq!(r("2e3"), 2000.0);
        assert!((r("sin(30)") - 0.5).abs() < 1e-12);
        assert!((r("2pi") - 2.0 * PI).abs() < 1e-12);
        assert_eq!(r("√(16)"), 4.0);
        assert_eq!(r("1,5×2"), 3.0);
        assert_eq!(r("ncr(5;2)"), 10.0);
        assert!(!rechne("1/0", &Werte::default()).ok);
        assert!(!rechne("2+", &Werte::default()).ok);
        assert!(!rechne("foo(2)", &Werte::default()).ok);
        let w = Werte { x: 3.0, deg: false, ..Werte::default() };
        assert_eq!(uebersetzen("x^2-2x").unwrap().wert(&w), 3.0);
    }

    #[test]
    fn texte() {
        assert_eq!(zahl_text(268268.0), "268268");
        assert_eq!(zahl_text(0.1 + 0.2), "0,3");
        assert_eq!(zahl_text(1.0 / 3.0), "0,333333333333");
        assert_eq!(zahl_text(1e21), "1e+21");
        assert_eq!(zahl_text(1e15), "1000000000000000");
        assert_eq!(js_zahl(1e-7), "1e-7");
        assert_eq!(anzeige("13134+134134"), "13 134 + 134 134");
        assert_eq!(anzeige("5 ! + 3 %"), "5!+ 3%");
        assert_eq!(anzeige("3%"), "3%");
        assert_eq!(anzeige("sin( 30 )"), "sin(30)");
    }
}
