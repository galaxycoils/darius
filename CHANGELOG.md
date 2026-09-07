# Changelog

All notable changes to this project will be documented in this file.

## [1.2.0] - 2026-09-07

### Added
- **Correlated Multi-Turn Agent Loop**: Implemented exact OpenAI-compatible adapter supporting function tool calls with strict ID correlation, cancellation tokens, and request deadlines.
- **Responsive Async Session Actor**: Replaced blocked synchronous loop with decoupled async session actor (`actor.rs`), keeping TUI controls responsive during provider latency and long-running tools.
- **Runtime Enforced Execution Policies**: Hard-enforced Auto and Plan modes. In Plan mode, mutating tool calls (`write_file`) and shell execution are strictly denied before execution.
- **Session Permission Management**: Interactive AllowOnce / AllowSession / Deny prompts for mutating and shell tools; noninteractive `darius run` rejects mutations with exit code 1 and guidance.
- **Canonical Command Registry**: Closed-world set of 13 slash commands and 4 CLI subcommands (`tui`, `run`, `config`, `memory`), with complete Clap argument parsing and help output.
- **Terminal Guard & Cleanup**: Guaranteed raw mode restoration, alternate screen exit, and cursor visibility across all normal and abnormal exits via RAII `TerminalGuard` and panic hook.
- **Robust Installer & Release Pipeline**: Aligned asset target naming (`darius-{macos|linux}-{aarch64|x86_64}.tar.gz`), local `--artifact-dir` install staging, sha256 checksum verification, and atomic binary replacement.

### Corrected
- **Retired Public Overclaims**: Formally retired unverified or non-working features from public exposure:
  - Retired unverified subagent orchestration (`subagent_steer`, `subagent_list`, `subagent_stop`).
  - Retired unverified cron job scheduling and persistence (`darius cron`).
  - Retired unverified MCP thin client registry.
  - Retired unverified A2A hub and peer messaging (`darius a2a`, `/a2a`, `/serve`).
  - Retired unverified worktree rollback and dry-run CLI (`darius approval-check`).
  - Corrected legacy commands (`daemon`, `status`, `start`, `stop`, `attach`, `eval`, `learn`, `session-smoke`).

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
