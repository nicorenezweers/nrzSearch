#!/usr/bin/env bash
# Baut die Oberflaeche (Rust -> WASM, Dioxus) und legt sie fertig nach public/ - vorkomprimiert
# (.br/.gz), so liefert der Server sie ohne Rechenarbeit aus.
#   scripts/build-web.sh            -> ./public
#   scripts/build-web.sh /pfad/ziel
# Voraussetzungen: dioxus-cli 0.7 (dx), Rust-Target wasm32-unknown-unknown; optional brotli.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ZIEL="${1:-${ROOT}/public}"
# Pfade ohne Leerzeichen fuer den Build (wasm-bindgen/wasm-opt vertragen sie schlecht).
BUILD="${NRZSEARCH_BUILD_DIR:-/tmp/nrzsearch-build}"

# dioxus-cli finden (`dx` im PATH kann auch Denos dx sein).
DX="${DX:-}"
for c in "${DX}" "$(command -v dx || true)" "${HOME}/.cargo/bin/dx" "/Users/${USER:-x}/.cargo/bin/dx"; do
  if [ -n "${c}" ] && "${c}" --version 2>/dev/null | grep -qi dioxus; then DX="${c}"; break; fi
  DX=""
done
[ -n "${DX}" ] || { echo "dioxus-cli (dx) nicht gefunden - cargo binstall dioxus-cli" >&2; exit 1; }

rm -rf "${BUILD}/webdist"
(cd "${ROOT}/web" && CARGO_TARGET_DIR="${BUILD}/wasm" "${DX}" bundle --platform web --release --out-dir "${BUILD}/webdist")
test -f "${BUILD}/webdist/public/index.html"

rm -rf "${ZIEL}"
mkdir -p "${ZIEL}"
cp -R "${BUILD}/webdist/public/." "${ZIEL}/"
# WASM verkleinern (binaryen), falls dx es nicht schon getan hat.
if command -v wasm-opt >/dev/null 2>&1; then
  for w in "${ZIEL}"/assets/*.wasm; do
    vorher=$(wc -c < "$w")
    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --strip-debug --strip-producers "$w" -o "$w.opt" && mv "$w.opt" "$w"
    echo "wasm-opt: $vorher -> $(wc -c < "$w") Bytes"
  done
fi
# Vorkomprimieren: alles Textartige und das WASM.
find "${ZIEL}" -type f \( -name '*.html' -o -name '*.js' -o -name '*.css' -o -name '*.svg' -o -name '*.wasm' -o -name '*.json' \) | while read -r f; do
  gzip -9 -k -f "$f"
  if command -v brotli >/dev/null 2>&1; then brotli -q 11 -k -f "$f"; fi
done
du -sh "${ZIEL}"
ls -la "${ZIEL}/assets" | grep -E '\.wasm(\.br)?$' || true
