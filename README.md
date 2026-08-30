# Darius

Darius is an open-source Rust agent harness. The active implementation and default branch is `master`; the historical `main` branch contains the obsolete scaffold.

## Phase 2 status

Phase 2 hardening provides an integrated local vertical slice:

- content-hash anchored PUT/CUT edits with stale-anchor rejection;
- a required pure-Rust RLM core with compact-safe handles and structured evaluation;
- SQLite event replay, versioned session handoffs, persistent daemon sessions, and profile isolation;
- CLI help, version reporting, and an end-to-end `session-smoke` command;
- skill loading, registry search, pinning, metrics, and non-destructive archival;
- model-role routing with failover and cache accounting;
- independent evaluation raters and learned fixtures.

Live providers, production messaging, Jupyter ZMQ kernels, and stronger process isolation are deferred to Phase 3.

## Build and test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p darius-cli
```

Run the integrated smoke path:

```sh
cargo run -p darius-cli -- session-smoke
```

## RLM feature gates

The pure-Rust RLM API is always available and has no Python or ZMQ runtime requirement. Default features are empty. The optional Python backend is enabled explicitly:

```sh
cargo test -p darius-rlm
cargo test -p darius-rlm --features rlm-python
```

The optional backend is not required for normal workspace tests or the CLI.
