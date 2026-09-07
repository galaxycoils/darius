#!/usr/bin/env bash
# Darius installer
# Usage:
#   curl -sSL https://github.com/galaxycoils/darius/releases/latest/download/install.sh | bash
#   bash install.sh --artifact-dir ./dist --version 1.2.0 --install-dir "$TMPDIR/bin"

set -euo pipefail

REPO="galaxycoils/darius"
BIN_NAME="darius"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
ARTIFACT_DIR=""
VERSION=""

usage() {
    echo "Darius Installer"
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --artifact-dir <DIR>   Use local directory for release archives and checksums"
    echo "  --version <VERSION>    Target version (e.g. 1.2.0 or v1.2.0)"
    echo "  --install-dir <DIR>    Installation directory (default: \$INSTALL_DIR or ~/.local/bin)"
    echo "  -h, --help             Show this help message"
    exit 0
}

while [ $# -gt 0 ]; do
    case "$1" in
        --artifact-dir)
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --artifact-dir=*)
            ARTIFACT_DIR="${1#*=}"
            shift 1
            ;;
        --version)
            VERSION="$2"
            shift 2
            ;;
        --version=*)
            VERSION="${1#*=}"
            shift 1
            ;;
        --install-dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        --install-dir=*)
            INSTALL_DIR="${1#*=}"
            shift 1
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

echo "=== Darius Installer ==="

# Detect and normalize OS
RAW_OS=$(uname -s)
case "$RAW_OS" in
    Linux|linux)
        OS="linux"
        ;;
    Darwin|darwin)
        OS="macos"
        ;;
    *)
        echo "Error: Unsupported OS: $RAW_OS" >&2
        exit 1
        ;;
esac

# Detect and normalize ARCH
RAW_ARCH=$(uname -m)
case "$RAW_ARCH" in
    x86_64|amd64)
        ARCH="x86_64"
        ;;
    arm64|aarch64)
        ARCH="aarch64"
        ;;
    *)
        echo "Error: Unsupported architecture: $RAW_ARCH" >&2
        exit 1
        ;;
esac

ASSET_STEM="darius-${OS}-${ARCH}"
ASSET="${ASSET_STEM}.tar.gz"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

compute_sha256() {
    local target="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$target" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$target" | awk '{print $1}'
    else
        echo "Error: Neither sha256sum nor shasum is available for checksum verification." >&2
        exit 1
    fi
}

if [ -n "$ARTIFACT_DIR" ]; then
    echo "Using local artifact directory: $ARTIFACT_DIR"
    ARCHIVE_PATH="$ARTIFACT_DIR/$ASSET"
    if [ ! -f "$ARCHIVE_PATH" ]; then
        echo "Error: Local archive not found: $ARCHIVE_PATH" >&2
        exit 1
    fi

    # Find checksum file
    CHECKSUM_PATH=""
    if [ -f "$ARTIFACT_DIR/${ASSET}.sha256" ]; then
        CHECKSUM_PATH="$ARTIFACT_DIR/${ASSET}.sha256"
    elif [ -f "$ARTIFACT_DIR/${ASSET_STEM}.sha256" ]; then
        CHECKSUM_PATH="$ARTIFACT_DIR/${ASSET_STEM}.sha256"
    else
        echo "Error: Checksum file not found in $ARTIFACT_DIR for $ASSET" >&2
        exit 1
    fi

    cp "$ARCHIVE_PATH" "$TMPDIR/$ASSET"
    cp "$CHECKSUM_PATH" "$TMPDIR/$ASSET.sha256"
else
    # Remote GitHub download
    if [ -z "$VERSION" ]; then
        echo "Fetching latest release version..."
        VERSION=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | head -n 1 | sed -E 's/.*"([^"]+)".*/\1/')
        if [ -z "$VERSION" ]; then
            echo "Error: Could not determine latest release version from GitHub." >&2
            exit 1
        fi
    fi

    TAG="$VERSION"
    if [[ "$TAG" != v* ]]; then
        TAG="v$TAG"
    fi

    echo "Release tag: $TAG"
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"
    CHECKSUM_URL="https://github.com/$REPO/releases/download/$TAG/${ASSET_STEM}.sha256"

    echo "Downloading $ASSET..."
    if ! curl -sSfL "$DOWNLOAD_URL" -o "$TMPDIR/$ASSET"; then
        echo "Error: Failed to download $DOWNLOAD_URL" >&2
        exit 1
    fi

    echo "Downloading checksum..."
    if ! curl -sSfL "$CHECKSUM_URL" -o "$TMPDIR/$ASSET.sha256"; then
        # Try alternate checksum url with .tar.gz.sha256
        if ! curl -sSfL "${DOWNLOAD_URL}.sha256" -o "$TMPDIR/$ASSET.sha256"; then
            echo "Error: Failed to download checksum file." >&2
            exit 1
        fi
    fi
fi

echo "Verifying checksum..."
EXPECTED_HASH=$(awk '{print $1}' "$TMPDIR/$ASSET.sha256" | tr -d ' \r\n')
ACTUAL_HASH=$(compute_sha256 "$TMPDIR/$ASSET")

if [ -z "$EXPECTED_HASH" ]; then
    echo "Error: Expected checksum is empty." >&2
    exit 1
fi

if [ "$EXPECTED_HASH" != "$ACTUAL_HASH" ]; then
    echo "Error: Checksum mismatch for $ASSET!" >&2
    echo "  Expected: $EXPECTED_HASH" >&2
    echo "  Actual:   $ACTUAL_HASH" >&2
    exit 1
fi
echo "✓ Checksum verified ($ACTUAL_HASH)"

echo "Extracting..."
tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

EXTRACTED_BIN="$TMPDIR/$BIN_NAME"
if [ ! -f "$EXTRACTED_BIN" ]; then
    echo "Error: Binary '$BIN_NAME' not found in archive." >&2
    exit 1
fi
chmod +x "$EXTRACTED_BIN"

echo "Validating binary..."
BIN_VERSION_OUTPUT=$("$EXTRACTED_BIN" --version 2>&1) || {
    echo "Error: Extracted binary failed to run --version: $BIN_VERSION_OUTPUT" >&2
    exit 1
}

if [ -n "$VERSION" ]; then
    CLEAN_VERSION="${VERSION#v}"
    if [[ "$BIN_VERSION_OUTPUT" != *"$CLEAN_VERSION"* ]]; then
        echo "Error: Binary version mismatch! Expected version $CLEAN_VERSION, got: $BIN_VERSION_OUTPUT" >&2
        exit 1
    fi
fi
echo "✓ Binary validated: $BIN_VERSION_OUTPUT"

echo "Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"

STAGE_TARGET="$INSTALL_DIR/.${BIN_NAME}.tmp.$$"
cp "$EXTRACTED_BIN" "$STAGE_TARGET"
chmod +x "$STAGE_TARGET"
mv -f "$STAGE_TARGET" "$INSTALL_DIR/$BIN_NAME"

echo ""
echo "✓ Darius installed to $INSTALL_DIR/$BIN_NAME"
echo ""

case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        ;;
    *)
        echo "Make sure $INSTALL_DIR is in your PATH:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
        echo ""
        ;;
esac

echo "Quickstart:"
echo "  darius tui"
echo "  darius --help"
echo "  darius run \"your goal here\""
