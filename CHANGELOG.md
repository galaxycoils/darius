# Changelog

All notable changes to this project will be documented in this file.

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

### Notes
- PTY integration tests removed in favor of unit tests (PTY was flaky)
- 325 tests passing across workspace
- Brainless was used as visual/interaction inspiration only. No source was copied.

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
