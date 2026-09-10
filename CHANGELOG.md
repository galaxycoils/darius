# Changelog

## [1.5.0] - 2026-09-09

### Added
- Native Anthropic Messages API provider (`AnthropicModel`) supporting Claude models (`claude-3-5-sonnet`, `claude-3-5-haiku`) with system message extraction, alternating user/assistant turns, and tool-use decoding.
- Real-time token streaming to TUI and `EventSink` via Server-Sent Events (SSE) for both OpenAI and Anthropic models with incremental tool call assembly.
- Dynamic MCP tool schema injection: discovered tools from MCP servers are automatically passed to model turns and registered in tool schemas.
- Live provider connectivity probe CLI command (`darius config probe`) verifying endpoint reachability, credentials, and roundtrip latency.
- Opt-in live provider verification integration test suite (`tests/live_providers_e2e.rs`) gated by `DARIUS_LIVE_TESTS=1`.
- Windows build matrix runner (`x86_64-pc-windows-msvc`) and zip packaging in CI release workflow and release scripts.

### Removed
- Removed `mock` default from model catalog and eliminated silent fallback to `MockModel` on missing credentials.

## [1.4.0] - 2026-09-09

### Added
- Local MCP (Model Context Protocol) stdio and SSE client support via `[[mcp.servers]]` profile configuration with JSON-RPC 2.0 framing and large output disk spill (>32 KiB).
- Session-scoped dynamic allowlist for discovered MCP tools: registers tools under namespaced identifiers (`mcp_{server}_{tool}`) and enforces closed-world model tool boundaries without static wildcards.
- TUI approval gating for mutating MCP tools with Auto permission prompts and Plan mode deny.
- MCP health diagnostics in `darius config show` and TUI `/status` without disclosing environment secrets.
- TUI write diff preview in session transcript with line-level addition and deletion markers (capped at 200 lines) on overwrite of pre-existing non-empty files.
- Verified test proofs for TUI `write_file` and `shell` AllowOnce, AllowSession, Deny, and Plan execution policies.

### Corrected
- Reconciled documented release target claims in `CAPABILITIES.md` (`macos-x86_64`, `macos-aarch64`, `linux-x86_64`) with the 3 targets actually built by the CI release matrix.
- Clarified write diff preview behavior: diff generation is active for file overwrites; first-time file creation remains summary-only (Option A).

## [1.3.0] - 2026-09-09

### Added
- `darius serve` CLI subcommand on `127.0.0.1:7432` by default: executor-gated web execution running the same policy-aware agent loop as `run`, with per-task SSE journals, task lookup, and an executor-gated agent card.
- Agent tool evidence: `search_files` result correlation, approved `memory_remember`/`memory_search` roundtrip with persisted record, task board completion of the returned id, and `spill_read` recall beyond the preview ceiling.
- Staged installer path: `scripts/pack-release.sh` plus `tests/install_staged_e2e.sh` repacking the current release binary on every run.
- README quickstart for build, provider config, TUI approvals, and serve curl examples.

### Corrected
- Web execution without a configured live provider refuses instead of simulating work; offline demo never executes goals.
- Headless web execution denies mutating tools; writes and shell work stay on the TUI approval surface.
- Capability rows promote only surfaces with named executable proof in the same checkout.

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
