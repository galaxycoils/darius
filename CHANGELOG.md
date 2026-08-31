# Changelog

All notable changes to this project will be documented in this file.

## [1.1.1] - 2026-08-31

### Added
- Claude-Code-style Darius TUI with streaming turns, command palette, modes, effort, todos, diffs, and permission chooser
- Darius web dashboard (Axum + SSE)
- A2A agent card + task server
- SessionRuntime shared across CLI, TUI, web, and A2A
- Real OpenAI-compatible provider HTTP client with wiremock tests
- IPyKernel RLM backend (feature-gated)
- Terminal lifecycle guard with drop-order test
- CI workflow for continuous integration

### Fixed
- Unified UiEvent/runtime across CLI, TUI, web, and A2A
- Real OpenAI-compatible provider requests and localhost server startup
- Verified release checksums and tag handling

### Notes
- Brainless was used as visual/interaction inspiration only. No source was copied.

## [1.1.0] - 2026-08-18

### Added
- FTS5-backed memory search
- Extended tool registry (shell, read_file, write_file, glob)
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
