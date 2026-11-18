// nrzSearch - die eingebaute SearXNG-Instanz.
//
// nrzSearch braucht keine externe Instanz: der Server holt sich die neueste
// SearXNG-Fassung selbst (Quellcode von GitHub, Zweig master - genau das, was
// auch im offiziellen Docker-Image steckt), legt eine eigene Python-Umgebung
// dafuer an und startet SearXNG mit granian nur auf 127.0.0.1. Die
// Einstellungen kommen aus searxng/settings.yml (eingebaut).
//
// Automatische Updates: alle SEARXNG_UPDATE_STUNDEN (Vorgabe 24) wird
// nachgesehen, ob es eine neuere Fassung gibt. Sie wird daneben installiert,
// auf dem zweiten Port gestartet und geprueft (healthz + eine echte Suche).
// Erst wenn sie laeuft, schaltet nrzSearch um und beendet die alte - die
// Suche faellt dabei nicht aus. Klappt etwas nicht, bleibt die alte Fassung.
// Stuerzt SearXNG ab, wird es neu gestartet.
//
// Ablage (SEARXNG_DIR):
//   versionen/<JJJJ.MM.TT-sha>/{src,venv,.fertig}   installierte Fassungen
//   aktuell                                        die laufende Fassung
//   settings.yml, secret, searxng.log
// Ein Image kann eine Fassung vorab mitbringen (SEARXNG_VORAB, gleicher
// Aufbau wie ein Ordner unter versionen/) - dann geht die Suche auch beim
// allerersten Start ohne Wartezeit.

use crate::instanz::Instanz;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::{Child, Command};

const EINSTELLUNGEN: &str = include_str!("../searxng/settings.yml");
const GITHUB_API: &str = "https://api.github.com/repos/searxng/searxng/commits/master";
const QUELLCODE: &str = "https://codeload.github.com/searxng/searxng/tar.gz";

/// Was die Oberflaeche (Admin, /api/health) ueber die eingebaute Instanz erfaehrt.
#[derive(Default, Clone)]
struct Status {
    version: String,
    port: u16,
    laeuft: bool,
    meldung: String,
    neueste: String,
    letzte_pruefung: u64,
    letztes_update: u64,
}

struct Laufend {
    kind: Child,
    port: u16,
    version: String,
    ordner: PathBuf,
}

pub struct Eingebaut {
    dir: PathBuf,
    python: String,
    vorab: Option<PathBuf>,
    port_basis: u16,
    alle: Option<Duration>,
    status: Mutex<Status>,
    laufend: tokio::sync::Mutex<Option<Laufend>>,
    /// Ein Update auf Wunsch (Admin) - weckt die Schleife.
    jetzt: tokio::sync::Notify,
}

fn jetzt_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Eine Fassung, wie sie GitHub meldet.
struct Fassung {
    sha: String,
    /// "2026.11.13-219b63b20" - wie die Tags des offiziellen Docker-Images.
    id: String,
}

impl Eingebaut {
    pub fn neu() -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let stunden: f64 = env("SEARXNG_UPDATE_STUNDEN").and_then(|v| v.parse().ok()).unwrap_or(24.0);
        Self {
            dir: PathBuf::from(env("SEARXNG_DIR").unwrap_or_else(|| "searxng-intern".into())),
            python: env("SEARXNG_PYTHON").unwrap_or_else(|| "python3".into()),
            vorab: env("SEARXNG_VORAB").map(PathBuf::from),
            port_basis: env("SEARXNG_PORT").and_then(|p| p.parse().ok()).unwrap_or(8888),
            alle: (stunden > 0.0).then(|| Duration::from_secs_f64(stunden * 3600.0)),
            status: Mutex::new(Status::default()),
            laufend: tokio::sync::Mutex::new(None),
            jetzt: tokio::sync::Notify::new(),
        }
    }

    fn melden(&self, f: impl FnOnce(&mut Status)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }

    /// Der Zustand fuer /api/health und das Admin-Dashboard.
    pub fn status(&self) -> Value {
        let s = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        json!({
            "eingebaut": true, "version": s.version, "port": s.port, "laeuft": s.laeuft, "meldung": s.meldung,
            "neueste": s.neueste, "letztePruefung": s.letzte_pruefung, "letztesUpdate": s.letztes_update,
            "automatisch": self.alle.map(|d| d.as_secs() / 3600),
        })
    }

    /// Jetzt nach einem Update sehen (Admin).
    pub fn jetzt_pruefen(&self) {
        self.jetzt.notify_one();
    }

    // ---- Ablage ---------------------------------------------------------------------------

    fn versionen(&self) -> PathBuf {
        self.dir.join("versionen")
    }

    fn settings(&self) -> PathBuf {
        self.dir.join("settings.yml")
    }

    /// Der geheime Schluessel von SearXNG: SEARXNG_SECRET oder einmal zufaellig erzeugt.
    fn secret(&self) -> String {
        if let Ok(s) = std::env::var("SEARXNG_SECRET") {
            if !s.trim().is_empty() {
                return s;
            }
        }
        let pfad = self.dir.join("secret");
        if let Ok(s) = std::fs::read_to_string(&pfad) {
            if s.trim().len() >= 16 {
                return s.trim().to_string();
            }
        }
        let mut b = [0u8; 24];
        let _ = getrandom::getrandom(&mut b);
        let s: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let _ = std::fs::write(&pfad, &s);
        s
    }

    fn fertig(ordner: &Path) -> bool {
        ordner.join(".fertig").exists() && ordner.join("venv/bin/granian").exists() && ordner.join("src/searx/webapp.py").exists()
    }

    /// Die Fassung, die zuletzt lief (oder die vorab mitgelieferte).
    fn installiert(&self) -> Option<(String, PathBuf)> {
        if let Ok(id) = std::fs::read_to_string(self.dir.join("aktuell")) {
            let ordner = self.versionen().join(id.trim());
            if Self::fertig(&ordner) {
                return Some((id.trim().to_string(), ordner));
            }
        }
        // Sonst die neueste fertige unter versionen/.
        let mut alle: Vec<String> = std::fs::read_dir(self.versionen())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| Self::fertig(&e.path()))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        alle.sort();
        if let Some(id) = alle.pop() {
            let ordner = self.versionen().join(&id);
            return Some((id, ordner));
        }
        let vorab = self.vorab.as_ref().filter(|v| Self::fertig(v))?;
        let id = std::fs::read_to_string(vorab.join("version")).map_or_else(|_| "vorab".into(), |s| s.trim().to_string());
        Some((id, vorab.clone()))
    }

    // ---- GitHub, Installation -------------------------------------------------------------

    async fn neueste(client: &reqwest::Client) -> Result<Fassung, String> {
        let antwort = client
            .get(GITHUB_API)
            .header("accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| format!("GitHub nicht erreichbar: {e}"))?;
        if !antwort.status().is_success() {
            return Err(format!("GitHub: HTTP {}", antwort.status()));
        }
        let v: Value = antwort.json().await.map_err(|e| format!("GitHub: {e}"))?;
        let sha = v["sha"].as_str().filter(|s| s.len() >= 9).ok_or("GitHub: keine Fassung")?.to_string();
        let datum = v.pointer("/commit/committer/date").and_then(Value::as_str).unwrap_or("0000-00-00");
        let tag = datum.get(..10).unwrap_or("0000-00-00").replace('-', ".");
        Ok(Fassung { id: format!("{tag}-{}", &sha[..9]), sha })
    }

    async fn befehl(&self, programm: &str, args: &[&str], ordner: &Path, frist: Duration) -> Result<(), String> {
        let lauf = Command::new(programm)
            .args(args)
            .current_dir(ordner)
            .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
            .env("PIP_NO_CACHE_DIR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output();
        match tokio::time::timeout(frist, lauf).await {
            Err(_) => Err(format!("{programm}: Zeit abgelaufen")),
            Ok(Err(e)) => Err(format!("{programm}: {e}")),
            Ok(Ok(aus)) if aus.status.success() => Ok(()),
            Ok(Ok(aus)) => {
                let fehler = String::from_utf8_lossy(&aus.stderr);
                let ende: String = fehler.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
                Err(format!("{programm} {}: {ende}", args.first().unwrap_or(&"")))
            }
        }
    }

    /// Eine Fassung installieren: Quellcode laden, Python-Umgebung, Abhaengigkeiten.
    async fn installieren(&self, client: &reqwest::Client, f: &Fassung) -> Result<PathBuf, String> {
        let ziel = self.versionen().join(&f.id);
        if Self::fertig(&ziel) {
            return Ok(ziel);
        }
        let _ = tokio::fs::remove_dir_all(&ziel).await;
        let src = ziel.join("src");
        tokio::fs::create_dir_all(&src).await.map_err(|e| format!("{}: {e}", src.display()))?;
        self.melden(|s| s.meldung = format!("Lade SearXNG {} …", f.id));

        // 1. Quellcode (tar.gz) laden und entpacken.
        let archiv = ziel.join("quelle.tar.gz");
        let daten = client
            .get(format!("{QUELLCODE}/{}", f.sha))
            .timeout(Duration::from_secs(300))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Quellcode: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("Quellcode: {e}"))?;
        tokio::fs::write(&archiv, &daten).await.map_err(|e| format!("Quellcode: {e}"))?;
        let archiv_s = archiv.to_string_lossy().to_string();
        self.befehl("tar", &["-xzf", &archiv_s, "-C", "src", "--strip-components=1"], &ziel, Duration::from_secs(120)).await?;
        let _ = tokio::fs::remove_file(&archiv).await;

        // 2. Python-Umgebung und Abhaengigkeiten (fertige Wheels fuer amd64/arm64).
        self.melden(|s| s.meldung = format!("Installiere SearXNG {} …", f.id));
        self.befehl(&self.python, &["-m", "venv", "venv"], &ziel, Duration::from_secs(120)).await?;
        self.befehl(
            "venv/bin/pip",
            &["install", "-q", "-r", "src/requirements.txt", "-r", "src/requirements-server.txt"],
            &ziel,
            Duration::from_secs(900),
        )
        .await?;

        // 3. Die Versionsangabe, die SearXNG sonst aus git liest.
        let ver = f.id.replacen('-', "+", 1);
        let frozen = format!(
            "# erzeugt von nrzSearch\nVERSION_STRING = \"{ver}\"\nVERSION_TAG = \"{ver}\"\nDOCKER_TAG = \"{}\"\nGIT_URL = \"https://github.com/searxng/searxng\"\nGIT_BRANCH = \"master\"\n",
            f.id
        );
        tokio::fs::write(src.join("searx/version_frozen.py"), frozen).await.map_err(|e| e.to_string())?;
        tokio::fs::write(ziel.join("version"), &f.id).await.map_err(|e| e.to_string())?;
        tokio::fs::write(ziel.join(".fertig"), jetzt_s().to_string()).await.map_err(|e| e.to_string())?;
        Ok(ziel)
    }

    // ---- Starten, pruefen, beenden --------------------------------------------------------

    fn starten_auf(&self, ordner: &Path, port: u16) -> Result<Child, String> {
        let log = std::fs::File::create(self.dir.join(format!("searxng-{port}.log"))).map_err(|e| e.to_string())?;
        let log2 = log.try_clone().map_err(|e| e.to_string())?;
        let src = ordner.join("src");
        let mut c = Command::new(ordner.join("venv/bin/granian"));
        c.arg("searx.webapp:app")
            .current_dir(&src)
            .env("GRANIAN_INTERFACE", "wsgi")
            .env("GRANIAN_HOST", "127.0.0.1")
            .env("GRANIAN_PORT", port.to_string())
            .env("GRANIAN_WEBSOCKETS", "false")
            .env("GRANIAN_WORKERS", "1")
            .env("GRANIAN_BLOCKING_THREADS", "4")
            .env("GRANIAN_BLOCKING_THREADS_IDLE_TIMEOUT", "5m")
            .env("GRANIAN_PROCESS_NAME", "searxng")
            .env("SEARXNG_SETTINGS_PATH", self.settings())
            .env("SEARXNG_SECRET", self.secret())
            .env("PYTHONPATH", &src)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log2))
            .kill_on_drop(true);
        #[cfg(unix)]
        c.process_group(0);
        let kind = c.spawn().map_err(|e| format!("SearXNG startet nicht: {e}"))?;
        if let Some(pid) = kind.id() {
            let _ = std::fs::write(self.dir.join(format!("searxng-{port}.pid")), pid.to_string());
        }
        Ok(kind)
    }

    /// Reste eines frueheren Laufs (nrzSearch hart beendet) aufraeumen: deren Prozessgruppen
    /// beenden, damit die Ports frei werden.
    fn reste_beenden(&self) {
        let Ok(eintraege) = std::fs::read_dir(&self.dir) else { return };
        for e in eintraege.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.starts_with("searxng-") && name.ends_with(".pid")) {
                continue;
            }
            #[cfg(unix)]
            if let Some(pid) = std::fs::read_to_string(e.path()).ok().and_then(|p| p.trim().parse::<i32>().ok()).filter(|p| *p > 1) {
                // SAFETY: nur Signale an die frueher von uns gestartete Prozessgruppe.
                unsafe {
                    libc::kill(-pid, libc::SIGTERM);
                }
            }
            let _ = std::fs::remove_file(e.path());
        }
        std::thread::sleep(Duration::from_millis(800));
    }

    /// Beim Beenden von nrzSearch: SearXNG mitnehmen.
    pub async fn stoppen(&self) {
        if let Some(l) = self.laufend.lock().await.take() {
            let pid = self.dir.join(format!("searxng-{}.pid", l.port));
            Self::beenden(l.kind).await;
            let _ = std::fs::remove_file(pid);
        }
    }

    /// Laeuft SearXNG auf dem Port und liefert eine echte JSON-Suche?
    async fn bereit(client: &reqwest::Client, kind: &mut Child, port: u16) -> bool {
        let basis = format!("http://127.0.0.1:{port}");
        for _ in 0..120 {
            if kind.try_wait().ok().flatten().is_some() {
                return false;
            }
            let ok = client.get(format!("{basis}/healthz")).timeout(Duration::from_secs(2)).send().await;
            if matches!(ok, Ok(a) if a.status().is_success()) {
                let probe = client
                    .get(format!("{basis}/search?q=nrzsearch&format=json&engines=duckduckgo,brave,bing"))
                    .timeout(Duration::from_secs(15))
                    .send()
                    .await;
                return match probe {
                    Ok(a) if a.status().is_success() => a.json::<Value>().await.is_ok_and(|v| v.get("results").is_some()),
                    _ => false,
                };
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }

    /// Einen Prozess (samt granian-Arbeitern) sanft beenden, notfalls hart.
    async fn beenden(mut kind: Child) {
        #[cfg(unix)]
        if let Some(pid) = kind.id() {
            // SAFETY: Signal an die eigene Prozessgruppe des Kindes (process_group(0) oben).
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
            if tokio::time::timeout(Duration::from_secs(15), kind.wait()).await.is_ok() {
                return;
            }
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        let _ = kind.kill().await;
    }

    /// Eine Fassung starten (auf dem freien der beiden Ports), pruefen und umschalten.
    async fn umschalten(&self, client: &reqwest::Client, instanz: &Instanz, id: &str, ordner: &Path) -> Result<(), String> {
        let mut laufend = self.laufend.lock().await;
        let belegt = laufend.as_ref().map(|l| l.port);
        let port = (self.port_basis..self.port_basis + 20)
            .find(|p| Some(*p) != belegt && std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok())
            .ok_or("kein freier Port fuer SearXNG")?;
        self.melden(|s| s.meldung = format!("Starte SearXNG {id} …"));
        let mut kind = self.starten_auf(ordner, port)?;
        if !Self::bereit(client, &mut kind, port).await {
            Self::beenden(kind).await;
            return Err(format!("SearXNG {id} antwortet nicht (Log: {})", self.dir.join(format!("searxng-{port}.log")).display()));
        }
        instanz.eingebaut_setzen(Some(format!("http://127.0.0.1:{port}")));
        let alt = laufend.replace(Laufend { kind, port, version: id.to_string(), ordner: ordner.to_path_buf() });
        drop(laufend);
        let _ = std::fs::write(self.dir.join("aktuell"), id);
        self.melden(|s| {
            s.version = id.to_string();
            s.port = port;
            s.laeuft = true;
            s.meldung = format!("SearXNG {id} läuft");
        });
        println!("SearXNG (eingebaut): {id} auf 127.0.0.1:{port}");
        let vorige = match alt {
            Some(alt) => {
                let _ = std::fs::remove_file(self.dir.join(format!("searxng-{}.pid", alt.port)));
                Self::beenden(alt.kind).await;
                if alt.version != id {
                    self.melden(|s| s.letztes_update = jetzt_s());
                }
                alt.version
            }
            None => String::new(),
        };
        self.aufraeumen(id, &vorige).await;
        Ok(())
    }

    /// Alte Fassungen loeschen - die laufende und die vorige (fuer den Notfall) bleiben.
    async fn aufraeumen(&self, aktuell: &str, vorige: &str) {
        let Ok(mut eintraege) = tokio::fs::read_dir(self.versionen()).await else { return };
        while let Ok(Some(e)) = eintraege.next_entry().await {
            let name = e.file_name().to_string_lossy().to_string();
            if name != aktuell && name != vorige {
                let _ = tokio::fs::remove_dir_all(e.path()).await;
            }
        }
    }

    /// Nach einer neueren Fassung sehen und sie ohne Unterbrechung einspielen.
    async fn aktualisieren(&self, client: &reqwest::Client, instanz: &Instanz) {
        self.melden(|s| s.letzte_pruefung = jetzt_s());
        let f = match Self::neueste(client).await {
            Ok(f) => f,
            Err(e) => {
                self.melden(|s| s.meldung = e.clone());
                eprintln!("SearXNG-Update: {e}");
                return;
            }
        };
        self.melden(|s| s.neueste = f.id.clone());
        let jetzt = self.laufend.lock().await.as_ref().map(|l| l.version.clone());
        if jetzt.as_deref() == Some(f.id.as_str()) {
            self.melden(|s| s.meldung = format!("SearXNG {} ist aktuell", f.id));
            return;
        }
        let ergebnis = match self.installieren(client, &f).await {
            Ok(ordner) => self.umschalten(client, instanz, &f.id, &ordner).await,
            Err(e) => Err(e),
        };
        if let Err(e) = ergebnis {
            let _ = tokio::fs::remove_dir_all(self.versionen().join(&f.id)).await;
            let weiter = if jetzt.is_some() { " - die bisherige Fassung läuft weiter" } else { "" };
            self.melden(|s| s.meldung = format!("Update auf {} fehlgeschlagen: {e}{weiter}", f.id));
            eprintln!("SearXNG-Update auf {}: {e}", f.id);
        }
    }

    /// Der Dauerlauf: einrichten, starten, ueberwachen, regelmaessig aktualisieren.
    pub async fn laufen(&self, client: reqwest::Client, instanz: &Instanz) {
        if let Err(e) = std::fs::create_dir_all(self.versionen()) {
            self.melden(|s| s.meldung = format!("{}: {e}", self.dir.display()));
            eprintln!("SearXNG: {} nicht anlegbar: {e}", self.dir.display());
            return;
        }
        let _ = std::fs::write(self.settings(), EINSTELLUNGEN);
        self.reste_beenden();

        // Erst das, was schon da ist - sofort suchbereit. Sonst die neueste Fassung holen.
        match self.installiert() {
            Some((id, ordner)) => {
                if let Err(e) = self.umschalten(&client, instanz, &id, &ordner).await {
                    eprintln!("SearXNG: {e}");
                    self.melden(|s| s.meldung = e);
                    self.aktualisieren(&client, instanz).await;
                } else if self.alle.is_some() {
                    // Gleich nach dem Start einmal nachsehen (im Hintergrund, die Suche laeuft schon).
                    self.aktualisieren(&client, instanz).await;
                }
            }
            None => self.aktualisieren(&client, instanz).await,
        }

        let mut naechste = tokio::time::Instant::now() + self.alle.unwrap_or(Duration::from_secs(3600));
        let mut abstuerze = 0u32;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                _ = self.jetzt.notified() => {
                    self.aktualisieren(&client, instanz).await;
                    continue;
                }
            }
            // Abgestuerzt? Neu starten (mit Pause, wenn es immer wieder passiert).
            let tot = {
                let mut l = self.laufend.lock().await;
                match l.as_mut() {
                    Some(x) => x.kind.try_wait().ok().flatten().is_some(),
                    None => true,
                }
            };
            if tot {
                self.melden(|s| s.laeuft = false);
                abstuerze += 1;
                tokio::time::sleep(Duration::from_secs(u64::from(abstuerze.min(6)) * 5)).await;
                let wieder = self.laufend.lock().await.take().map(|l| (l.version, l.ordner)).or_else(|| self.installiert());
                match wieder {
                    Some((id, ordner)) => {
                        if self.umschalten(&client, instanz, &id, &ordner).await.is_ok() {
                            abstuerze = 0;
                        }
                    }
                    None => self.aktualisieren(&client, instanz).await,
                }
                continue;
            }
            if self.alle.is_some() && tokio::time::Instant::now() >= naechste {
                naechste = tokio::time::Instant::now() + self.alle.unwrap_or(Duration::from_secs(86400));
                self.aktualisieren(&client, instanz).await;
            }
        }
    }
}
