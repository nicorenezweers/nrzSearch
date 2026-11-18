//! Der Draht zum Server: JSON holen und senden, der Such-Strom (Server-Sent Events) und der
//! KI-Strom (POST mit SSE-Antwort, abbrechbar). Alles ueber fetch/EventSource des Browsers.

use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, EventSource, MessageEvent, Request, RequestInit, Response};

/// Ein Fehler vom Server: Text (`fehler`) und optional ein Hinweis (`hinweis`).
#[derive(Clone, Debug, PartialEq)]
pub struct Fehler {
    pub status: u16,
    pub text: String,
    pub hinweis: Option<String>,
}

impl Fehler {
    fn netz() -> Self {
        Self { status: 0, text: "Keine Verbindung zum Server.".into(), hinweis: None }
    }
}

impl std::fmt::Display for Fehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

pub type Ergebnis<T> = Result<T, Fehler>;

/// Pfad mit Query-Parametern; leere Werte fallen weg.
pub fn url_mit(pfad: &str, params: &[(&str, &str)]) -> String {
    let teile: Vec<String> = params
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect();
    if teile.is_empty() { pfad.to_string() } else { format!("{pfad}?{}", teile.join("&")) }
}

async fn abschicken(methode: &str, pfad: &str, koerper: Option<JsValue>, art: Option<&str>, bearer: Option<&str>, signal: Option<&web_sys::AbortSignal>) -> Ergebnis<Response> {
    let fenster = web_sys::window().ok_or_else(Fehler::netz)?;
    let init = RequestInit::new();
    init.set_method(methode);
    if let Some(k) = &koerper {
        init.set_body(k);
    }
    if let Some(s) = signal {
        init.set_signal(Some(s));
    }
    let anfrage = Request::new_with_str_and_init(pfad, &init).map_err(|_| Fehler::netz())?;
    let kopf = anfrage.headers();
    let _ = kopf.set("accept", "application/json");
    if let Some(a) = art {
        let _ = kopf.set("content-type", a);
    }
    if let Some(b) = bearer.filter(|b| !b.is_empty()) {
        let _ = kopf.set("authorization", &format!("Bearer {b}"));
    }
    let antwort = JsFuture::from(fenster.fetch_with_request(&anfrage)).await.map_err(|_| Fehler::netz())?;
    antwort.dyn_into::<Response>().map_err(|_| Fehler::netz())
}

async fn json_aus(antwort: Response) -> Ergebnis<Value> {
    let status = antwort.status();
    let text = match antwort.text() {
        Ok(p) => JsFuture::from(p).await.ok().and_then(|t| t.as_string()).unwrap_or_default(),
        Err(_) => String::new(),
    };
    let wert: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let fehler = wert.get("fehler").and_then(Value::as_str).map(String::from);
        return Err(Fehler {
            status,
            text: fehler.unwrap_or_else(|| format!("Anfrage fehlgeschlagen (HTTP {status})")),
            hinweis: wert.get("hinweis").and_then(Value::as_str).map(String::from),
        });
    }
    Ok(wert)
}

/// GET mit Parametern, Antwort als JSON.
pub async fn get(pfad: &str, params: &[(&str, &str)]) -> Ergebnis<Value> {
    json_aus(abschicken("GET", &url_mit(pfad, params), None, None, None, None).await?).await
}

/// GET mit Bearer-Token (z. B. /api/admin/…), Antwort als JSON.
pub async fn get_mit_token(pfad: &str, bearer: &str) -> Ergebnis<Value> {
    json_aus(abschicken("GET", pfad, None, None, Some(bearer), None).await?).await
}

/// GET - bei jedem Fehler `None` (wie `apiOderNichts`).
pub async fn get_oder_nichts(pfad: &str, params: &[(&str, &str)]) -> Option<Value> {
    get(pfad, params).await.ok()
}

/// POST mit JSON-Koerper (optional mit Bearer-Token), Antwort als JSON.
pub async fn post(pfad: &str, koerper: &Value, bearer: Option<&str>) -> Ergebnis<Value> {
    let k = JsValue::from_str(&koerper.to_string());
    json_aus(abschicken("POST", pfad, Some(k), Some("application/json"), bearer, None).await?).await
}

/// POST mit rohen Bytes (z. B. eine Datei an /api/datei), Antwort als JSON.
pub async fn post_bytes(pfad: &str, daten: &JsValue, art: &str) -> Ergebnis<Value> {
    json_aus(abschicken("POST", pfad, Some(daten.clone()), Some(art), None, None).await?).await
}

/// Ein laufender Server-Sent-Events-Strom (GET). Beim Fallenlassen wird er geschlossen.
pub struct Strom {
    quelle: EventSource,
    _hoerer: Vec<Closure<dyn FnMut(MessageEvent)>>,
    _fehler: Closure<dyn FnMut(web_sys::Event)>,
}

impl Strom {
    /// Oeffnet `url` und ruft `an(ereignis, daten)` fuer jedes der genannten Ereignisse auf;
    /// `bei_fehler` bei einem Abbruch der Verbindung.
    pub fn oeffnen(url: &str, ereignisse: &[&str], an: impl Fn(&str, Value) + Clone + 'static, bei_fehler: impl FnMut() + 'static) -> Option<Self> {
        let quelle = EventSource::new(url).ok()?;
        let mut hoerer = Vec::new();
        for name in ereignisse {
            let (n, an) = (name.to_string(), an.clone());
            let c = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                let text = e.data().as_string().unwrap_or_default();
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    an(&n, v);
                }
            });
            let _ = quelle.add_event_listener_with_callback(name, c.as_ref().unchecked_ref());
            hoerer.push(c);
        }
        let mut bei_fehler = bei_fehler;
        let fehler = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| bei_fehler());
        quelle.set_onerror(Some(fehler.as_ref().unchecked_ref()));
        Some(Self { quelle, _hoerer: hoerer, _fehler: fehler })
    }

    pub fn schliessen(&self) {
        self.quelle.close();
    }
}

impl Drop for Strom {
    fn drop(&mut self) {
        self.quelle.set_onerror(None);
        self.quelle.close();
    }
}

/// Abbrechen eines laufenden POST-Stroms (KI). Klonbar; `abbrechen()` beendet ihn auch auf dem
/// Server (die Verbindung geht zu).
#[derive(Clone)]
pub struct Abbruch(AbortController);

impl Abbruch {
    pub fn neu() -> Self {
        Self(AbortController::new().expect("AbortController"))
    }
    pub fn abbrechen(&self) {
        self.0.abort();
    }
    pub fn abgebrochen(&self) -> bool {
        self.0.signal().aborted()
    }
}

impl PartialEq for Abbruch {
    fn eq(&self, other: &Self) -> bool {
        JsValue::from(self.0.clone()) == JsValue::from(other.0.clone())
    }
}

/// POST mit JSON und einer SSE-Antwort (`event: name` / `data: json`), z. B. /api/ki.
/// `an(ereignis, daten)` wird fuer jeden Block aufgerufen. Endet, wenn der Server fertig ist
/// oder `abbruch` ausgeloest wurde.
pub async fn post_strom(pfad: &str, koerper: &Value, abbruch: &Abbruch, mut an: impl FnMut(&str, Value)) -> Ergebnis<()> {
    let signal = abbruch.0.signal();
    let k = JsValue::from_str(&koerper.to_string());
    let antwort = abschicken("POST", pfad, Some(k), Some("application/json"), None, Some(&signal)).await?;
    if !antwort.ok() {
        return Err(json_aus(antwort).await.err().unwrap_or(Fehler { status: 0, text: "Die KI ist nicht erreichbar.".into(), hinweis: None }));
    }
    let Some(body) = antwort.body() else { return Err(Fehler::netz()) };
    let leser: web_sys::ReadableStreamDefaultReader = body.get_reader().unchecked_into();
    let decoder = web_sys::TextDecoder::new().map_err(|_| Fehler::netz())?;
    let mut puffer = String::new();
    loop {
        let teil = match JsFuture::from(leser.read()).await {
            Ok(t) => t,
            Err(_) => return Ok(()), // abgebrochen
        };
        let fertig = js_sys::Reflect::get(&teil, &"done".into()).ok().and_then(|d| d.as_bool()).unwrap_or(true);
        if fertig {
            return Ok(());
        }
        if let Ok(wert) = js_sys::Reflect::get(&teil, &"value".into()) {
            let bytes: js_sys::Uint8Array = wert.unchecked_into();
            let opts = web_sys::TextDecodeOptions::new();
            opts.set_stream(true);
            let mut v = bytes.to_vec();
            if let Ok(s) = decoder.decode_with_u8_array_and_options(&mut v, &opts) {
                puffer.push_str(&s);
            }
        }
        while let Some(ende) = puffer.find("\n\n") {
            let block: String = puffer.drain(..ende + 2).collect();
            let mut name = "";
            let mut daten = String::new();
            for zeile in block.lines() {
                if let Some(n) = zeile.strip_prefix("event:") {
                    name = n.trim();
                } else if let Some(d) = zeile.strip_prefix("data:") {
                    daten.push_str(d.trim());
                }
            }
            if !name.is_empty() {
                if let Ok(v) = serde_json::from_str::<Value>(&daten) {
                    an(name, v);
                }
            }
        }
    }
}
