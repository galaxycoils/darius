# Changelog

All notable changes to this project will be documented in this file.

## [1.0.0] - 2026-08-30

### Added
- **darius-memory**: Single SQLite file per profile, WAL mode, FTS5 full-text search, MemoryPack capped at 3500 chars, content-hash dedupe, JSONL import/export
- **darius-tools**: ToolRegistry with disk spill above 32 KiB preview, TOOL line protocol, built-in memory/task tools, shell/read_file/write_file/glob tools
- **darius-cognitive**: CognitiveLoop (Plan → TaskBoard → ReAct → Accept), MockModel for offline tests
- **Live ModelRouter**: `LiveModel` behind `DARIUS_LIVE_MODEL` env var for `darius run`
- **Messaging adapter**: Thin I/O layer for Telegram/Discord/Slack (stub)
- **Optional Jupyter/ZMQ**: `IpKernelConnection` behind `rlm-ipykernel` feature
- **Isolation hardening**: `force_terminate`, `detect_gvisor`, `terminate_with_timeout`
- **CLI**: `darius run`, `darius memory *`, `darius session-smoke`
- **Integration e2e**: Temp profile → memory → tools → cognitive loop test

### Phase 2 (prior)
- Content-hash anchored PUT/CUT edits with stale-anchor rejection
- Pure-Rust RLM core with compact-safe handles
- SQLite event replay, versioned session handoffs, persistent daemon sessions
- Profile isolation, skill loading/registry, model-role routing

### Known Limitations
- Live provider HTTP integration is stub (Mock works offline)
- Production messaging I/O is stub
- No Firecracker, embeddings, cloud sync, or training
