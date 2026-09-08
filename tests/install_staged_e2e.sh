#!/usr/bin/env bash
set -euo pipefail

# tests/install_staged_e2e.sh
# E2E test: install.sh with staged tarball, verify darius --version matches Cargo version.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_SCRIPT="$REPO_ROOT/install.sh"
RELEASE_DIR="$REPO_ROOT/target/release"

# Read expected version from Cargo.toml workspace
CARGO_VERSION=$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | sed -E 's/version *= *"([^"]+)".*/\1/')
EXPECTED_VERSION="darius $CARGO_VERSION"

# Detect current platform
RAW_OS=$(uname -s)
case "$RAW_OS" in
    Linux|linux)    OS="linux" ;;
    Darwin|darwin)  OS="macos" ;;
    *)
        echo "SKIP: Unsupported OS: $RAW_OS"
        exit 0
        ;;
esac

RAW_ARCH=$(uname -m)
case "$RAW_ARCH" in
    x86_64|amd64)   ARCH="x86_64" ;;
    arm64|aarch64)  ARCH="aarch64" ;;
    *)
        echo "SKIP: Unsupported architecture: $RAW_ARCH"
        exit 0
        ;;
esac

ASSET_STEM="darius-${OS}-${ARCH}"
ASSET="${ASSET_STEM}.tar.gz"
TARBALL="$RELEASE_DIR/$ASSET"
CHECKSUM="$RELEASE_DIR/${ASSET_STEM}.sha256"

echo "=== Install staged tarball E2E test ==="
echo "Platform: $OS-$ARCH"
echo "Expected version: $EXPECTED_VERSION"
echo ""

# 1. Ensure release binary exists
if [ ! -f "$RELEASE_DIR/darius" ]; then
    echo "Error: Release binary not found at $RELEASE_DIR/darius" >&2
    echo "Run: cargo build --release -p darius-cli" >&2
    exit 1
fi

# 2. Ensure tarball exists (create via pack-release.sh if needed)
if [ ! -f "$TARBALL" ] || [ ! -f "$CHECKSUM" ]; then
    echo "Tarball or checksum missing. Creating..."
    bash "$REPO_ROOT/scripts/pack-release.sh"
fi

echo "Tarball: $TARBALL"
echo "Checksum: $(cat "$CHECKSUM")"
echo ""

# 3. Create temp install prefix
WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

STAGE_DIR="$WORKDIR/dist"
INSTALL_DIR="$WORKDIR/bin"
mkdir -p "$STAGE_DIR" "$INSTALL_DIR"

# Copy staged assets to temp dir
cp "$TARBALL" "$STAGE_DIR/"
cp "$CHECKSUM" "$STAGE_DIR/${ASSET_STEM}.sha256"

# 4. Run installer pointing at local artifact directory
echo "Running install.sh with --artifact-dir..."
INSTALL_OUT=$(bash "$INSTALL_SCRIPT" \
    --artifact-dir "$STAGE_DIR" \
    --version "$CARGO_VERSION" \
    --install-dir "$INSTALL_DIR" 2>&1) || {
    echo "Error: install.sh failed" >&2
    echo "$INSTALL_OUT" >&2
    exit 1
}
echo "$INSTALL_OUT"

# 5. Verify installed binary
echo ""
echo "Verifying installed binary..."
if [ ! -x "$INSTALL_DIR/darius" ]; then
    echo "Error: Binary not found or not executable at $INSTALL_DIR/darius" >&2
    exit 1
fi

INSTALLED_VERSION=$("$INSTALL_DIR/darius" --version 2>&1)
echo "Installed version output: $INSTALLED_VERSION"

if [ "$INSTALLED_VERSION" != "$EXPECTED_VERSION" ]; then
    echo "Error: Version mismatch!" >&2
    echo "  Expected: $EXPECTED_VERSION" >&2
    echo "  Got:      $INSTALLED_VERSION" >&2
    exit 1
fi

echo ""
echo "✓ Install staged tarball E2E test PASSED"
echo "  - install.sh ran successfully with --artifact-dir"
echo "  - Binary installed to temp prefix"
echo "  - $INSTALLED_VERSION matches Cargo.toml version"
