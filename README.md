# Darius

Darius is an open-source Rust agent harness. The active implementation and default branch is `master`; the historical `main` branch has been removed.

## Lean cognitive phase status

Phase 3 delivers a **lean cognitive harness** on top of the Phase 2 baseline. Three deep crates, minimal dependencies, no embedding/vector DB required:

- **`darius-memory`** — single SQLite file per profile, WAL, FTS-ready, `MemoryPack` capped at 3500 chars, content-hash dedupe, JSONL import/export
- **`darius-tools`** — `ToolRegistry` with disk spill above 32 KiB preview, weak-model `TOOL {"name":"...","arguments":{...}}` line protocol, built-in `memory_search` / `memory_pack` / `memory_remember` / `task_add` / `task_list` / `task_complete`
- **`darius-cognitive`** — `CognitiveLoop`: Plan → TaskBoard → ReAct (capped at 12 iters/task) → Accept with `MockModel` tests (zero network)

CLI surface:

```sh
darius run --goal "..."          # full cognitive loop (mock or live)
darius memory search <q>         # FTS search
darius memory pack               # bounded MemoryPack
darius memory import <file>      # deduped JSONL
darius memory export <file>      # JSONL export
darius memory stats              # record count
darius session-smoke             # daemon + session + handoff
```

Resource defaults: preview 32 KiB, pack 3500 chars, TaskBoard 15 tasks, ReAct 12 iters/task, single SQLite connection per engine.

## Remaining work completed

| Area | Status |
|------|--------|
| Branch hygiene (`main` removed, `master` default) | ✅ |
| Integration e2e test (memory + tools + cognitive) | ✅ |
| Live ModelRouter for `darius run` (env-gated) | ✅ |
| A2A Agent Card HTTP response | ✅ |
| Messaging adapter (Telegram/Discord/Slack in `platform_adapters.rs`) | ✅ |
| Optional Jupyter/ZMQ RLM backend (behind `rlm-ipykernel` feature) | ✅ |
| Isolation hardening (process kill + gVisor detect) | ✅ |
| Release binary (2.1 MB arm64) | ✅ |

### Live ModelRouter for `darius run`

Set `DARIUS_LIVE_MODEL=1` to route cognitive loop requests through the `ModelRouter` (provider registry, budget enforcement, failover). Without the env var, `darius run` uses the offline `MockModel`.

```sh
DARIUS_LIVE_MODEL=1 darius run --goal "analyze this"
```

### Optional Jupyter/IPython kernel backend

The `darius-rlm` crate has an optional `IpKernelConnection` behind the `rlm-ipykernel` feature flag. Default builds never link `zmq`.

```sh
cargo test -p darius-rlm --features rlm-ipykernel ipykernel
```

### Isolation hardening

The sandbox module now includes:
- `detect_gvisor()` — checks for `runsc` binary in common locations
- `force_terminate(pid)` — sends SIGKILL (cross-platform)
- `terminate_with_timeout(pid, timeout_ms)` — graceful SIGTERM then force kill

```sh
cargo test -p darius-daemon sandbox
```

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

Live provider HTTP integration (ModelRouter `route()` is stub), production messaging I/O (Telegram adapter is stub), Firecracker fleet, mandatory embeddings, cloud memory sync, training/fine-tuning.

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

The pure-Rust RLM API is always available and has no Python or ZMQ runtime requirement. Default features are empty. Optional backends are enabled explicitly:

```sh
cargo test -p darius-rlm
cargo test -p darius-rlm --features rlm-python
cargo test -p darius-rlm --features rlm-ipykernel
```

Optional backends are not required for normal workspace tests or the CLI.
