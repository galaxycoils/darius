# Darius

Darius is an open-source Rust agent harness. The active implementation and default branch is `master`; the historical `main` branch contains the obsolete scaffold.

## Lean cognitive phase status

Phase 3 delivers a **lean cognitive harness** on top of the Phase 2 baseline. Three deep crates, minimal dependencies, no embedding/vector DB required:

- **`darius-memory`** — single SQLite file per profile, WAL, FTS-ready, `MemoryPack` capped at 3500 chars, content-hash dedupe, JSONL import/export
- **`darius-tools`** — `ToolRegistry` with disk spill above 32 KiB preview, weak-model `TOOL {"name":"...","arguments":{...}}` line protocol, built-in `memory_search` / `memory_pack` / `memory_remember` / `task_add` / `task_list` / `task_complete`
- **`darius-cognitive`** — `CognitiveLoop`: Plan → TaskBoard → ReAct (capped at 12 iters/task) → Accept with `MockModel` tests (zero network)

CLI surface:

```sh
darius run --goal "..."          # full cognitive loop with MockModel
darius memory search <q>         # FTS search
darius memory pack               # bounded MemoryPack
darius memory import <file>      # deduped JSONL
darius memory export <file>      # JSONL export
darius memory stats              # record count
darius session-smoke             # daemon + session + handoff
```

Resource defaults: preview 32 KiB, pack 3500 chars, TaskBoard 15 tasks, ReAct 12 iters/task, single SQLite connection.

## Phase 2 status

Phase 2 hardening provides an integrated local vertical slice:

- content-hash anchored PUT/CUT edits with stale-anchor rejection;
- a required pure-Rust RLM core with compact-safe handles and structured evaluation;
- SQLite event replay, versioned session handoffs, persistent daemon sessions, and profile isolation;
- CLI help, version reporting, and an end-to-end `session-smoke` command;
- skill loading, registry search, pinning, metrics, and non-destructive archival;
- model-role routing with failover and cache accounting;
- independent evaluation raters and learned fixtures.

## Deferred

Live providers, production messaging, Jupyter ZMQ kernels, gVisor/Firecracker, full Hindsight graph persistence, embedding indexes, cloud sync, training/fine-tuning.

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
