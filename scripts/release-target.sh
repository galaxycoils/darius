#!/usr/bin/env bash
set -euo pipefail

# scripts/release-target.sh
# Maps OS and architecture to canonical release tarball / asset names.
# Canonical names:
#   - darius-linux-x86_64.tar.gz
#   - darius-macos-aarch64.tar.gz
#   - darius-macos-x86_64.tar.gz

STEM_ONLY=0
OS_ARG=""
ARCH_ARG=""

for arg in "$@"; do
    case "$arg" in
        --stem)
            STEM_ONLY=1
            ;;
        --help|-h)
            echo "Usage: $0 [--stem] [OS] [ARCH]"
            echo "Outputs canonical asset name for Darius release."
            exit 0
            ;;
        *)
            if [ -z "$OS_ARG" ]; then
                OS_ARG="$arg"
            elif [ -z "$ARCH_ARG" ]; then
                ARCH_ARG="$arg"
            fi
            ;;
    esac
done

OS="${OS_ARG:-$(uname -s)}"
ARCH="${ARCH_ARG:-$(uname -m)}"

# Normalize OS
case "$OS" in
    Linux|linux)
        CANONICAL_OS="linux"
        ;;
    Darwin|darwin|mac|macos|macOS)
        CANONICAL_OS="macos"
        ;;
    *)
        echo "Unsupported OS: $OS" >&2
        exit 1
        ;;
esac

# Normalize ARCH
case "$ARCH" in
    x86_64|amd64)
        CANONICAL_ARCH="x86_64"
        ;;
    arm64|aarch64)
        CANONICAL_ARCH="aarch64"
        ;;
    *)
        echo "Unsupported architecture: $ARCH" >&2
        exit 1
        ;;
esac

ASSET_STEM="darius-${CANONICAL_OS}-${CANONICAL_ARCH}"

if [ "$STEM_ONLY" -eq 1 ]; then
    echo "$ASSET_STEM"
else
    echo "${ASSET_STEM}.tar.gz"
fi
