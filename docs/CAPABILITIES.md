# Darius capability inventory

Scope: v1.2.1 release. Verified means the narrowly described local contract has an executable test, not that a release, hosted model, or every operating system was validated. Run the linked test suite at the exact checkout before release. Experimental is not used here; source-only features are unavailable.

## Verified local contracts

| Surface and claim boundary | Status | Named proof |
| --- | --- | --- |
| Generated top-level CLI exposes only tui/run/config/memory | **Verified** | [`public_help_is_exact_generated_surface`](../crates/darius-cli/tests/cli_contract.rs) |
| Version flags report the package version | **Verified** | [`version_flags_print_the_current_package_version`](../crates/darius-cli/tests/cli_contract.rs) |
| Global --profile/--cwd/--offline parse before and after commands | **Verified** | [`globals_parse_before_and_after_subcommand`](../crates/darius-cli/tests/cli_contract.rs) |
| Removed CLI tokens fail with exit 2 | **Verified** | [`parse_errors_unknown_and_legacy_commands_exit_two`](../crates/darius-cli/tests/cli_contract.rs) |
| Explicit demo is labelled and does not claim analysis/completion | **Verified** | [`runtime_selection_offline_is_explicit_and_never_claims_analysis_or_completion`](../crates/darius-cli/tests/cli_contract.rs) |
| Missing configured key reports the variable, not its value | **Verified** | [`runtime_selection_configured_missing_key_names_only_the_variable`](../crates/darius-cli/tests/cli_contract.rs) |
| No usable configuration/key selects setup, not a mock | **Verified** | [`runtime_selection_no_config_or_usable_key_enters_setup`](../crates/darius-cli/tests/cli_contract.rs) |
| Config/status diagnostics report runtime selection, not remote health | **Verified** | [`runtime_selection_config_show_exposes_secret_safe_diagnostics`](../crates/darius-cli/tests/cli_contract.rs), [`runtime_selection_status_exposes_diagnostics_without_done`](../crates/darius-cli/tests/cli_contract.rs) |
| Noninteractive read/tool goal via local fake provider | **Verified** | [`test_run_read_goal_succeeds`](../crates/darius-cli/tests/run_e2e.rs) |
| Noninteractive mutation denied with TUI guidance | **Verified** | [`test_run_mutation_goal_denied_with_tui_guidance`](../crates/darius-cli/tests/run_e2e.rs) |
| Config init/show lifecycle | **Verified** | [`test_config_show_and_init`](../crates/darius-cli/tests/run_e2e.rs) |
| Memory search/pack/import/export/stats lifecycle | **Verified** | [`test_memory_cli_lifecycle`](../crates/darius-cli/tests/run_e2e.rs) |
| Clean-home bare PTY setup and selected exit cleanup | **Verified** | [`clean_home_bare_launch`](../crates/darius-cli/tests/tui_pty.rs), [`cleanup_path_idle_ctrl_c`](../crates/darius-cli/tests/tui_pty.rs) |
| Local fake-provider multi-turn TUI journey | **Verified** | [`full_agent_journey`](../crates/darius-cli/tests/tui_pty.rs) |
| Generated slash help and offline status do not claim hidden features or completion | **Verified** | [`generated_slash_help_and_status_are_truthful`](../crates/darius-cli/tests/public_claims.rs) |
| Closed-world slash palette/registry, not every command outcome | **Verified** | [`generated_slash_palette_has_only_supported_commands`](../crates/darius-core/tests/public_claims.rs) |
| Interactive model picker opens, filters, selects, and persists to profile | **Verified** | [`model_picker_then_run_mock`](../crates/darius-cli/tests/tui_pty.rs) |
| Mock model TUI agent journey with truthful offline status | **Verified** | [`full_agent_journey_mock`](../crates/darius-cli/tests/tui_pty.rs) |
| Exhaustive slash command handling across all 13 canonical commands | **Verified** | [`slash_command_execution_semantic_table_all_13_commands`](../crates/darius-cli/src/tui_runtime/tests.rs) |
| Tool events paired and rendered in session transcript | **Verified** | [`tool_events_appear_in_session_transcript`](../crates/darius-tui/src/app.rs) |
| Compatibility web refuses execution and advertises no capabilities | **Verified** | [`unavailable_web_surface_never_claims_execution`](../crates/darius-web/tests/public_claims.rs) |

## Retained surfaces and policy

CLI: `tui`, `run <goal...>`, `config show`, `config init`, `config preset`, `memory search <query>`, `memory pack`, `memory import <file>`, `memory export <file>`, `memory stats`. Missing required nested arguments fail; short command aliases and --session are unavailable.

Slash registry: `/help`, `/clear`, `/compact`, `/model`, `/mode`, `/permissions`, `/memory`, `/pack`, `/tasks`, `/status`, `/config`, `/stop`, `/quit`. `/model` opens an interactive picker or selects from catalog. Auto gates mutating/shell tools on approval; Plan denies them. This is a tool policy, not a process sandbox or a guarantee that session storage is never written.

Tool inventory: `memory_search`, `memory_pack`, `memory_remember`, `task_add`, `task_list`, `task_complete`, `shell`, `read_file`, `search_files`, `write_file`, `spill_read`. Individual registrations are not blanket end-to-end verification. Shell authorization does not confine arbitrary subprocess effects. Cancellation/terminal restoration coverage is path-specific, without a universal latency bound.

## Unavailable integrations and removed names

Unavailable CLI: `daemon`, `status`, `start`, `stop`, `attach`, `eval`, `learn`, `session-smoke`, `serve`, `a2a`, `cron`, `approval-check`, `help`, `doctor`, `config set`. Doctor means the config/status diagnostic workflow, not a new command.

Unavailable slash commands: `/plan`, `/effort`, `/skills`, `/a2a`, `/serve`; unavailable mode values: Manual and AcceptEdits. Use `/mode plan` instead.

Unavailable: `peer_send`, `subagent_spawn`, `subagent_list`, `subagent_steer`, `subagent_stop`, MCP, cron scheduling, worktree management/rollback, browser integration, skill mutation and remote sandboxes. Internal Rust symbols are not public support claims.

Unavailable web/A2A execution: no CLI listener is shipped. If embedded, `GET /` renders an unavailable notice; `GET /a2a/card` returns an empty capabilities array. `/api/events`, `/api/goal`, `/a2a/tasks`, `/a2a/tasks/{id}`, `/a2a/peer`, `/a2a/inbox/{handle}`, `/health`, `/status` return 503 unavailable, without creating work or reporting success.

## Provider and platform boundaries

OpenAI-compatible wire behavior is tested with local fake providers. Credentialed OpenAI, OpenRouter, Groq, Ollama, native Anthropic, automatic failover and role-model overrides are not verified integrations. A `live` diagnostic means configuration/key selection, not a connectivity or health probe. Only `--offline` selects the demo. See [setup and diagnostics](TROUBLESHOOTING.md).

Release target naming: `macos-x86_64`, `macos-aarch64`, `linux-x86_64`, `linux-aarch64`. Historical `darwin-x86_64`/`darwin-aarch64` names are not current asset names. Published assets, credentialed downloads, cross-platform installation and Windows support are unavailable as claims from this local audit. Local installer tests are not publication evidence.

## Audit boundary

`bash scripts/audit-public-claims.sh` rebuilds the CLI, traverses generated nested help, checks diagnostics/installer help, authoritative prose and manifest descriptions, and executes negative fixtures plus web/slash/CLI contracts. It verifies named capability proof functions exist; it does not execute all linked PTY, installer or provider journey suites. Those remain separate release gates. Release workflow literals are checked, but the workflow itself is not executed locally. Pattern rules are a regression guard, not an exhaustive semantic proof of arbitrary future copy.
