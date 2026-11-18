# nrzSearch – Umbrel-Paket

Dieser Zweig enthaelt nur fertige Dateien fuer die App `nrz-nrz-search` im nrzStore:

| Datei | Inhalt |
|-------|--------|
| `nrzsearch-amd64.gz`, `nrzsearch-arm64.gz` | Server (Rust, axum) fuer Linux |
| `web.tar.gz` | Oberflaeche (Rust/WASM, Dioxus + nrzUI), vorkomprimiert |
| `module.json` | mitgelieferte Konfiguration der Module |
| `SHA256SUMS` | Pruefsummen, `hooks/pre-start` prueft sie vor dem Entpacken |

SearXNG ist nicht enthalten: nrzSearch installiert die neueste Fassung beim ersten Start selbst
und aktualisiert sie taeglich. Gebaut mit `scripts/umbrel-release.sh` auf dem Zweig `main`.
