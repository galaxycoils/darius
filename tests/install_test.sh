#!/usr/bin/env bash
set -euo pipefail

# tests/install_test.sh
# Verification test suite for Darius installer and release asset alignment.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_TARGET_SCRIPT="$REPO_ROOT/scripts/release-target.sh"
INSTALL_SCRIPT="$REPO_ROOT/install.sh"

compute_sha256() {
    local target="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$target" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$target" | awk '{print $1}'
    else
        echo "Error: Neither sha256sum nor shasum available" >&2
        exit 1
    fi
}

test_release_matrix() {
    echo "--- Testing release matrix mappings ---"
    test "$("$RELEASE_TARGET_SCRIPT" linux x86_64)" = "darius-linux-x86_64.tar.gz"
    test "$("$RELEASE_TARGET_SCRIPT" linux amd64)" = "darius-linux-x86_64.tar.gz"
    test "$("$RELEASE_TARGET_SCRIPT" macos aarch64)" = "darius-macos-aarch64.tar.gz"
    test "$("$RELEASE_TARGET_SCRIPT" darwin arm64)" = "darius-macos-aarch64.tar.gz"
    test "$("$RELEASE_TARGET_SCRIPT" macos x86_64)" = "darius-macos-x86_64.tar.gz"
    test "$("$RELEASE_TARGET_SCRIPT" darwin x86_64)" = "darius-macos-x86_64.tar.gz"
    test "$("$RELEASE_TARGET_SCRIPT" linux x86_64 --stem)" = "darius-linux-x86_64"
    test "$("$RELEASE_TARGET_SCRIPT" macos aarch64 --stem)" = "darius-macos-aarch64"
    echo "✓ Release matrix mappings verified"
}

test_local_installer() {
    echo "--- Testing local installer lifecycle ---"
    local WORKDIR
    WORKDIR=$(mktemp -d)
    trap 'rm -rf "$WORKDIR"' EXIT

    local STAGE_DIR="$WORKDIR/dist"
    local INSTALL_DIR="$WORKDIR/bin"
    local BIN_DIR="$WORKDIR/src_bin"
    mkdir -p "$STAGE_DIR" "$INSTALL_DIR" "$BIN_DIR"

    # Detect current platform asset name
    local ASSET
    ASSET=$("$RELEASE_TARGET_SCRIPT")
    local ASSET_STEM
    ASSET_STEM=$("$RELEASE_TARGET_SCRIPT" --stem)

    # 1. Create a dummy initial binary in INSTALL_DIR (v1.0.0)
    cat << 'MOCK_OLD' > "$INSTALL_DIR/darius"
#!/usr/bin/env bash
if [ "$1" = "--version" ]; then
    echo "darius 1.0.0"
    exit 0
fi
echo "old darius binary"
MOCK_OLD
    chmod +x "$INSTALL_DIR/darius"

    # 2. Create mock release binary (v1.2.0)
    cat << 'MOCK_NEW' > "$BIN_DIR/darius"
#!/usr/bin/env bash
if [ "$1" = "--version" ]; then
    echo "darius 1.2.0"
    exit 0
fi
echo "new darius binary"
MOCK_NEW
    chmod +x "$BIN_DIR/darius"

    # Package into tar.gz
    tar -czf "$STAGE_DIR/$ASSET" -C "$BIN_DIR" darius

    # Generate sha256 file
    local HASH
    HASH=$(compute_sha256 "$STAGE_DIR/$ASSET")
    echo "$HASH  $ASSET" > "$STAGE_DIR/$ASSET_STEM.sha256"

    # 3. Test successful installation
    echo "Testing successful install..."
    local INSTALL_OUT
    INSTALL_OUT=$(bash "$INSTALL_SCRIPT" --artifact-dir "$STAGE_DIR" --version 1.2.0 --install-dir "$INSTALL_DIR")
    test -x "$INSTALL_DIR/darius"
    local VER_OUT
    VER_OUT=$("$INSTALL_DIR/darius" --version)
    test "$VER_OUT" = "darius 1.2.0"
    echo "$INSTALL_OUT" | grep -q "Make sure $INSTALL_DIR is in your PATH"
    echo "✓ Successful install and PATH guidance verified"

    # 4. Test checksum mismatch failure and atomic preservation
    echo "Testing checksum mismatch failure..."
    echo "corrupted content" > "$STAGE_DIR/$ASSET"
    set +e
    local ERR_OUT
    ERR_OUT=$(bash "$INSTALL_SCRIPT" --artifact-dir "$STAGE_DIR" --version 1.2.0 --install-dir "$INSTALL_DIR" 2>&1)
    local ERR_CODE=$?
    set -euo pipefail
    test "$ERR_CODE" -ne 0
    echo "$ERR_OUT" | grep -q "Checksum mismatch"
    # Verify that existing binary was NOT corrupted
    test "$("$INSTALL_DIR/darius" --version)" = "darius 1.2.0"
    echo "✓ Checksum mismatch failed cleanly and preserved existing binary"

    # 5. Test missing asset failure
    echo "Testing missing asset failure..."
    local EMPTY_DIR="$WORKDIR/empty"
    mkdir -p "$EMPTY_DIR"
    set +e
    local MISSING_OUT
    MISSING_OUT=$(bash "$INSTALL_SCRIPT" --artifact-dir "$EMPTY_DIR" --version 1.2.0 --install-dir "$INSTALL_DIR" 2>&1)
    local MISSING_CODE=$?
    set -euo pipefail
    test "$MISSING_CODE" -ne 0
    echo "$MISSING_OUT" | grep -q "Local archive not found"
    echo "✓ Missing asset error verified"

    # 6. Test version mismatch failure
    echo "Testing version mismatch failure..."
    # Restore valid tarball
    tar -czf "$STAGE_DIR/$ASSET" -C "$BIN_DIR" darius
    HASH=$(compute_sha256 "$STAGE_DIR/$ASSET")
    echo "$HASH  $ASSET" > "$STAGE_DIR/$ASSET_STEM.sha256"
    set +e
    local VER_MISMATCH_OUT
    VER_MISMATCH_OUT=$(bash "$INSTALL_SCRIPT" --artifact-dir "$STAGE_DIR" --version 9.9.9 --install-dir "$INSTALL_DIR" 2>&1)
    local VER_MISMATCH_CODE=$?
    set -euo pipefail
    test "$VER_MISMATCH_CODE" -ne 0
    echo "$VER_MISMATCH_OUT" | grep -q "Binary version mismatch"
    echo "✓ Binary version mismatch rejected cleanly"

    # Clean up trap
    rm -rf "$WORKDIR"
    trap - EXIT
}

MODE="${1:-all}"
case "$MODE" in
    release_matrix)
        test_release_matrix
        ;;
    all)
        test_release_matrix
        test_local_installer
        ;;
    *)
        echo "Unknown mode: $MODE" >&2
        exit 1
        ;;
esac

echo "✓ All install tests passed!"
