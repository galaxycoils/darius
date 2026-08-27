#!/bin/bash
# Darius installer
# Usage: curl -sSL https://install.darius.ai | bash
#   or: ./install.sh [--prefix PATH] [--features FEATURES]

set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
FEATURES="${FEATURES:-}"
BIN_DIR="$PREFIX/bin"

echo "=== Darius Installer ==="

# Check prerequisites
check_prereq() {
    if ! command -v "$1" &>/dev/null; then
        echo "ERROR: $1 is required but not installed."
        exit 1
    fi
}

check_prereq cargo
check_prereq rustc

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            PREFIX="$2"
            BIN_DIR="$PREFIX/bin"
            shift 2
            ;;
        --features)
            FEATURES="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [--prefix PATH] [--features FEATURES]"
            echo ""
            echo "Options:"
            echo "  --prefix PATH    Installation prefix (default: /usr/local)"
            echo "  --features FEAT  Cargo features to enable (e.g., rlm-python)"
            echo "  --help, -h       Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "Building Darius..."
if [[ -n "$FEATURES" ]]; then
    cargo build --release --workspace --features "$FEATURES"
else
    cargo build --release --workspace
fi

echo "Installing to $BIN_DIR..."
mkdir -p "$BIN_DIR"

# Install main binary
if [[ -f target/release/darius ]]; then
    cp target/release/darius "$BIN_DIR/"
    chmod +x "$BIN_DIR/darius"
    echo "Installed: $BIN_DIR/darius"
fi

# Verify installation
if command -v darius &>/dev/null; then
    echo ""
    echo "Darius installed successfully!"
    darius --version 2>/dev/null || echo "darius (version unknown)"
else
    echo ""
    echo "Darius installed to $BIN_DIR/"
    echo "Add to PATH: export PATH=\"$BIN_DIR:\$PATH\""
fi

echo ""
echo "Next steps:"
echo "  darius daemon    # Start the daemon"
echo "  darius status    # Check status"
echo "  darius start     # Start a session"
