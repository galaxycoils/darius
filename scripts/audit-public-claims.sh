#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Never trust a stale target/debug binary. No install or PATH mutation.
cargo build -p darius-cli
python3 -B - <<'PY'
from pathlib import Path
import sys
sys.path.insert(0, 'scripts')
from claims_sources import audit as sources
from claims_runtime import audit as runtime
root = Path.cwd()
errors = sources(root) + runtime(root, str(root / 'target/debug/darius'))
if errors:
    print('\n'.join(errors), file=sys.stderr)
    raise SystemExit(1)
PY
python3 -B -m unittest discover -s tests -p '*claims_test.py'
cargo test -p darius-core --test public_claims
cargo test -p darius-web --test public_claims
cargo test -p darius-cli --test cli_contract --test public_claims
printf '%s\n' 'Public claims audit PASS (local contract evidence; not release or live-service proof)'
