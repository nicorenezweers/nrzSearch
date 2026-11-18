# syntax=docker/dockerfile:1.7
# nrzSearch - Release-Image (amd64 + arm64).
#
# Server (Rust) und Oberflaeche (Rust/WASM) werden vorher gebaut - das macht
# scripts/umbrel-release.sh (dist/linux-<arch>/nrzsearch und public/). Das Image bringt eine
# SearXNG-Fassung schon mit (sofort suchbereit); nrzSearch holt danach selbst taeglich die
# neueste Fassung nach /data/searxng - ohne externe Instanz.
#
# Bauen:   scripts/umbrel-release.sh <version>      (baut, pusht Image und Release)
# Starten: docker run -d -p 8188:8188 -v nrzsearch-data:/data --name nrzsearch \
#            100.78.107.162:8390/nicorenezweers/nrzsearch:latest

FROM python:3.14-slim-trixie@sha256:9aaa474097d094040649f2162a627c0049b408f52138d980916788d80f04802f

ARG TARGETARCH
ARG SEARXNG_SHA=master
ARG SEARXNG_ID=vorab
ARG VERSION=dev

LABEL org.opencontainers.image.title="nrzSearch" \
      org.opencontainers.image.description="Eigene Suchmaschine in Rust mit eingebautem SearXNG" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.source="http://100.78.107.162:8390/nicorenezweers/nrzSearch" \
      org.opencontainers.image.licenses="MIT"

# Die mitgelieferte SearXNG-Fassung (gleicher Aufbau wie unter /data/searxng/versionen/<id>).
ADD https://codeload.github.com/searxng/searxng/tar.gz/${SEARXNG_SHA} /tmp/searxng.tar.gz
RUN set -eux; \
    mkdir -p /opt/searxng/src; \
    tar -xzf /tmp/searxng.tar.gz -C /opt/searxng/src --strip-components=1; \
    rm /tmp/searxng.tar.gz; \
    python3 -m venv /opt/searxng/venv; \
    /opt/searxng/venv/bin/pip install --no-cache-dir --disable-pip-version-check -q \
      -r /opt/searxng/src/requirements.txt -r /opt/searxng/src/requirements-server.txt; \
    ver="$(echo "${SEARXNG_ID}" | sed 's/-/+/')"; \
    printf 'VERSION_STRING = "%s"\nVERSION_TAG = "%s"\nDOCKER_TAG = "%s"\nGIT_URL = "https://github.com/searxng/searxng"\nGIT_BRANCH = "master"\n' \
      "$ver" "$ver" "${SEARXNG_ID}" > /opt/searxng/src/searx/version_frozen.py; \
    python3 -m compileall -q /opt/searxng/src/searx >/dev/null || true; \
    echo "${SEARXNG_ID}" > /opt/searxng/version; \
    touch /opt/searxng/.fertig; \
    useradd -u 1000 -m -d /home/nrz nrz; \
    mkdir -p /data /app; chown 1000:1000 /data

COPY --chmod=755 dist/linux-${TARGETARCH}/nrzsearch /app/nrzsearch
COPY public /app/web
COPY module.json /app/module.json

ENV TZ=Europe/Berlin \
    PORT=8188 \
    HOME=/data/tmp \
    TMPDIR=/data/tmp \
    WEB_DIR=/app/web \
    MODULE_CONFIG=/data/module.json \
    MODULE_STANDARD=/app/module.json \
    SEARXNG_DIR=/data/searxng \
    SEARXNG_VORAB=/opt/searxng \
    OPENROUTER_KEY_FILE=/data/openrouter.key \
    ABLAGE_DIR=/data/ablagen \
    STATISTIK_DATEI=/data/statistik.json \
    ADMIN_DATEI=/data/admin.json

USER 1000
WORKDIR /data
VOLUME /data
EXPOSE 8188

HEALTHCHECK --interval=30s --timeout=6s --start-period=60s --retries=3 \
  CMD python3 -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8188/api/statistik', timeout=4)"

CMD ["/bin/sh", "-c", "mkdir -p /data/tmp /data/searxng && exec /app/nrzsearch"]
