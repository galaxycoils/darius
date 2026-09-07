#!/usr/bin/env bash
set -euo pipefail

# scripts/audit-public-claims.sh
# Audits machine-visible claims, CLI help, slash commands, manifests, docs,
# and installer for truthfulness and absence of retired/unsupported claims.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Starting Darius Public Claims Audit ==="

# Build cli binary if not already built
if [ ! -f target/debug/darius ]; then
    cargo build -p darius-cli
fi
DARIUS_BIN="target/debug/darius"

# 1. Audit CLI Help
echo "1. Auditing CLI help outputs..."
CLI_HELP=$("$DARIUS_BIN" --help)
RUN_HELP=$("$DARIUS_BIN" run --help)
CONFIG_HELP=$("$DARIUS_BIN" config --help)
MEMORY_HELP=$("$DARIUS_BIN" memory --help)

# Verify canonical subcommands exist
for cmd in tui run config memory; do
    if ! echo "$CLI_HELP" | grep -q "  $cmd"; then
        echo "Error: CLI help missing canonical subcommand '$cmd'" >&2
        exit 1
    fi
done

# Verify removed commands do NOT appear in CLI help
REMOVED_CLI_TOKENS=("session-smoke" "serve" "a2a" "cron" "approval-check" "daemon" "start" "attach" "eval" "learn")
for token in "${REMOVED_CLI_TOKENS[@]}"; do
    if echo "$CLI_HELP" | grep -qw "$token"; then
        echo "Error: CLI help exposes removed command '$token'" >&2
        exit 1
    fi
done

# Verify config subcommands: show, init
for cmd in show init; do
    if ! echo "$CONFIG_HELP" | grep -q "  $cmd"; then
        echo "Error: config help missing subcommand '$cmd'" >&2
        exit 1
    fi
done

# Verify memory subcommands: search, pack, import, export, stats
for cmd in search pack import export stats; do
    if ! echo "$MEMORY_HELP" | grep -q "  $cmd"; then
        echo "Error: memory help missing subcommand '$cmd'" >&2
        exit 1
    fi
done
echo "✓ CLI help truthful and free of removed commands"

# 2. Audit Slash Commands Registry (closed-world 13 commands)
echo "2. Auditing slash commands registry..."
CANONICAL_SLASH=(
    "/help"
    "/clear"
    "/compact"
    "/model"
    "/mode"
    "/permissions"
    "/memory"
    "/pack"
    "/tasks"
    "/status"
    "/config"
    "/stop"
    "/quit"
)
REGISTRY_FILE="crates/darius-core/src/commands.rs"
REGISTERED_SLASH=($(grep -oE 'name: "/[^"]+"' "$REGISTRY_FILE" | sed -E 's/name: "([^"]+)"/\1/' | sort -u))

if [ "${#REGISTERED_SLASH[@]}" -ne 13 ]; then
    echo "Error: Expected exactly 13 slash commands in $REGISTRY_FILE, found ${#REGISTERED_SLASH[@]}: ${REGISTERED_SLASH[*]}" >&2
    exit 1
fi

for cmd in "${CANONICAL_SLASH[@]}"; do
    if ! grep -q "name: \"$cmd\"" "$REGISTRY_FILE"; then
        echo "Error: Missing canonical slash command $cmd in $REGISTRY_FILE" >&2
        exit 1
    fi
done

# Ensure retired slash commands are not in registry
RETIRED_SLASH=("/plan" "/skills" "/effort" "/a2a" "/serve")
for cmd in "${RETIRED_SLASH[@]}"; do
    if grep -q "name: \"$cmd\"" "$REGISTRY_FILE"; then
        echo "Error: Retired slash command $cmd found in $REGISTRY_FILE" >&2
        exit 1
    fi
done
echo "✓ Slash registry matches exact closed-world 13 commands"

# 3. Audit Package Manifest Descriptions
echo "3. Auditing Cargo.toml descriptions..."
while IFS= read -r toml; do
    DESC=$(grep -E '^\s*description\s*=' "$toml" || true)
    if [ -n "$DESC" ]; then
        if echo "$DESC" | grep -qi "most powerful"; then
            echo "Error: Superlative found in $toml: $DESC" >&2
            exit 1
        fi
        if echo "$DESC" | grep -qi "A2A hub"; then
            echo "Error: 'A2A hub' claim found in $toml: $DESC" >&2
            exit 1
        fi
        if echo "$DESC" | grep -qi "eval, learn"; then
            echo "Error: Removed commands found in $toml: $DESC" >&2
            exit 1
        fi
    fi
done < <(find . \( -name .git -o -name target \) -prune -o -name "Cargo.toml" -print)
echo "✓ Manifest descriptions clean and free of overclaims"

# 4. Audit Documentation
echo "4. Auditing documentation and operational truth..."

# Check README.md
if grep -q "darius session-smoke" README.md; then
    echo "Error: README.md references removed 'darius session-smoke'" >&2
    exit 1
fi

# Check TROUBLESHOOTING.md exists
if [ ! -f docs/TROUBLESHOOTING.md ]; then
    echo "Error: docs/TROUBLESHOOTING.md does not exist" >&2
    exit 1
fi

# Check CAPABILITIES.md drift guard
python3 - <<'PY'
from pathlib import Path
import re

root = Path('.')
doc = (root / 'docs/CAPABILITIES.md').read_text()
checks = {
    'recovery CLI': (root / 'crates/darius-cli/src/lib.rs').read_text(),
    'slash registry': (root / 'crates/darius-tui/src/commands.rs').read_text(),
    'tool registrations': (root / 'crates/darius-tools/src/lib.rs').read_text(),
    'web routes': (root / 'crates/darius-web/src/lib.rs').read_text(),
}
needles = set(re.findall(r'"(tui|run|config|memory|daemon|status|start|stop|attach|eval|learn|session-smoke|serve|a2a|cron|approval-check|help)"', checks['recovery CLI']))
needles |= set(re.findall(r'name: "(/[^" ]+)"', checks['slash registry']))
needles |= set(re.findall(r'register_with_risk\("([a-z_]+)"', checks['tool registrations']))
needles |= set(re.findall(r'\.route\("([^"{]+(?:\{id\}|\{handle\})?)"', checks['web routes']))
needles |= {'darwin-x86_64', 'darwin-aarch64', 'linux-x86_64', 'linux-aarch64'}
missing = sorted(item for item in needles if item not in doc)
assert not missing, f'CAPABILITIES.md missing inventory entries: {missing}'
PY
echo "✓ Documentation and capability inventory verified"

# 5. Audit Installer
echo "5. Auditing installer..."
if grep -q "session-smoke" install.sh; then
    echo "Error: install.sh contains legacy session-smoke reference" >&2
    exit 1
fi
if ! grep -q -- "--artifact-dir" install.sh; then
    echo "Error: install.sh missing --artifact-dir support" >&2
    exit 1
fi
if ! grep -q -- "compute_sha256" install.sh; then
    echo "Error: install.sh missing checksum verification" >&2
    exit 1
fi
echo "✓ Installer script verified"

# 6. Audit Web Agent Card & Dashboard
echo "6. Auditing agent card and web dashboard..."
if grep -q '"peer_a2a"' crates/darius-web/src/lib.rs; then
    echo "Error: crates/darius-web/src/lib.rs agent_card contains peer_a2a capability" >&2
    exit 1
fi
echo "✓ Agent card clean"

# 7. Audit Verified Capability Tests
echo "7. Auditing verified test coverage..."
test -f crates/darius-cli/tests/cli_contract.rs
test -f crates/darius-cli/tests/tui_pty.rs
test -f crates/darius-cli/tests/run_e2e.rs
test -f tests/install_test.sh
echo "✓ Verified capability test files present"

echo "=== Public Claims Audit PASS ==="
