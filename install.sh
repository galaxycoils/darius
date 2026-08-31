#!/bin/bash
# Darius v1.0.0 installer
# Usage: curl -sSL https://github.com/galaxycoils/darius/releases/latest/download/install.sh | bash

set -euo pipefail

REPO="galaxycoils/darius"
BIN_NAME="darius"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

echo "=== Darius Installer ==="

# Detect OS and arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

# Find latest release version
echo "Fetching latest release..."
LATEST=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$LATEST" ]; then
    echo "Could not determine latest release. Falling back to building from source."
    echo "Run: cargo install --git https://github.com/$REPO darius-cli"
    exit 1
fi

echo "Latest release: $LATEST"

ASSET="${BIN_NAME}-${OS}-${ARCH}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST/$ASSET"

echo "Downloading $ASSET..."
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

curl -sSL "$DOWNLOAD_URL" -o "$TMPDIR/$ASSET"

echo "Extracting..."
tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

echo "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"
cp "$TMPDIR/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

echo ""
echo "✓ Darius installed to $INSTALL_DIR/$BIN_NAME"
echo ""
echo "Make sure $INSTALL_DIR is in your PATH:"
echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
echo ""
echo "Quickstart:"
echo "  darius session-smoke"
echo "  darius run \"your goal here\""
echo "  darius memory stats"
