#!/usr/bin/env bash
set -euo pipefail

# Exact-host release binary gate.
# Runs the release binary (not cargo-built dev binary) through:
#   1. PTY first-run setup journey (clean bare launch, setup, quit)
#   2. PTY full agent journey (real text/tools/deny/approve/plan/cancel/recovery)
#   3. Bare non-TTY help (no-arg --help exits cleanly without a TTY)
# The binary under test MUST be set via DARIUS_BIN_UNDER_TEST so the gate
# audits the exact release artifact, not a development build.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -z "${DARIUS_BIN_UNDER_TEST:-}" ]; then
    echo "DARIUS_BIN_UNDER_TEST must point at the release binary" >&2
    exit 1
fi
if [ ! -x "$DARIUS_BIN_UNDER_TEST" ]; then
    echo "Release binary not executable: $DARIUS_BIN_UNDER_TEST" >&2
    exit 1
fi

export DARIUS_BIN_UNDER_TEST
DARIUS_BIN_UNDER_TEST="$(cd "$(dirname "$DARIUS_BIN_UNDER_TEST")" && pwd)/$(basename "$DARIUS_BIN_UNDER_TEST")"
cd "$REPO_ROOT"
echo "=== Release binary: $DARIUS_BIN_UNDER_TEST ==="
"$DARIUS_BIN_UNDER_TEST" --version

echo "=== Bare non-TTY help ==="
"$DARIUS_BIN_UNDER_TEST" < /dev/null
echo "✓ Non-TTY help exits cleanly"

run_journey() {
    local name="$1"
    cargo test -p darius-cli --test tui_pty "$name" -- --list |
        grep -Fx "$name: test" >/dev/null
    cargo test -p darius-cli --test tui_pty "$name" -- --exact --nocapture --test-threads=1
}

echo "=== PTY first-run setup journey ==="
run_journey first_run_setup_journey
echo "✓ first_run_setup_journey passed"

echo "=== PTY full agent journey ==="
run_journey full_agent_journey
echo "✓ full_agent_journey passed"

echo "=== Release host binary gate PASSED ==="
