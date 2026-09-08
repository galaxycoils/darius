#!/usr/bin/env bash
set -euo pipefail

# Mandatory pinned actionlint v1.7.12 gate.
# Installs the exact version locally (never a floating/latest tag) and lints
# every workflow. Fails closed on any mismatch so CI and release cannot drift.

REQUIRED_VERSION="1.7.12"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKFLOWS="$REPO_ROOT/.github/workflows"
INSTALL_DIR="${DARIUS_ACTIONLINT_DIR:-/tmp/darius-tools}"
TARGET="$INSTALL_DIR/actionlint"

log() { printf '[actionlint] %s\n' "$*"; }

version_matches() {
    local out
    out="$("$TARGET" -version 2>/dev/null | head -n 1 || true)"
    [ "$out" = "$REQUIRED_VERSION" ]
}

install_exact() {
    log "Installing actionlint v${REQUIRED_VERSION} to $INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"
    local tmp
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    local os arch archive
    case "$(uname -s)" in
        Linux) os="linux" ;;
        Darwin) os="darwin" ;;
        *) log "Unsupported OS for actionlint: $(uname -s)" >&2; exit 1 ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64) arch="amd64" ;;
        arm64|aarch64) arch="arm64" ;;
        *) log "Unsupported arch for actionlint: $(uname -m)" >&2; exit 1 ;;
    esac
    archive="actionlint_${REQUIRED_VERSION}_${os}_${arch}.tar.gz"
    if ! curl -sSfL "https://github.com/rhysd/actionlint/releases/download/v${REQUIRED_VERSION}/${archive}" -o "$tmp/$archive"; then
        log "Download failed for v${REQUIRED_VERSION}" >&2
        exit 1
    fi
    tar -xzf "$tmp/$archive" -C "$tmp" actionlint
    install -m 755 "$tmp/actionlint" "$TARGET"
    rm -rf "$tmp"
    trap - EXIT
}

if [ ! -x "$TARGET" ]; then
    install_exact
elif ! version_matches; then
    log "Installed version mismatched; reinstalling v${REQUIRED_VERSION}"
    rm -f "$TARGET"
    install_exact
fi

if ! version_matches; then
    log "actionlint version is not exactly v${REQUIRED_VERSION}" >&2
    exit 1
fi

log "Linting $WORKFLOWS/*.yml with actionlint v${REQUIRED_VERSION}"
"$TARGET" "$WORKFLOWS"/*.yml
log "All workflows pass actionlint v${REQUIRED_VERSION}"
