// nrzSearch - der Server.
//
// Eigene Such-Oberflaeche fuer die eigene SearXNG-Instanz. Der Browser
// redet NIE direkt mit der Instanz - er schickt seine Anfrage an /api/...,
// der Server holt die JSON-Antwort, prueft sie und schickt sie zurueck. So
// bleiben die Adresse der Instanz und etwaige Zugangsregeln im Server.
// Gespeichert wird nichts: keine Datenbank, kein Protokoll, kein Tracking.
//
// Der Aufbau:
//   /api/suche          die Suche als Strom (Server-Sent Events)
//   /api/search         dieselbe Suche in einem Rutsch
//   /api/ort            Ort, Sehenswuerdigkeit, Person oder Unternehmen
//   /api/kurz           die schnelle Kurzfassung dazu (nur Wikipedia)
//   /api/wetter         die Wettervorhersage
//   /api/kurs           die Kurskarte
//   /api/module         die Konfiguration der Module (module.json)
//   /api/ip             die eigene IP-Adresse
//   /api/geo            ein Ort mit Koordinaten und Zeitzone
//   /api/ki             Uebersicht mit KI und KI-Modus (Strom, OpenRouter)
//   /api/uebersetzen    der Uebersetzer (Dieter)
//   /api/datei          Text aus PDF, Word, Excel ... (fuer den Arbeitsbereich)
//   /api/ablage         Chats und Arbeitsbereiche je Browser (Token), verschluesselt
//   /api/statistik      anonyme Zaehler fuer den Footer
//   /api/admin          Einrichtung beim ersten Start, Dashboard (/admin)
//   alles andere        die Oberflaeche (Rust/WASM aus web/, fertig gebaut in WEB_DIR)

// Schneller als der System-Allocator bei vielen kleinen JSON-Werten.
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod datei;
mod ablage;
mod filter;
mod admin;
mod instanz;
mod ki;
mod market;
mod place;
mod schutz;
mod search;
mod searxng;
mod statistik;
mod util;

use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Query, Request, State},
    middleware::{self, Next},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
};
use util::hole;

/// Alles, was der Server braucht.
pub struct AppState {
    client: reqwest::Client,
    cache: search::Cache,
    markt: market::Markt,
    place: place::Place,
    verzeichnis: search::EngineVerzeichnis,
    instanz: instanz::Instanz,
    /// Die eingebaute SearXNG-Instanz (None = abgeschaltet, nur SXNG_URL).
    searxng: Option<Arc<searxng::Eingebaut>>,
    port: u16,
    engine_timeout: Duration,
    module: String,
    ki: ki::Ki,
    ablagen: ablage::Ablagen,
    schutz: schutz::Schutz,
    statistik: Arc<statistik::Statistik>,
    admin: admin::Admin,
    /// Kleine Antworten anderer Dienste (Vorschlaege, Geo, Wetter, IP-Info) - kurz gemerkt.
    merker: std::sync::Mutex<search::Lru<Value>>,
}

impl AppState {
    fn merken_lesen(&self, k: &str) -> Option<Value> {
        self.merker.lock().ok()?.lesen(k)
    }
    fn merken(&self, k: String, v: &Value) {
        if let Ok(mut m) = self.merker.lock() {
            m.schreiben(k, v.clone());
        }
    }
}

type St = State<Arc<AppState>>;
type Params = Query<HashMap<String, String>>;

/// Eine Einstellung aus der Umgebung (ohne "/" am Ende).
fn env(name: &str, vorgabe: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| vorgabe.into()).trim_end_matches('/').to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = env("PORT", "8188").parse()?;
    // Der HTTP-Client: ein Verbindungspool fuer alle, kurze Timeouts.
    let client = reqwest::Client::builder()
        .user_agent(concat!("nrzSearch/", env!("CARGO_PKG_VERSION")))
        .pool_max_idle_per_host(32)
        .tcp_nodelay(true)
        .pool_idle_timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()?;
    let statistik = Arc::new(statistik::Statistik::neu(env("STATISTIK_DATEI", "statistik.json")));
    let state = Arc::new(AppState {
        cache: Default::default(),
        markt: market::Markt::default(),
        place: place::Place::neu(
            client.clone(),
            env("NOMINATIM_URL", "https://nominatim.openstreetmap.org"),
            env("WIKIDATA_URL", "https://www.wikidata.org"),
        ),
        verzeichnis: Default::default(),
        // Eine oder mehrere Adressen, mit Komma getrennt - die erste, die
        // antwortet, wird genommen (siehe instanz.rs).
        instanz: instanz::Instanz::neu(&env("SXNG_URL", "")),
        // Die eingebaute Instanz (Vorgabe). SEARXNG_EINGEBAUT=0 schaltet sie ab.
        searxng: (env("SEARXNG_EINGEBAUT", "1") != "0").then(|| Arc::new(searxng::Eingebaut::neu())),
        port,
        engine_timeout: Duration::from_millis(env("ENGINE_TIMEOUT", "12000").parse()?),
        module: env("MODULE_CONFIG", "module.json"),
        ki: ki::Ki::neu(
            env("OPENROUTER_URL", "https://openrouter.ai/api/v1"),
            env("OPENROUTER_MODEL", "openrouter/free"),
            env("OPENROUTER_KEY_FILE", "openrouter.key"),
            statistik.clone(),
        ),
        // Die Konten liegen verschluesselt in diesem Ordner.
        // Die Chats je Browser liegen verschluesselt in diesem Ordner.
        ablagen: ablage::Ablagen::neu(&env("ABLAGE_DIR", "ablagen")),
        schutz: Default::default(),
        statistik: statistik.clone(),
        admin: admin::Admin::neu(env("ADMIN_DATEI", "admin.json")),
        merker: std::sync::Mutex::new(search::Lru::neu(5000, Duration::from_secs(600))),
        client,
    });
    filter::vorbereiten();

    // Die Oberflaeche: die fertigen Dateien von `dx bundle` (index.html + assets/ mit Hash im
    // Namen). Vorkomprimiert (.br/.gz neben der Datei) geht direkt raus.
    let web_dir = env("WEB_DIR", "public");
    let index = ServeFile::new(format!("{web_dir}/index.html")).precompressed_br().precompressed_gzip();
    let oberflaeche = ServeDir::new(&web_dir).precompressed_br().precompressed_gzip().fallback(index);

    let app = Router::new()
        .route("/api/suche", get(api_suche))
        .route("/api/search", get(api_search))
        .route("/api/ort", get(api_ort))
        .route("/api/kurz", get(api_kurz))
        .route("/api/wetter", get(api_wetter))
        .route("/api/vorschlaege", get(api_vorschlaege))
        .route("/api/engines", get(api_engines))
        .route("/api/version", get(api_version))
        .route("/api/health", get(api_health))
        .route("/api/kurs", get(api_kurs))
        .route("/api/module", get(api_module))
        .route("/api/ip", get(api_ip))
        .route("/api/geo", get(api_geo))
        // Dateien und Bilder aus dem Arbeitsbereich duerfen gross sein.
        .route("/api/ki", post(api_ki).layer(DefaultBodyLimit::max(60 << 20)))
        .route("/api/uebersetzen", post(api_uebersetzen))
        .route("/api/datei", post(api_datei).layer(DefaultBodyLimit::max(60 << 20)))
        .route("/api/ki/status", get(api_ki_status))
        .route("/api/ablage/laden", post(api_ablage_laden))
        .route("/api/ablage/speichern", post(api_ablage_speichern).layer(DefaultBodyLimit::max(80 << 20)))
        .route("/api/statistik", get(api_statistik))
        .route("/api/admin/status", get(api_admin_status))
        .route("/api/admin/einrichten", post(api_admin_einrichten))
        .route("/api/admin/anmelden", post(api_admin_anmelden))
        .route("/api/admin/abmelden", post(api_admin_abmelden))
        .route("/api/admin/uebersicht", get(api_admin_uebersicht))
        .route("/api/admin/module", post(api_admin_module))
        .route("/api/admin/ki", post(api_admin_ki))
        .route("/api/admin/searxng", post(api_admin_searxng))
        .route("/api/admin/profil", post(api_admin_profil).layer(DefaultBodyLimit::max(4 << 20)))
        .fallback_service(oberflaeche)
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn(kopfzeilen))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("nrzSearch läuft auf http://127.0.0.1:{port}");
    if !state.instanz.adressen().is_empty() {
        println!("Externe SearXNG-Adressen: {}", state.instanz.adressen().join(", "));
    }
    if let Some(sx) = state.searxng.clone() {
        let st = state.clone();
        tokio::spawn(async move { sx.laufen(st.client.clone(), &st.instanz).await });
    }
    let waechter = state.clone();
    tokio::spawn(async move { waechter.instanz.ueberwachen(waechter.client.clone()).await });
    let zaehler = state.clone();
    tokio::spawn(async move { zaehler.statistik.speichern().await });
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(ende())
        .await?;
    // SearXNG mitnehmen (sonst bliebe es als Waise auf seinem Port).
    if let Some(sx) = &state.searxng {
        sx.stoppen().await;
    }
    Ok(())
}

/// Wartet auf Strg+C oder SIGTERM (docker stop).
async fn ende() {
    let strg_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = strg_c => {}, _ = term => {} }
}

/// Cache- und Schutz-Kopfzeilen je Pfad: Dateien mit Hash im Namen ein Jahr, index.html immer
/// neu pruefen, kleine API-Antworten kurz im Browser, alles andere nie. Kein Referrer: die
/// besuchte Seite erfaehrt nicht, wonach gesucht wurde.
async fn kopfzeilen(req: Request, next: Next) -> Response {
    let pfad = req.uri().path().to_owned();
    let mut antwort = next.run(req).await;
    let cache = if pfad.starts_with("/assets/") && pfad.contains("-dxh") {
        "public, max-age=31536000, immutable"
    } else if ["/api/kurz", "/api/ort", "/api/vorschlaege", "/api/geo", "/api/engines", "/api/wetter"].contains(&pfad.as_str()) {
        "private, max-age=600"
    } else if pfad == "/api/kurs" {
        "private, max-age=60"
    } else if pfad.starts_with("/api/") {
        "no-store"
    } else {
        "no-cache"
    };
    let k = antwort.headers_mut();
    k.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static(cache));
    k.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    k.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    antwort
}

fn fehler(status: StatusCode, wert: Value) -> Response {
    (status, Json(wert)).into_response()
}

/// GET /api/suche - die Suche, jeder Treffer kommt sofort.
async fn api_suche(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Query(p): Params) -> Response {
    let p = search::SucheParams::neu(p);
    if p.q.trim().is_empty() {
        return fehler(StatusCode::BAD_REQUEST, json!({"fehler": "Suchbegriff fehlt."}));
    }
    // Spam-Schutz: eine Suche je 2 Sekunden im Schnitt, kurz auch mehr.
    if let Some(r) = gebremst(&st, peer, &kopf, "suche", 30.0, 15.0) { return r; }
    st.statistik.suche();
    let strom = ReceiverStream::new(search::suche_starten(st, p))
        .map(|(name, daten)| Ok::<_, std::convert::Infallible>(Event::default().event(name).data(&*daten)));
    Sse::new(strom).keep_alive(KeepAlive::default()).into_response()
}

/// POST /api/ki - die Antwort der KI als Strom (siehe ki.rs).
async fn api_ki(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Json(anfrage): Json<Value>) -> Response {
    // Die KI kostet Tokens: hoechstens 10 Anfragen je Minute (kurz 5 am Stueck).
    if let Some(r) = gebremst(&st, peer, &kopf, "ki", 10.0, 5.0) { return r; }
    let strom = ReceiverStream::new(ki::starten(st, anfrage))
        .map(|(name, daten)| Ok::<_, std::convert::Infallible>(Event::default().event(name).data(daten.to_string())));
    Sse::new(strom).keep_alive(KeepAlive::default()).into_response()
}

/// POST /api/uebersetzen {text, ziel} -> {quelle, text}
async fn api_uebersetzen(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    // Beim Tippen kommt oefter etwas - aber nicht endlos.
    if let Some(r) = gebremst(&st, peer, &kopf, "uebersetzen", 30.0, 10.0) { return r; }
    match ki::uebersetzen(&st, util::s(&a, "text"), util::s(&a, "ziel")).await {
        Ok((quelle, text)) => Json(json!({"quelle": quelle, "text": text})).into_response(),
        Err(f) => fehler(StatusCode::BAD_REQUEST, json!({"fehler": f})),
    }
}

/// POST /api/datei?name=… - der Text einer Datei (PDF, Office, OpenDocument).
async fn api_datei(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Query(p): Params, daten: axum::body::Bytes) -> Response {
    if let Some(r) = gebremst(&st, peer, &kopf, "datei", 30.0, 20.0) { return r; }
    let name = p.get("name").cloned().unwrap_or_default();
    match tokio::task::spawn_blocking(move || datei::lesen(&name, &daten)).await {
        Ok(Ok((text, seiten))) => Json(json!({"text": text, "seiten": seiten})).into_response(),
        Ok(Err(f)) => fehler(StatusCode::UNPROCESSABLE_ENTITY, json!({"fehler": f})),
        Err(_) => fehler(StatusCode::UNPROCESSABLE_ENTITY, json!({"fehler": "Die Datei lässt sich nicht lesen."})),
    }
}

// ---- Spam-Schutz ----------------------------------------------------------------

/// Die IP des Nutzers - hinter einem Proxy aus X-Forwarded-For.
fn kunde(peer: SocketAddr, kopf: &HeaderMap) -> IpAddr {
    kopf.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next()).and_then(|v| v.trim().parse().ok()).unwrap_or(peer.ip())
}

/// Darf dieser Nutzer das gerade? Sonst 429 mit einem Satz.
fn gebremst(st: &AppState, peer: SocketAddr, kopf: &HeaderMap, art: &'static str, pro_minute: f64, stoss: f64) -> Option<Response> {
    (!st.schutz.erlaubt(kunde(peer, kopf), art, pro_minute, stoss))
        .then(|| fehler(StatusCode::TOO_MANY_REQUESTS, json!({"fehler": "Zu viele Anfragen in kurzer Zeit - bitte einen Moment warten."})))
}

// ---- Ablage: Chats und Arbeitsbereiche je Browser ---------------------------------

/// Die Ablage zum Token des Browsers ("Authorization: Bearer t_…").
fn ablage_von(st: &AppState, kopf: &HeaderMap) -> Result<ablage::Ablage, Response> {
    let token = kopf.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    st.ablagen.oeffnen(token).map_err(|f| fehler(StatusCode::UNAUTHORIZED, json!({"fehler": f})))
}

/// POST /api/ablage/laden {teil} - ein Teil entschluesselt (JSON oder null).
async fn api_ablage_laden(State(st): St, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    let ablage = match ablage_von(&st, &kopf) { Ok(x) => x, Err(r) => return r };
    match ablage.lesen(util::s(&a, "teil")) {
        Ok(d) => Json(json!({"daten": d.and_then(|d| serde_json::from_slice::<Value>(&d).ok())})).into_response(),
        Err(f) => fehler(StatusCode::INTERNAL_SERVER_ERROR, json!({"fehler": f})),
    }
}

/// POST /api/ablage/speichern {teil, daten} - daten: null entfernt den Teil.
async fn api_ablage_speichern(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    if let Some(r) = gebremst(&st, peer, &kopf, "ablage", 120.0, 60.0) { return r; }
    let ablage = match ablage_von(&st, &kopf) { Ok(x) => x, Err(r) => return r };
    let teil = util::s(&a, "teil");
    if teil.is_empty() || teil.len() > 100 {
        return fehler(StatusCode::BAD_REQUEST, json!({"fehler": "Was soll gespeichert werden?"}));
    }
    let ergebnis = if a["daten"].is_null() { ablage.entfernen(teil); Ok(()) } else { ablage.schreiben(teil, a["daten"].to_string().as_bytes()) };
    match ergebnis {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(f) => fehler(StatusCode::INTERNAL_SERVER_ERROR, json!({"fehler": f})),
    }
}

/// GET /api/statistik - Suchen, KI-Antworten, Tokens (nur Zahlen).
async fn api_statistik(State(st): St) -> Json<Value> {
    Json(st.statistik.json())
}

/// GET /api/ki/status - ist ein Schluessel da? (Der Schluessel selbst nie.)
async fn api_ki_status(State(st): St) -> Json<Value> {
    Json(st.ki.status().await)
}

// ---- Admin (/admin) ----------------------------------------------------------------

fn bearer(kopf: &HeaderMap) -> String {
    kopf.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("").to_string()
}

/// Nur mit offener Admin-Sitzung weiter.
fn nur_admin(st: &AppState, kopf: &HeaderMap) -> Option<Response> {
    (!st.admin.pruefen(&bearer(kopf))).then(|| fehler(StatusCode::UNAUTHORIZED, json!({"fehler": "Bitte als Admin anmelden."})))
}

/// GET /api/admin/status - eingerichtet? angemeldet? (Profil nur fuer den Admin.)
async fn api_admin_status(State(st): St, kopf: HeaderMap) -> Json<Value> {
    let angemeldet = st.admin.pruefen(&bearer(&kopf));
    Json(json!({"eingerichtet": st.admin.eingerichtet(), "angemeldet": angemeldet, "profil": if angemeldet { st.admin.profil() } else { Value::Null }}))
}

/// POST /api/admin/einrichten {email, passwort, name, schluessel?, modell?} -
/// nur beim allerersten Start.
async fn api_admin_einrichten(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    if let Some(r) = gebremst(&st, peer, &kopf, "admin", 10.0, 5.0) { return r; }
    let s = st.clone();
    let (email, passwort, name) = (util::s(&a, "email").to_string(), util::s(&a, "passwort").to_string(), util::s(&a, "name").to_string());
    let token = match tokio::task::spawn_blocking(move || s.admin.einrichten(&email, &passwort, &name)).await {
        Ok(Ok(t)) => t,
        Ok(Err(f)) => return fehler(StatusCode::BAD_REQUEST, json!({"fehler": f})),
        Err(_) => return fehler(StatusCode::INTERNAL_SERVER_ERROR, json!({"fehler": "Einrichten abgebrochen."})),
    };
    // OpenRouter gleich mit - ein falscher Schluessel haelt die Einrichtung nicht auf.
    let ki = st.ki.admin_setzen(&st.client, a["schluessel"].as_str(), a["modell"].as_str()).await.err();
    Json(json!({"token": token, "profil": st.admin.profil(), "ki_fehler": ki})).into_response()
}

/// POST /api/admin/anmelden {email, passwort}
async fn api_admin_anmelden(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    if let Some(r) = gebremst(&st, peer, &kopf, "admin", 10.0, 5.0) { return r; }
    let s = st.clone();
    let (email, passwort) = (util::s(&a, "email").to_string(), util::s(&a, "passwort").to_string());
    match tokio::task::spawn_blocking(move || s.admin.anmelden(&email, &passwort)).await {
        Ok(Ok(token)) => Json(json!({"token": token, "profil": st.admin.profil()})).into_response(),
        _ => {
            tokio::time::sleep(Duration::from_millis(800)).await;
            fehler(StatusCode::UNAUTHORIZED, json!({"fehler": "E-Mail oder Passwort stimmen nicht."}))
        }
    }
}

async fn api_admin_abmelden(State(st): St, kopf: HeaderMap) -> Json<Value> {
    st.admin.abmelden(&bearer(&kopf));
    Json(json!({"ok": true}))
}

/// GET /api/admin/uebersicht - Insights, KI, Module, Ablagen.
async fn api_admin_uebersicht(State(st): St, kopf: HeaderMap) -> Response {
    if let Some(r) = nur_admin(&st, &kopf) { return r; }
    let module: Value = match tokio::fs::read(&st.module).await {
        Ok(d) => serde_json::from_slice(&d).unwrap_or_default(),
        Err(_) => tokio::fs::read("module.json").await.ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default(),
    };
    let (browser, groesse) = st.ablagen.uebersicht();
    Json(json!({"statistik": st.statistik.json(), "ki": st.ki.status().await, "module": module, "version": env!("CARGO_PKG_VERSION"),
        "instanz": st.instanz.adresse(), "searxng": st.searxng.as_ref().map(|s| s.status()), "ablagen": {"browser": browser, "groesse": groesse}, "profil": st.admin.profil()})).into_response()
}

/// POST /api/admin/module {module: {...}} - die ganze module.json.
async fn api_admin_module(State(st): St, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    if let Some(r) = nur_admin(&st, &kopf) { return r; }
    if !a["module"]["module"].is_object() {
        return fehler(StatusCode::BAD_REQUEST, json!({"fehler": "Die Konfiguration braucht ein Objekt \"module\"."}));
    }
    match tokio::fs::write(&st.module, serde_json::to_string_pretty(&a["module"]).unwrap_or_default()).await {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(e) => fehler(StatusCode::INTERNAL_SERVER_ERROR, json!({"fehler": format!("Speichern ging nicht ({}): {e}", st.module)})),
    }
}

/// POST /api/admin/ki {schluessel?, modell?}
async fn api_admin_ki(State(st): St, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    if let Some(r) = nur_admin(&st, &kopf) { return r; }
    match st.ki.admin_setzen(&st.client, a["schluessel"].as_str(), a["modell"].as_str()).await {
        Ok(()) => Json(st.ki.status().await).into_response(),
        Err(f) => fehler(StatusCode::BAD_REQUEST, json!({"fehler": f})),
    }
}

/// POST /api/admin/profil {name?, bild?, passwort?}
async fn api_admin_profil(State(st): St, kopf: HeaderMap, Json(a): Json<Value>) -> Response {
    if let Some(r) = nur_admin(&st, &kopf) { return r; }
    let s = st.clone();
    match tokio::task::spawn_blocking(move || s.admin.profil_aendern(&a)).await {
        Ok(Ok(())) => Json(st.admin.profil()).into_response(),
        Ok(Err(f)) => fehler(StatusCode::BAD_REQUEST, json!({"fehler": f})),
        Err(_) => fehler(StatusCode::INTERNAL_SERVER_ERROR, json!({"fehler": "Abgebrochen."})),
    }
}

/// POST /api/admin/searxng - die eingebaute Instanz jetzt aktualisieren (Admin).
async fn api_admin_searxng(State(st): St, kopf: HeaderMap) -> Response {
    if let Some(r) = nur_admin(&st, &kopf) {
        return r;
    }
    match &st.searxng {
        Some(sx) => {
            sx.jetzt_pruefen();
            Json(json!({"ok": true, "searxng": sx.status()})).into_response()
        }
        None => fehler(StatusCode::BAD_REQUEST, json!({"fehler": "Die eingebaute SearXNG-Instanz ist abgeschaltet."})),
    }
}

/// GET /api/search - dieselbe Suche in einem Rutsch.
async fn api_search(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap, Query(p): Params) -> Response {
    let p = search::SucheParams::neu(p);
    if p.q.trim().is_empty() {
        return fehler(StatusCode::BAD_REQUEST, json!({"fehler": "Suchbegriff fehlt."}));
    }
    // Zu jeder Suche holt die Seite Videos, Bilder, News - dafuer mehr Luft.
    if let Some(r) = gebremst(&st, peer, &kopf, "search", 90.0, 40.0) { return r; }
    match search::suche_einmal(&st, p).await {
        Some(antwort) => Json(antwort).into_response(),
        None => fehler(
            StatusCode::GATEWAY_TIMEOUT,
            json!({"fehler": "Die Suchinstanz antwortet nicht.", "hinweis": "Die eingebaute SearXNG-Instanz startet noch oder wird gerade installiert (/api/health).".to_string()}),
        ),
    }
}

/// GET /api/ort - Ort, Sehenswuerdigkeit, Person oder Unternehmen.
async fn api_ort(State(st): St, Query(p): Params) -> Json<Value> {
    let frage = p.get("q").map_or("", |q| q.trim());
    let sprache = p.get("sprache").map_or("", |s| s);
    let schluessel = format!("ort|{sprache}|{}", frage.to_lowercase());
    if let Some(v) = st.merken_lesen(&schluessel) {
        return Json(v);
    }
    let ort = if frage.len() < 2 || filter::frage_gesperrt(frage) { None } else { st.place.suchen(frage, sprache).await };
    let antwort = json!({"ort": ort});
    if !antwort["ort"].is_null() {
        st.merken(schluessel, &antwort);
    }
    Json(antwort)
}

/// GET /api/kurz - die schnelle Kurzfassung fuer die Info-Karte.
async fn api_kurz(State(st): St, Query(p): Params) -> Json<Value> {
    let frage = p.get("q").map_or("", |q| q.trim());
    let kurz = if frage.len() < 2 || filter::frage_gesperrt(frage) { None } else { st.place.kurz(frage, p.get("sprache").map_or("", |s| s)).await };
    Json(json!({"kurz": kurz}))
}

/// GET /api/wetter - die Wettervorhersage.
async fn api_wetter(State(st): St, Query(p): Params) -> Response {
    let zahl = |k: &str| p.get(k).and_then(|v| v.parse::<f64>().ok());
    let (Some(lat), Some(lon)) = (zahl("lat"), zahl("lon")) else {
        return fehler(StatusCode::BAD_REQUEST, json!({"wetter": null}));
    };
    // Auf etwa einen Kilometer gerundet zehn Minuten merken.
    let schluessel = format!("wetter|{:.2}|{:.2}", lat, lon);
    if let Some(v) = st.merken_lesen(&schluessel) {
        return Json(v).into_response();
    }
    let antwort = json!({"wetter": market::wetter(&st.client, lat, lon).await});
    if !antwort["wetter"].is_null() {
        st.merken(schluessel, &antwort);
    }
    Json(antwort).into_response()
}

/// GET /api/vorschlaege - die Autovervollstaendigung aus der Instanz. Drei Quellen
/// gleichzeitig, die erste mit Vorschlaegen gewinnt; zehn Minuten gemerkt; durch den
/// Jugendschutz.
async fn api_vorschlaege(State(st): St, Query(p): Params) -> Json<Value> {
    let q = p.get("q").map_or("", |q| q.trim());
    if q.chars().count() < 2 || filter::frage_gesperrt(q) {
        return Json(json!({"vorschlaege": []}));
    }
    let schluessel = format!("vorschlag|{}", q.to_lowercase());
    if let Some(v) = st.merken_lesen(&schluessel) {
        return Json(v);
    }
    let sxng = st.instanz.bereit(Duration::from_secs(5)).await;
    let holen = |engine: &'static str| {
        let url = format!("{sxng}/autocompleter?q={}&engines={engine}", util::enc(q));
        let anfrage = st.client.get(url).timeout(Duration::from_secs(3));
        async move {
            let roh = hole(anfrage).await.unwrap_or_default();
            roh[1]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|v| v.as_str().is_some_and(|t| !filter::frage_gesperrt(t) && filter::punkte(t) < 2))
                .take(8)
                .cloned()
                .collect::<Vec<Value>>()
        }
    };
    let mut laeufe: futures_util::stream::FuturesUnordered<_> = ["duckduckgo", "brave", ""].into_iter().map(holen).collect();
    let mut liste = Vec::new();
    while let Some(l) = laeufe.next().await {
        if !l.is_empty() {
            liste = l;
            break;
        }
    }
    let antwort = json!({"vorschlaege": liste});
    if !liste.is_empty() {
        st.merken(schluessel, &antwort);
    }
    Json(antwort)
}

/// GET /api/engines - alles, was die Instanz kennt.
async fn api_engines(State(st): St) -> Json<Value> {
    let liste = st.verzeichnis.holen(&st.client, &st.instanz.adresse()).await;
    let motoren: Vec<Value> = liste
        .iter()
        .map(|e| json!({"name": e.name, "kurz": e.kurz, "kategorien": e.kategorien, "aktiv": e.aktiv}))
        .collect();
    Json(json!({"motoren": motoren, "anzahl": motoren.len(), "quelle": st.instanz.adresse()}))
}

/// GET /api/version - die Version der Suchinstanz.
async fn api_version(State(st): St) -> Json<Value> {
    let html = match st.client.get(&st.instanz.adresse()).send().await {
        Ok(a) if a.status().is_success() => a.text().await.unwrap_or_default(),
        _ => String::new(),
    };
    match html.split("searxng/").nth(1).and_then(|r| r.split('"').next()).filter(|v| !v.is_empty()) {
        Some(v) => Json(json!({"version": v, "quelle": st.instanz.adresse()})),
        None => Json(json!({"version": null, "quelle": null})),
    }
}

/// GET /api/health - der Zustand.
async fn api_health(State(st): St) -> Json<Value> {
    let instanz = match st.client.get(&st.instanz.adresse()).timeout(Duration::from_secs(5)).send().await {
        Ok(a) if a.status().is_success() => "erreichbar".to_string(),
        Ok(a) => format!("HTTP {}", a.status()),
        Err(_) => "unerreichbar".to_string(),
    };
    Json(json!({"ok": true, "sxng": st.instanz.adresse(), "adressen": st.instanz.adressen(), "instanz": instanz, "port": st.port,
        "searxng": st.searxng.as_ref().map(|s| s.status())}))
}

/// GET /api/kurs - die Kurskarte.
async fn api_kurs(State(st): St, Query(p): Params) -> Response {
    let symbol = p.get("symbol").map_or(String::new(), |s| s.trim().to_uppercase());
    if symbol.is_empty() || !symbol.chars().all(|c| c.is_ascii_alphanumeric() || ".-=^".contains(c)) {
        return fehler(StatusCode::BAD_REQUEST, json!({"fehler": "Unbekanntes Kurs-Symbol.", "kurs": null}));
    }
    Json(st.markt.kurs(&st.client, &symbol, p.get("bereich").map_or("1d", |b| b)).await).into_response()
}

/// GET /api/module - die Konfiguration der Module. Die Datei wird bei
/// jeder Anfrage gelesen: Aenderungen gelten ohne Neustart. Fehlt die
/// eigene Datei (MODULE_CONFIG), gilt die mitgelieferte module.json.
async fn api_module(State(st): St) -> Response {
    let daten = match tokio::fs::read(&st.module).await {
        Ok(d) => Ok(d),
        Err(_) => tokio::fs::read(env("MODULE_STANDARD", "module.json")).await,
    };
    match daten {
        Ok(daten) => ([(header::CONTENT_TYPE, "application/json")], daten).into_response(),
        Err(_) => Json(json!({"module": {}})).into_response(),
    }
}

/// GET /api/ip - die oeffentliche IP-Adresse mit Anbieter und Ort. Kommt die
/// Anfrage aus dem eigenen Netz, ist es die Adresse, mit der dieses Netz
/// im Internet steht.
async fn api_ip(State(st): St, ConnectInfo(peer): ConnectInfo<SocketAddr>, kopf: HeaderMap) -> Json<Value> {
    let weiter = kopf.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next()).and_then(|v| v.trim().parse().ok());
    let ip: IpAddr = weiter.unwrap_or(peer.ip());
    let privat = match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || (v4.octets()[0] == 100 && v4.octets()[1] & 0xc0 == 64),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80,
    };
    let adresse = |url: &'static str| eigene_ip(&st.client, url);
    let (v4, v6) = if privat {
        tokio::join!(adresse("https://api.ipify.org"), adresse("https://api6.ipify.org"))
    } else {
        (ip.is_ipv4().then(|| ip.to_string()), ip.is_ipv6().then(|| ip.to_string()))
    };
    let Some(haupt) = v4.clone().or(v6.clone()) else { return Json(json!({"ip": null})) };
    let info = hole(st.client.get(format!("https://ipwho.is/{haupt}?lang=de")).timeout(Duration::from_secs(5))).await.unwrap_or_default();
    let text = |k: &str| info.pointer(k).and_then(Value::as_str).unwrap_or("");
    let ort: Vec<&str> = ["/city", "/region", "/country"].into_iter().map(text).filter(|t| !t.is_empty()).collect();
    Json(json!({
        "ip": haupt, "v4": v4, "v6": v6,
        "anbieter": Some(text("/connection/isp")).filter(|t| !t.is_empty()).unwrap_or(text("/connection/org")),
        "ort": ort.join(", "),
        "zeitzone": text("/timezone/id"),
    }))
}

/// Die eigene Adresse im Internet, wie ipify sie sieht.
async fn eigene_ip(client: &reqwest::Client, url: &str) -> Option<String> {
    let antwort = client.get(url).timeout(Duration::from_secs(4)).send().await.ok()?;
    antwort.text().await.ok().map(|t| t.trim().to_string()).filter(|t| t.parse::<IpAddr>().is_ok())
}

/// GET /api/geo?q=Tokio - Name, Land, Koordinaten und Zeitzone (Open-Meteo).
async fn api_geo(State(st): St, Query(p): Params) -> Json<Value> {
    let q = p.get("q").map_or("", |q| q.trim());
    if q.is_empty() {
        return Json(json!({"ort": null}));
    }
    let schluessel = format!("geo|{}", q.to_lowercase());
    if let Some(v) = st.merken_lesen(&schluessel) {
        return Json(v);
    }
    let url = format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=de&format=json", util::enc(q));
    let roh = hole(st.client.get(url).timeout(Duration::from_secs(6))).await.unwrap_or_default();
    let ort = roh.pointer("/results/0").map(|o| {
        json!({
            "name": o["name"], "land": o["country"], "region": o["admin1"],
            "lat": o["latitude"], "lon": o["longitude"], "zeitzone": o["timezone"],
        })
    });
    let antwort = json!({"ort": ort});
    if !antwort["ort"].is_null() {
        st.merken(schluessel, &antwort);
    }
    Json(antwort)
}
