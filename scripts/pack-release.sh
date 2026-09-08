#!/usr/bin/env bash
set -euo pipefail

# scripts/pack-release.sh
# Packs target/release/darius into darius-<os>-<arch>.tar.gz with sha256 checksum.
# Outputs to target/release/

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RELEASE_DIR="$REPO_ROOT/target/release"
BIN_PATH="$RELEASE_DIR/darius"

if [ ! -f "$BIN_PATH" ]; then
    echo "Error: Release binary not found at $BIN_PATH" >&2
    echo "Run: cargo build --release -p darius-cli" >&2
    exit 1
fi

# Detect OS
RAW_OS=$(uname -s)
case "$RAW_OS" in
    Linux|linux)    OS="linux" ;;
    Darwin|darwin)  OS="macos" ;;
    *)
        echo "Error: Unsupported OS: $RAW_OS" >&2
        exit 1
        ;;
esac

# Detect ARCH
RAW_ARCH=$(uname -m)
case "$RAW_ARCH" in
    x86_64|amd64)   ARCH="x86_64" ;;
    arm64|aarch64)  ARCH="aarch64" ;;
    *)
        echo "Error: Unsupported architecture: $RAW_ARCH" >&2
        exit 1
        ;;
esac

ASSET_STEM="darius-${OS}-${ARCH}"
ASSET="${ASSET_STEM}.tar.gz"

echo "Packing $BIN_PATH -> $RELEASE_DIR/$ASSET"

# Create tarball
tar -czf "$RELEASE_DIR/$ASSET" -C "$RELEASE_DIR" darius

# Generate sha256 checksum
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$RELEASE_DIR/$ASSET" | awk '{print $1}' > "$RELEASE_DIR/${ASSET_STEM}.sha256"
elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$RELEASE_DIR/$ASSET" | awk '{print $1}' > "$RELEASE_DIR/${ASSET_STEM}.sha256"
else
    echo "Error: Neither sha256sum nor shasum available" >&2
    exit 1
fi

echo "✓ Created $RELEASE_DIR/$ASSET"
echo "✓ Created $RELEASE_DIR/${ASSET_STEM}.sha256"
echo ""
echo "Asset: $ASSET"
echo "Checksum: $(cat "$RELEASE_DIR/${ASSET_STEM}.sha256")"
