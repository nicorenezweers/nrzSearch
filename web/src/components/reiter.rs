//! Die Reiter wie bei Google (der Rest unter "Mehr") und rechts "Tools": Sprache, Zeitraum,
//! SafeSearch, die Suchmaschinen (/api/engines) und die Zeile mit Trefferzahl und Dauer.

use super::helfer::{nach_oben, use_klick_daneben};
use crate::api;
use crate::i18n::{SPRACHEN, t};
use crate::pages::suche::{LAEDT, neu_suchen, sprache_setzen};
use crate::state::{self, EINSTELLUNGEN, KATEGORIEN, SUCHE, ZUSTAND};
use crate::werkzeug::zahl;
use dioxus::prelude::*;
use serde_json::Value;

/// Der Rest steht unter "Mehr".
const SICHTBARE_REITER: usize = 6;

/// Eine Suchmaschine der Instanz.
#[derive(Clone, Debug, PartialEq)]
struct Motor {
    name: String,
    kurz: String,
    kategorien: Vec<String>,
    aktiv: bool,
}

/// Alle Suchmaschinen (einmal geladen, beim ersten Oeffnen der Liste).
static MOTOREN: GlobalSignal<Option<Vec<Motor>>> = Signal::global(|| None);

fn motoren_von_kat(alle: &[Motor], kat: &str) -> Vec<Motor> {
    let kats: &[&str] = if kat == "images" { &["images", "stock images"] } else { &[kat] };
    alle.iter().filter(|m| m.kategorien.iter().any(|c| kats.contains(&c.as_str()))).cloned().collect()
}

thread_local! {
    static MOTOREN_ANGEFRAGT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

async fn motoren_laden() {
    if MOTOREN_ANGEFRAGT.with(|a| a.replace(true)) {
        return;
    }
    let d = api::get_oder_nichts("/api/engines", &[]).await;
    let liste = d
        .as_ref()
        .and_then(|d| d.get("motoren"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|m| Motor {
                    name: m.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                    kurz: m.get("kurz").and_then(Value::as_str).unwrap_or_default().to_string(),
                    kategorien: m.get("kategorien").and_then(Value::as_array).map(|k| k.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default(),
                    aktiv: m.get("aktiv").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    *MOTOREN.write() = Some(liste);
}

fn motoren_uebernehmen(wahl: Vec<String>) {
    let mut e = EINSTELLUNGEN.write();
    e.motoren = wahl;
    e.speichern();
    drop(e);
    neu_suchen();
}

#[component]
pub fn Reiter() -> Element {
    let mut mehr_offen = use_signal(|| false);
    let mut tools_offen = use_signal(|| false);
    let mut motoren_offen = use_signal(|| false);
    use_klick_daneben("mehr-wahl", move || {
        if *mehr_offen.peek() {
            mehr_offen.set(false);
        }
    });
    use_klick_daneben("motoren-wahl", move || {
        if *motoren_offen.peek() {
            motoren_offen.set(false);
        }
    });

    let z = ZUSTAND.read();
    let einst = EINSTELLUNGEN.read();
    let laedt = LAEDT();
    let aktiv = KATEGORIEN.iter().position(|(id, _, _)| *id == z.kategorie);
    let sichtbar: Vec<usize> = (0..KATEGORIEN.len()).filter(|&i| i < SICHTBARE_REITER || Some(i) == aktiv).collect();
    let mehr: Vec<usize> = (0..KATEGORIEN.len()).filter(|i| !sichtbar.contains(i)).collect();
    let kategorie = z.kategorie.clone();

    let knopf = move |i: usize| {
        let (id, text, stern) = KATEGORIEN[i];
        let ist = id == kategorie;
        rsx! {
            button {
                key: "{id}",
                r#type: "button",
                "data-kat": id,
                "aria-current": if ist { "true" } else { "false" },
                "data-ladend": if ist && laedt { "1" } else { "" },
                onclick: move |_| {
                    mehr_offen.set(false);
                    if ZUSTAND.peek().kategorie == id {
                        nach_oben(true);
                        return;
                    }
                    nach_oben(false);
                    state::kategorie_setzen(id);
                },
                if stern {
                    span { class: "ki-stern" }
                }
                {t(text)}
            }
        }
    };

    rsx! {
        nav { class: "kategorien", id: "kategorien", "aria-label": "Kategorien",
            div { class: "reiter", id: "reiter",
                for i in sichtbar.iter().copied() {
                    {knopf(i)}
                }
                if !mehr.is_empty() {
                    span { class: "mehr", id: "mehr-wahl",
                        button {
                            r#type: "button",
                            class: "mehr-knopf",
                            "aria-expanded": "{mehr_offen()}",
                            "aria-controls": "mehr-liste",
                            onclick: move |_| mehr_offen.toggle(),
                            {t("mehr")}
                            " "
                            span { class: "pfeil", "▾" }
                        }
                        span { class: "mehr-liste", id: "mehr-liste", hidden: !mehr_offen(),
                            for i in mehr.iter().copied() {
                                {knopf(i)}
                            }
                        }
                    }
                }
            }
            button {
                r#type: "button",
                class: "tools-knopf",
                "aria-expanded": "{tools_offen()}",
                "aria-controls": "tools",
                onclick: move |_| tools_offen.toggle(),
                {t("tools")}
            }
        }
        if tools_offen() {
            div { class: "tools", id: "tools",
                label { class: "tool",
                    select {
                        id: "sprache",
                        "aria-label": "Sprache",
                        value: "{einst.sprache}",
                        onchange: move |e: FormEvent| sprache_setzen(&e.value()),
                        for (code, name) in SPRACHEN.iter() {
                            option { key: "{code}", value: *code, selected: einst.sprache == *code, "{name}" }
                        }
                        option { value: "all", selected: einst.sprache == "all", "🌐" }
                    }
                }
                label { class: "tool",
                    select {
                        id: "zeitraum",
                        "aria-label": "Zeitraum",
                        onchange: move |e: FormEvent| {
                            let mut s = EINSTELLUNGEN.write();
                            s.zeitraum = e.value();
                            s.speichern();
                            drop(s);
                            neu_suchen();
                        },
                        for (wert, text) in [("", "Beliebige Zeit"), ("day", "Letzte 24 Stunden"), ("week", "Letzte Woche"), ("month", "Letzter Monat"), ("year", "Letztes Jahr")] {
                            option { key: "{wert}", value: wert, selected: einst.zeitraum == wert, "{text}" }
                        }
                    }
                }
                // Immer streng - der Server filtert ausserdem selbst (Jugendschutz).
                span { class: "tool tool-fest", title: "Pornografische Inhalte werden immer herausgefiltert", "SafeSearch: streng" }
                MotorenWahl { offen: motoren_offen }
                span { class: "tools-info", id: "tools-info", InfoZeile {} }
            }
        }
    }
}

/// Die Auswahl der Suchmaschinen: Standard, Alle oder einzelne fuer den Reiter.
#[component]
fn MotorenWahl(offen: Signal<bool>) -> Element {
    let kat = ZUSTAND.read().kategorie.clone();
    let wahl = EINSTELLUNGEN.read().motoren.clone();
    let alle = MOTOREN.read().clone();
    let geladen = alle.as_ref().is_some_and(|a| !a.is_empty());
    let von_kat = alle.as_deref().map(|a| motoren_von_kat(a, &kat)).unwrap_or_default();
    let n = wahl.iter().filter(|name| !geladen || von_kat.iter().any(|m| &&m.name == name)).count();
    let beschriftung = if wahl.iter().any(|w| w == "alle") {
        "Alle Suchmaschinen".to_string()
    } else if n > 0 {
        format!("{n} Suchmaschinen")
    } else {
        "Standard-Suchmaschinen".to_string()
    };
    let mut sortiert = von_kat.clone();
    sortiert.sort_by(|a, b| b.aktiv.cmp(&a.aktiv).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let alle_an = wahl.iter().any(|w| w == "alle");
    let gesamt = alle.as_ref().map_or(0, Vec::len);

    rsx! {
        span { class: "tool tool-motoren", id: "motoren-wahl",
            button {
                r#type: "button",
                "aria-expanded": "{offen()}",
                "aria-controls": "motoren",
                onclick: move |_| {
                    offen.toggle();
                    if offen() {
                        spawn(motoren_laden());
                    }
                },
                span { id: "lbl-motoren", "{beschriftung}" }
                " ▾"
            }
            div { class: "motoren", id: "motoren", hidden: !offen(),
                div { class: "motoren-kopf",
                    span { id: "motoren-zahl",
                        if alle.is_some() { "{von_kat.len()} von {gesamt}" } else { "…" }
                    }
                    span { class: "motoren-wahl",
                        button { r#type: "button", onclick: move |_| motoren_uebernehmen(Vec::new()), "Standard" }
                        button { r#type: "button", onclick: move |_| motoren_uebernehmen(vec!["alle".into()]), "Alle" }
                    }
                }
                div { class: "motoren-liste", id: "motoren-liste", role: "group", "aria-label": "Suchmaschinen auswählen",
                    if alle.is_some() && !geladen {
                        div { class: "motoren-leer", "Die Liste der Suchmaschinen ist gerade nicht erreichbar." }
                    } else if geladen && sortiert.is_empty() {
                        div { class: "motoren-leer", "Für diesen Reiter ist nichts eingetragen." }
                    }
                    for m in sortiert.iter() {
                        label { key: "{m.name}", class: if m.aktiv { "motoren-zeile aktiv" } else { "motoren-zeile" },
                            input {
                                r#type: "checkbox",
                                value: "{m.name}",
                                checked: alle_an || wahl.contains(&m.name),
                                onchange: {
                                    let name = m.name.clone();
                                    let namen: Vec<String> = sortiert.iter().map(|x| x.name.clone()).collect();
                                    let wahl = wahl.clone();
                                    move |e: FormEvent| {
                                        let an = e.checked();
                                        // Wie im Original: nur die angehakten dieses Reiters.
                                        let neu: Vec<String> = namen
                                            .iter()
                                            .filter(|x| if **x == name { an } else { alle_an || wahl.contains(x) })
                                            .cloned()
                                            .collect();
                                        motoren_uebernehmen(neu);
                                    }
                                },
                            }
                            span { class: "motoren-name", "{m.name}" }
                            if !m.kurz.is_empty() {
                                span { class: "motoren-kurz", "{m.kurz}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Trefferzahl und Dauer - wie bei Google in der Tools-Zeile; dazu die Quellen, die nicht
/// antworten (mit Liste beim Drueberfahren).
#[component]
fn InfoZeile() -> Element {
    let d = SUCHE.read();
    let z = ZUSTAND.read();
    if d.frage != z.q || (d.treffer.is_empty() && !d.fertig) || z.kategorie == "ki" {
        return rsx! {};
    }
    let dauer = if d.dauer_ms > 0 { format!(" ({} Sekunden)", zahl(d.dauer_ms as f64 / 1000.0, 2)) } else { String::new() };
    let text = format!("Ungefähr {} Ergebnisse{dauer}", zahl(d.treffer.len() as f64, 0));
    let mut zeilen: Vec<String> = d
        .kaputte
        .iter()
        .take(12)
        .map(|k| {
            let grund = k.get("grund").and_then(Value::as_str).filter(|g| !g.is_empty()).unwrap_or("kein Grund angegeben");
            format!("{} – {grund}", k.get("engine").and_then(Value::as_str).unwrap_or("?"))
        })
        .collect();
    if d.kaputte.len() > 12 {
        zeilen.push(format!("… und {} weitere", d.kaputte.len() - 12));
    }
    let anzahl = d.kaputte.len();
    rsx! {
        "{text}"
        if anzahl > 0 {
            " · "
            span { class: "kaputt", tabindex: "0",
                "{anzahl} Quellen antworten nicht"
                span { class: "tipp",
                    for (i, z) in zeilen.iter().enumerate() {
                        span { key: "{i}", "{z}" }
                    }
                }
            }
        }
    }
}
