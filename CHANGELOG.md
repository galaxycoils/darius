# Changelog

## [1.2.1] - 2026-09-08

### Fixed
- Fixed cargo fmt and clippy issues across workspace.
- Fixed SQLite FTS5 query token quoting for hyphenated terms in durable memory search.
- Fixed TUI auto-tail transcript scrolling to keep viewport at the conversation bottom.
- Fixed TUI screen clearing on `/clear` command via `terminal.clear()`.
- Persisted conversation session context to durable memory during `/compact`.

## [1.2.0] - 2026-09-07

### Added
- OpenAI-compatible multi-turn tool protocol with correlated responses, deadlines and cancellation.
- Async terminal session actor, Auto/Plan tool policies, and interactive approval prompts; noninteractive mutation requests are denied.
- Generated CLI help for `tui`, `run`, `config`, and `memory`; thirteen canonical slash commands.
- Explicit `--offline` demo, setup guidance, provider configuration and secret-safe diagnostics.
- Local installer staging with checksum verification and atomic replacement. Release assets and platform success still require release-job evidence.

### Corrected
- Removed the silent MockModel fallback claim: missing configured credentials are errors; only explicit `--offline` selects the demo.
- Removed unconditional terminal cleanup timing and restoration promises. Terminal guards cover tested paths; SIGKILL and failures outside those paths are not covered.
- Retired unverified MCP, subagent orchestration, cron, approval-check, peer_send, worktree rollback and A2A claims from public support.
- Disabled legacy web work submission, task delivery and event routes. Compatibility metadata has no executable capabilities; the dashboard states unavailable and has no active controls.
- Replaced stale capability links with named existing test functions. Test coverage is not proof of public release or third-party service availability.

## [1.1.2] - 2026-08-31

### Historical additions (not current capability proof)
- Terminal composer, palette, permission dialogs and event rendering were introduced.
- Provider HTTP client and localhost scaffolding were introduced with tests.
- Terminal guard, cancellation support and CI scaffolding were introduced.

### Corrected historical overclaims
- The advertised working web dashboard and A2A task server were unverified: goal submission only emitted synthetic events and task submission only stored pending records. These routes are now unavailable.
- Earlier claims of complete TUI safety and universal cleanup overstated the evidence. Current support is limited to the tested paths in the capability matrix.
- The prior live-provider wording meant OpenAI-compatible source/client tests, not verification of a deployed service. Native Anthropic support is unavailable.
- Internal web/A2A event types remain implementation details, not supported public features; A2A is unavailable.

## [1.1.0] - 2026-08-18

Historical work: FTS5 search, tool registry and model-router implementations. Source presence alone was not end-to-end evidence.

## [1.0.0] - 2026-08-18

Historical initial release: mock model, provider adapter, SQLite memory, cognitive-loop and session/event scaffolding. The old automatic offline fallback and broad live-completion claims are superseded by the explicit setup/live/offline-demo contract above.
