#!/usr/bin/env bash
# Baut nrzSearch (Server fuer Linux amd64 + arm64, Oberflaeche als WASM) und veroeffentlicht:
#   1. Zweig `umbrel` + Tag `umbrel-<version>` (fertige Dateien) -> nrzGit traegt die Version per
#      nrzStore.json selbst im nrzStore ein (RELEASE in hooks/pre-start, version, releaseNotes).
#   2. Docker-Image <nrzgit>/nicorenezweers/nrzsearch:<version> (+ :latest), Multi-Arch, mit
#      vorinstallierter SearXNG-Fassung (aktualisiert sich danach selbst).
#   3. Release v<version> auf nrzGit mit den Programmdateien.
#
#   ./scripts/umbrel-release.sh 3.0.0 "Kurze Beschreibung"
#   NUR_PAKET=1 ./scripts/umbrel-release.sh ...   (ohne Image und Release)
#
# Voraussetzungen: dioxus-cli 0.7 (dx), zig + cargo-zigbuild, Targets x86_64/aarch64-unknown-linux-gnu
# und wasm32-unknown-unknown, binaryen (wasm-opt), brotli, docker buildx, curl.
set -euo pipefail

VERSION="${1:?Version fehlt, z.B. 3.0.1}"
NOTES="${2:-nrzSearch ${VERSION}}"
TAG="umbrel-${VERSION}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"
REGISTRY="${NRZGIT_REGISTRY:-100.78.107.162:8390}"
API="${NRZGIT_URL:-http://100.78.107.162:8390}/api/v1/repos/nicorenezweers/nrzSearch"
IMAGE="${REGISTRY}/nicorenezweers/nrzsearch"
# Git/Docker mit den Zugaengen eines anderen Home-Verzeichnisses (Schluesselbund), falls noetig.
gitz() { HOME="${NRZ_GIT_HOME:-$HOME}" git "$@"; }
dockerz() { HOME="${NRZ_GIT_HOME:-$HOME}" docker "$@"; }

if [ -n "$(git status --porcelain)" ]; then
  echo "Arbeitsverzeichnis nicht sauber - erst committen." >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
  echo "Tag ${TAG} gibt es schon." >&2
  exit 1
fi
cargo zigbuild --help >/dev/null 2>&1 || { echo "cargo-zigbuild fehlt" >&2; exit 1; }

# Pfade ohne Leerzeichen (zig-Wrapper und C-Builds wie mimalloc vertragen keine).
BUILD="${NRZSEARCH_BUILD_DIR:-/tmp/nrzsearch-build}"
export CARGO_ZIGBUILD_CACHE_DIR="${BUILD}/zigcache"
mkdir -p "${BUILD}/bin"
# Apples ar kann keine ELF-Objekte - ar/ranlib von zig verwenden.
printf '#!/bin/sh\nexec zig ar "$@"\n' > "${BUILD}/bin/zig-ar"
printf '#!/bin/sh\nexec zig ranlib "$@"\n' > "${BUILD}/bin/zig-ranlib"
chmod +x "${BUILD}/bin/zig-ar" "${BUILD}/bin/zig-ranlib"
export AR="${BUILD}/bin/zig-ar" RANLIB="${BUILD}/bin/zig-ranlib"

echo "==> Oberflaeche (Rust/WASM)"
scripts/build-web.sh "${ROOT}/public"

OUT="$(mktemp -d)"
trap 'rm -rf "${OUT}"' EXIT
rm -rf dist && mkdir -p dist
build() { # <rust-target> <arch>
  echo "==> Server ${2} (${1})"
  CARGO_TARGET_DIR="${BUILD}/linux" cargo zigbuild --release --target "${1}.2.31"
  mkdir -p "dist/linux-${2}"
  cp "${BUILD}/linux/${1}/release/nrzsearch" "dist/linux-${2}/nrzsearch"
  gzip -9 -c "dist/linux-${2}/nrzsearch" > "${OUT}/nrzsearch-${2}.gz"
}
build x86_64-unknown-linux-gnu amd64
build aarch64-unknown-linux-gnu arm64
COPYFILE_DISABLE=1 tar --no-xattrs -czf "${OUT}/web.tar.gz" -C public .
cp module.json "${OUT}/module.json"
(cd "${OUT}" && shasum -a 256 nrzsearch-amd64.gz nrzsearch-arm64.gz web.tar.gz module.json > SHA256SUMS)

echo "==> Zweig umbrel + Tag ${TAG}"
WT="$(mktemp -d)"
gitz fetch origin umbrel 2>/dev/null || true
if git rev-parse -q --verify origin/umbrel >/dev/null; then
  git worktree add -q "${WT}" -B umbrel origin/umbrel
else
  git worktree add -q --detach "${WT}"
  (cd "${WT}" && git checkout -q --orphan umbrel && git rm -rqf .)
fi
cp "${OUT}"/* "${WT}/"
git show HEAD:scripts/umbrel-README.md > "${WT}/README.md"
(
  cd "${WT}"
  git add -A
  git commit -qm "nrzSearch ${VERSION}: Umbrel-Paket"
  git tag -a "${TAG}" -m "${NOTES}"
  gitz push -q origin umbrel
  gitz push -q origin "${TAG}"
)
git worktree remove --force "${WT}"
echo "==> ${TAG} veroeffentlicht. nrzGit aktualisiert den nrzStore."
# Erst jetzt main: aendert sich nrzStore.json/umbrel, legt nrzGit die App an - der Tag existiert schon.
gitz push -q origin HEAD:main

[ -n "${NUR_PAKET:-}" ] && exit 0

# Zugang fuer Registry und API aus dem Git-Schluesselbund (wird nicht ausgegeben).
PASS="$(printf 'protocol=http\nhost=%s\n\n' "${REGISTRY}" | gitz credential fill 2>/dev/null | sed -n 's/^password=//p')"
[ -n "${PASS}" ] || { echo "Kein Zugang fuer ${REGISTRY} im Git-Schluesselbund." >&2; exit 1; }

echo "==> Docker-Image ${IMAGE}:${VERSION} (amd64 + arm64)"
SX="$(curl -fsSL -H 'accept: application/vnd.github+json' https://api.github.com/repos/searxng/searxng/commits/master)"
SX_SHA="$(printf '%s' "${SX}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["sha"])')"
SX_DATUM="$(printf '%s' "${SX}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["commit"]["committer"]["date"][:10].replace("-","."))')"
# Eigene Docker-Konfiguration: Zugang fuer die HTTP-Registry ohne `docker login` (der Daemon
# verlangt dort HTTPS) und ein buildx-Builder, der die Registry per HTTP anspricht.
DCFG="$(mktemp -d)"
trap 'rm -rf "${OUT}" "${DCFG}"' EXIT
printf '{"auths":{"%s":{"auth":"%s"}},"cliPluginsExtraDirs":["/opt/homebrew/lib/docker/cli-plugins","%s/.docker/cli-plugins"]}' \
  "${REGISTRY}" "$(printf 'nicorenezweers:%s' "${PASS}" | base64)" "${NRZ_GIT_HOME:-$HOME}" > "${DCFG}/config.json"
printf '[registry."%s"]\n  http = true\n  insecure = true\n' "${REGISTRY}" > "${DCFG}/buildkitd.toml"
export DOCKER_HOST="${DOCKER_HOST:-$(HOME="${NRZ_GIT_HOME:-$HOME}" docker context inspect --format '{{.Endpoints.docker.Host}}' 2>/dev/null)}"
export DOCKER_CONFIG="${DCFG}"
docker buildx create --name nrzsearch-builder --driver docker-container --driver-opt network=host \
  --buildkitd-config "${DCFG}/buildkitd.toml" >/dev/null
docker buildx build --builder nrzsearch-builder --platform linux/amd64,linux/arm64 \
  --build-arg "SEARXNG_SHA=${SX_SHA}" --build-arg "SEARXNG_ID=${SX_DATUM}-${SX_SHA:0:9}" \
  --build-arg "VERSION=${VERSION}" \
  -t "${IMAGE}:${VERSION}" -t "${IMAGE}:latest" --push .
docker buildx rm nrzsearch-builder >/dev/null 2>&1 || true
unset DOCKER_CONFIG

echo "==> Release v${VERSION} auf nrzGit"
REL="$(curl -fsS -u "nicorenezweers:${PASS}" -H 'content-type: application/json' -X POST "${API}/releases" \
  -d "$(python3 -c 'import json,sys; print(json.dumps({"tagName": "v"+sys.argv[1], "target": "main", "title": "nrzSearch "+sys.argv[1], "isDraft": False, "body": sys.argv[2]+"\n\nDocker: "+sys.argv[3]+":"+sys.argv[1]}))' "${VERSION}" "${NOTES}" "${IMAGE}")")"
REL_ID="$(printf '%s' "${REL}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
for f in nrzsearch-amd64.gz nrzsearch-arm64.gz web.tar.gz SHA256SUMS; do
  curl -fsS -u "nicorenezweers:${PASS}" -F "file=@${OUT}/${f}" "${API}/releases/${REL_ID}/assets?name=${f}" >/dev/null
done
echo "==> fertig: ${TAG}, ${IMAGE}:${VERSION}, Release v${VERSION}"
