# Changelog

All notable changes to this project will be documented in this file.

## [1.2.0] - 2026-09-01

### Added
- **Lean-Tail Context Compression**: `darius-cognitive/src/compress.rs` implements `lean_tail_compress` and `/compact` command to maintain head system context and recent turns while discarding middle bulk under token budgets.
- **Tool Spill Recall Path**: Large tool outputs (>32 KiB) automatically spill to disk in `tool_results/`; added `spill_read`/`read_spill` tools restricted to spilled results.
- **Live Subagent Steer / List / Stop & Schema Validation**: `LocalSubagentRuntime` supporting live mid-turn message injection (`subagent_steer`), active task listing (`subagent_list`), graceful cancellation (`subagent_stop`), and JSON Schema output validation.
- **Cron Memory Continuity & Persistent Notepad**: Scheduled recurring jobs preserve durable memory and notepads across runs, with hash-based change detection to skip model invocations when payloads remain unchanged.
- **Instruction-File Write Protection**: Safety gate and `InstructionWriteGate` requiring explicit approval before modifying `AGENTS.md`, `SKILL.md`, `skills/`, `memory.db`, or `.darius/`.
- **Secret Redaction**: Automatic regex-driven scrubbing of `sk-` keys, Bearer tokens, and password/secret fields across all tool outputs, UI events, and traces.
- **Prompt-Cache Coordinator**: Deterministic prefix hashing (`compute_prefix_cache_key`) and hit/miss token tracking to optimize LLM prompt cache performance.
- **MCP Thin Client & Registry**: Stdio and SSE Model Context Protocol client with ping health checks, tool discovery, and step-gating enforcement.
- **Peer A2A Messaging**: Direct agent-to-agent envelope messaging (`POST /a2a/peer`, `GET /a2a/inbox/{handle}`) with sender quota rate-limiting.
- **Live Status Metrics & Fuzzy Command Palette**: Real-time cache hit ratio, memory char size, and subagent counters in TUI status footer, accompanied by subsequence fuzzy palette filtering.
- **Worktree Management & Rollback**: Isolated git worktree lifecycle management with session rollback and TTL-based pruning.
- **Approval Dry-Run CLI**: `darius approval-check <tool> [args]` command to verify permission requirements and risk level without execution.
- **Dynamic Role Model Overrides**: Profile config support for `[model_overrides]` routing specialized roles (`planner`, `rater`, `smol`, `advisor`).
- **Comprehensive E2E Integration Suite**: End-to-end integration tests verifying the full matrix of new capabilities.

## [1.1.2] - 2026-08-31

### Added
- Functional Claude-Code-style TUI with proper terminal interaction
- Ordinary text submission (q/j/k type normally, not shortcuts)
- Command palette with `/` and `-` aliases
- Permission gating for mutating/shell tools with AllowOnce/AllowSession/Deny
- Live cognitive events streamed to TUI via broadcast channels
- Reusable model across multiple TUI turns
- Cancellation support (Ctrl+C interrupts active turns)
- TuiWorker with ChannelRunControl for safe tool execution
- Tool risk classification (ReadOnly/Mutating/Shell)
- Darius web dashboard (Axum + SSE)
- A2A agent card + task server
- Real OpenAI-compatible provider HTTP client with wiremock tests
- IPyKernel RLM backend (feature-gated)
- Terminal lifecycle guard with drop-order test
- CI workflow for continuous integration

### Fixed
- TUI reducer now handles all action variants
- AppState uses structured transcript/tasks instead of raw strings
- CognitiveLoop exposes EventSink/RunControl traits
- Terminal event loop polls crossterm without blocking indefinitely
- Unified UiEvent/runtime across CLI, TUI, web, and A2A
- Real OpenAI-compatible provider requests and localhost server startup

## [1.1.0] - 2026-08-18

### Added
- FTS5-backed memory search
- Extended tool registry (shell, file read/write, glob)
- Live ModelRouter for `darius run`

## [1.0.0] - 2026-08-18

### Added
- Initial release
- Offline MockModel (no network)
- Live provider when configured
- Durable SQLite memory with FTS5 search
- Plan–execute–accept cognitive loop
- CLI with memory operations
- Session handoff + event replay
