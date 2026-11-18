//! Modul "speedtest": misst direkt im Browser Ping, Jitter, Download und Upload - gegen die
//! offenen Messpunkte von Cloudflare (speed.cloudflare.com, Client-Bibliothek Open Source unter
//! MIT). Nichts wird gespeichert.
//! Einstellungen: "server" (Basis-Adresse mit /__down und /__up), "sekunden" (Dauer je
//! Richtung), "verbindungen" (gleichzeitige Stroeme).

use super::ModulCtx;
use crate::bib::mathe::js_zahl;
use crate::werkzeug::{Kopf, ModulCss, zahl};
use dioxus::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, ReadableStreamDefaultReader, RequestCache, RequestInit, Response, XmlHttpRequest};

fn mbit(bytes: f64, ms: f64) -> f64 {
    (bytes * 8.0) / (ms * 1000.0)
}

fn jetzt() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or_else(js_sys::Date::now, |p| p.now())
}

async fn schlafen(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

pub async fn erkennen(ctx: &ModulCtx) -> Option<Value> {
    Some(ctx.gruppen_json())
}

/// Der Tacho: ein Halbkreis bis 1000 Mbit/s (logarithmisch, wie ueblich).
#[component]
fn Tacho(mbit: f64) -> Element {
    let anteil = ((1.0 + mbit).log10() / 3.0).min(1.0);
    let winkel = PI * (1.0 - anteil);
    let x = 100.0 + 80.0 * winkel.cos();
    let y = 100.0 - 80.0 * winkel.sin();
    let wert = format!("M20 100 A80 80 0 0 1 {x:.1} {y:.1}");
    rsx! {
        svg { view_box: "0 0 200 116", class: "sp-tacho", "aria-hidden": "true",
            path { d: "M20 100 A80 80 0 0 1 180 100", class: "sp-bahn" }
            if anteil > 0.0 {
                path { d: "{wert}", class: "sp-wert" }
            }
            for m in [1.0f64, 10.0, 100.0, 1000.0] {
                {
                    let w = PI * (1.0 - (1.0 + m).log10() / 3.0);
                    rsx! {
                        text { key: "{m}", x: "{js_zahl(100.0 + 96.0 * w.cos())}", y: "{js_zahl(104.0 - 96.0 * w.sin())}", "{m}" }
                    }
                }
            }
        }
    }
}

/// Was eine Messung zwischen den Schritten teilt.
struct Lauf {
    server: String,
    sekunden: f64,
    stroeme: usize,
    laeuft: Cell<bool>,
    abbruch: RefCell<Option<AbortController>>,
    /// Offene Uploads (werden beim Abbrechen/Ende abgebrochen).
    offen: RefCell<Vec<XmlHttpRequest>>,
}

impl Lauf {
    fn alle_uploads_abbrechen(&self) {
        for x in self.offen.borrow().iter() {
            let _ = x.abort();
        }
    }

    async fn holen(&self, pfad: &str) -> Result<Response, JsValue> {
        let init = RequestInit::new();
        init.set_cache(RequestCache::NoStore);
        if let Some(a) = self.abbruch.borrow().as_ref() {
            init.set_signal(Some(&a.signal()));
        }
        let url = format!("{}{pfad}&r={}", self.server, js_sys::Math::random());
        let fenster = web_sys::window().ok_or(JsValue::NULL)?;
        let antwort = JsFuture::from(fenster.fetch_with_str_and_init(&url, &init)).await?;
        antwort.dyn_into::<Response>()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Anzeige {
    live: f64,
}

#[component]
pub fn Zeigen(ctx: ModulCtx, daten: Value) -> Element {
    let _ = &daten;
    let lauf = use_hook(|| {
        let server = ctx.cfg_str("server").filter(|s| !s.is_empty()).unwrap_or("https://speed.cloudflare.com");
        Rc::new(Lauf {
            server: server.strip_suffix('/').unwrap_or(server).to_string(),
            sekunden: ctx.cfg_f64("sekunden").filter(|s| *s > 0.0).unwrap_or(8.0),
            stroeme: ctx.cfg_f64("verbindungen").filter(|s| *s >= 1.0).unwrap_or(4.0) as usize,
            laeuft: Cell::new(false),
            abbruch: RefCell::new(None),
            offen: RefCell::new(Vec::new()),
        })
    });
    let host = web_sys::Url::new(&lauf.server).map(|u| u.host()).unwrap_or_default();
    let mut ort = use_signal(|| host.clone());
    let mut anzeige = use_signal(|| Anzeige { live: 0.0 });
    let mut phase = use_signal(|| "Bereit".to_string());
    let mut werte = use_signal(|| ["–".to_string(), "–".to_string(), "–".to_string(), "–".to_string()]);
    let mut knopf = use_signal(|| "Test starten".to_string());
    {
        let lauf = lauf.clone();
        use_drop(move || {
            lauf.laeuft.set(false);
            if let Some(a) = lauf.abbruch.borrow().as_ref() {
                a.abort();
            }
            lauf.alle_uploads_abbrechen();
        });
    }

    let mut live = move |m: f64, p: Option<&str>| {
        anzeige.set(Anzeige { live: m });
        if let Some(p) = p {
            phase.set(p.to_string());
        }
    };

    // Ping: 15 kleine Anfragen, der Median - Jitter ist die mittlere Abweichung zweier
    // aufeinanderfolgender Messungen.
    let ping = {
        let lauf = lauf.clone();
        move || {
            let lauf = lauf.clone();
            async move {
                let mut zeiten: Vec<f64> = Vec::new();
                let mut i = 0;
                while i < 15 && lauf.laeuft.get() {
                    let start = jetzt();
                    let antwort = lauf.holen("/__down?bytes=0").await?;
                    JsFuture::from(antwort.array_buffer()?).await?;
                    zeiten.push(jetzt() - start);
                    if i == 0 {
                        let k = antwort.headers();
                        if let Ok(Some(colo)) = k.get("cf-meta-colo") {
                            let stadt = k.get("cf-meta-city").ok().flatten().map(|c| format!(" · {c}")).unwrap_or_default();
                            ort.set(format!("Cloudflare {colo}{stadt}"));
                        }
                    }
                    i += 1;
                }
                let mut sortiert = zeiten.clone();
                sortiert.sort_by(f64::total_cmp);
                let jitter = zeiten.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (zeiten.len().saturating_sub(1)).max(1) as f64;
                Ok::<_, JsValue>((sortiert.get(sortiert.len() / 2).copied().unwrap_or(f64::NAN), jitter))
            }
        }
    };

    // Download: mehrere Stroeme laden grosse Bloecke, gezaehlt wird jedes Byte, die erste
    // Sekunde (Anlauf) zaehlt nicht.
    let download = {
        let lauf = lauf.clone();
        move || {
            let lauf = lauf.clone();
            async move {
                let bytes = Rc::new(Cell::new(0.0f64));
                let start = Rc::new(Cell::new(jetzt()));
                let gezaehlt = Rc::new(Cell::new(0.0f64));
                let ende = start.get() + lauf.sekunden * 1000.0;
                let fertig = Rc::new(Cell::new(false));
                {
                    let (bytes, start, gezaehlt, fertig) = (bytes.clone(), start.clone(), gezaehlt.clone(), fertig.clone());
                    spawn(async move {
                        loop {
                            schlafen(200).await;
                            if fertig.get() {
                                break;
                            }
                            let j = jetzt();
                            if gezaehlt.get() == 0.0 && j - start.get() > 1000.0 {
                                gezaehlt.set(bytes.get());
                                start.set(j);
                            }
                            if gezaehlt.get() != 0.0 {
                                live(mbit(bytes.get() - gezaehlt.get(), j - start.get()), None);
                            }
                        }
                    });
                }
                let strom = || {
                    let (lauf, bytes) = (lauf.clone(), bytes.clone());
                    async move {
                        while lauf.laeuft.get() && jetzt() < ende {
                            let antwort = lauf.holen("/__down?bytes=25000000").await?;
                            let Some(koerper) = antwort.body() else { break };
                            let leser: ReadableStreamDefaultReader = koerper.get_reader().unchecked_into();
                            loop {
                                let stueck = JsFuture::from(leser.read()).await?;
                                if js_sys::Reflect::get(&stueck, &"done".into())?.is_truthy() {
                                    break;
                                }
                                let wert = js_sys::Reflect::get(&stueck, &"value".into())?;
                                let laenge = js_sys::Reflect::get(&wert, &"length".into())?.as_f64().unwrap_or(0.0);
                                bytes.set(bytes.get() + laenge);
                                if jetzt() > ende {
                                    let _ = leser.cancel();
                                    break;
                                }
                            }
                        }
                        Ok::<_, JsValue>(())
                    }
                };
                let ergebnisse = futures::future::join_all((0..lauf.stroeme).map(|_| strom())).await;
                fertig.set(true);
                ergebnisse.into_iter().collect::<Result<Vec<_>, _>>()?;
                Ok::<_, JsValue>(mbit(bytes.get() - gezaehlt.get(), jetzt() - start.get()))
            }
        }
    };

    // Upload: mehrere Stroeme schicken Bloecke, der Fortschritt kommt von XHR.
    let upload = {
        let lauf = lauf.clone();
        move || {
            let lauf = lauf.clone();
            async move {
                let block = js_sys::Uint8Array::new_with_length(4 * 1024 * 1024);
                let mut zufall = vec![0u8; 65536];
                if let Some(k) = web_sys::window().and_then(|w| w.crypto().ok()) {
                    let _ = k.get_random_values_with_u8_array(&mut zufall);
                }
                block.subarray(0, 65536).copy_from(&zufall);
                let bytes = Rc::new(Cell::new(0.0f64));
                let start = Rc::new(Cell::new(jetzt()));
                let gezaehlt = Rc::new(Cell::new(0.0f64));
                let ende = start.get() + lauf.sekunden * 1000.0;
                let fertig = Rc::new(Cell::new(false));
                {
                    let (bytes, start, gezaehlt, fertig, lauf) = (bytes.clone(), start.clone(), gezaehlt.clone(), fertig.clone(), lauf.clone());
                    spawn(async move {
                        loop {
                            schlafen(200).await;
                            if fertig.get() {
                                break;
                            }
                            let j = jetzt();
                            if j > ende {
                                lauf.alle_uploads_abbrechen();
                            }
                            if gezaehlt.get() == 0.0 && j - start.get() > 1000.0 {
                                gezaehlt.set(bytes.get());
                                start.set(j);
                            }
                            if gezaehlt.get() != 0.0 {
                                live(mbit(bytes.get() - gezaehlt.get(), j - start.get()), None);
                            }
                        }
                    });
                }
                let senden = || {
                    let (lauf, bytes, block) = (lauf.clone(), bytes.clone(), block.clone());
                    async move {
                        let xhr = XmlHttpRequest::new()?;
                        let vorher = Rc::new(Cell::new(0.0f64));
                        let fortschritt = Closure::<dyn FnMut(web_sys::ProgressEvent)>::new(move |e: web_sys::ProgressEvent| {
                            bytes.set(bytes.get() + e.loaded() - vorher.get());
                            vorher.set(e.loaded());
                        });
                        xhr.upload()?.set_onprogress(Some(fortschritt.as_ref().unchecked_ref()));
                        let x2 = xhr.clone();
                        let versprechen = js_sys::Promise::new(&mut |fertig, _| x2.set_onloadend(Some(&fertig)));
                        lauf.offen.borrow_mut().push(xhr.clone());
                        xhr.open("POST", &format!("{}/__up", lauf.server))?;
                        xhr.send_with_opt_js_u8_array(Some(&block))?;
                        let _ = JsFuture::from(versprechen).await;
                        lauf.offen.borrow_mut().retain(|x| x != &xhr);
                        xhr.set_onloadend(None);
                        drop(fortschritt);
                        Ok::<_, JsValue>(())
                    }
                };
                let strom = || {
                    let lauf = lauf.clone();
                    async move {
                        while lauf.laeuft.get() && jetzt() < ende {
                            senden().await?;
                        }
                        Ok::<_, JsValue>(())
                    }
                };
                let ergebnisse = futures::future::join_all((0..lauf.stroeme).map(|_| strom())).await;
                fertig.set(true);
                ergebnisse.into_iter().collect::<Result<Vec<_>, _>>()?;
                Ok::<_, JsValue>(mbit(bytes.get() - gezaehlt.get(), jetzt() - start.get()))
            }
        }
    };

    let starten = {
        let lauf = lauf.clone();
        move |_| {
            if lauf.laeuft.get() {
                lauf.laeuft.set(false);
                if let Some(a) = lauf.abbruch.borrow().as_ref() {
                    a.abort();
                }
                lauf.alle_uploads_abbrechen();
                return;
            }
            lauf.laeuft.set(true);
            *lauf.abbruch.borrow_mut() = AbortController::new().ok();
            knopf.set("Abbrechen".into());
            werte.set(["–".into(), "–".into(), "–".into(), "–".into()]);
            let (lauf, ping, download, upload) = (lauf.clone(), ping.clone(), download.clone(), upload.clone());
            spawn(async move {
                let messung = async {
                    live(0.0, Some("Ping wird gemessen …"));
                    let (p, j) = ping().await?;
                    werte.with_mut(|w| {
                        w[2] = js_zahl(p.round());
                        w[3] = zahl(j, 1);
                    });
                    live(0.0, Some("Download wird gemessen …"));
                    let d = download().await?;
                    werte.with_mut(|w| w[0] = zahl(d, 1));
                    live(0.0, Some("Upload wird gemessen …"));
                    let u = upload().await?;
                    werte.with_mut(|w| w[1] = zahl(u, 1));
                    live(d, Some("Fertig"));
                    Ok::<_, JsValue>(())
                };
                if messung.await.is_err() {
                    live(0.0, Some(if lauf.laeuft.get() { "Der Messpunkt ist nicht erreichbar." } else { "Abgebrochen" }));
                }
                lauf.laeuft.set(false);
                knopf.set("Erneut testen".into());
            });
        }
    };

    let m = anzeige().live;
    let live_text = if m >= 100.0 { js_zahl(m.round()) } else { zahl(m, 1) };
    let [down, up, ping_t, jitter] = werte();

    rsx! {
        ModulCss { name: "speedtest".to_string(), css: CSS }
        Kopf {
            titel: "Speedtest",
            rechts: rsx! {
                "Messpunkt: "
                span { class: "sp-ort", "{ort}" }
            },
        }
        div { class: "sp-rumpf",
            div { class: "sp-anzeige",
                div { class: "sp-tacho-halter", Tacho { mbit: m } }
                div { class: "sp-zahl",
                    span { class: "sp-live", "{live_text}" }
                    small { "Mbit/s" }
                }
                div { class: "sp-phase w-text", "{phase}" }
            }
            div { class: "sp-werte",
                div { span { "Download" } b { class: "sp-down", "{down}" } small { "Mbit/s" } }
                div { span { "Upload" } b { class: "sp-up", "{up}" } small { "Mbit/s" } }
                div { span { "Ping" } b { class: "sp-ping", "{ping_t}" } small { "ms" } }
                div { span { "Jitter" } b { class: "sp-jitter", "{jitter}" } small { "ms" } }
            }
        }
        div { class: "w-zeile",
            button { r#type: "button", class: "w-knopf haupt sp-start", onclick: starten, "{knopf}" }
            span { class: "w-text sp-hinweis", "Misst die Verbindung dieses Geräts ins Internet." }
        }
    }
}

const CSS: &str = r#"
.sp-rumpf { display: flex; gap: 24px; align-items: center; flex-wrap: wrap; }
.sp-anzeige { position: relative; width: 240px; text-align: center; }
.sp-tacho { width: 240px; display: block; }
.sp-bahn { fill: none; stroke: var(--flaeche3); stroke-width: 14; stroke-linecap: round; }
.sp-wert { fill: none; stroke: var(--hoch); stroke-width: 14; stroke-linecap: round; transition: d .2s; }
.sp-tacho text { font-size: 9px; fill: var(--gedimmt); text-anchor: middle; }
.sp-zahl { margin-top: -54px; font-size: 40px; line-height: 1; font-variant-numeric: tabular-nums; }
.sp-zahl small { display: block; font-size: 13px; color: var(--gedimmt); margin-top: 4px; }
.sp-phase { margin-top: 6px; font-size: 13px; }
.sp-werte { flex: 1; display: grid; grid-template-columns: 1fr 1fr; gap: 12px; min-width: 240px; }
.sp-werte div { display: flex; flex-direction: column; padding: 10px 14px; background: var(--flaeche2); border-radius: var(--r); }
.sp-werte span { font-size: 13px; color: var(--gedimmt); }
.sp-werte b { font-size: 28px; font-weight: 400; font-variant-numeric: tabular-nums; }
.sp-werte small { font-size: 12px; color: var(--gedimmt); }
"#;
