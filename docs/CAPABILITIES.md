# Darius capability inventory

Scope: v1.4.0 release. Verified means the narrowly described local contract has an executable test, not that a release, hosted model, or every operating system was validated. Run the linked test suite at the exact checkout before release. Experimental is not used here; source-only features are unavailable.

## Verified local contracts

| Surface and claim boundary | Status | Named proof |
| --- | --- | --- |
| Generated top-level CLI exposes tui/run/config/memory/serve | **Verified** | [`public_help_is_exact_generated_surface`](../crates/darius-cli/tests/cli_contract.rs) |
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
| Web goal execution, correlated SSE, and task completion | **Verified** | [`goal_post_streams_correlated_sse_until_done`](../crates/darius-web/tests/transport.rs), [`failed_execution_reports_failed_task_and_error_sse`](../crates/darius-web/tests/transport.rs), [`web_executor_runs_agent_loop_with_real_tool_result`](../crates/darius-cli/tests/web_execute.rs), [`missing_executor_never_claims_execution`](../crates/darius-web/tests/public_claims.rs) |
| Agent `search_files` via local fake provider, real path in tool result | **Verified** | [`test_run_search_files_goal_succeeds`](../crates/darius-cli/tests/run_fixtures/search.rs) |
| Approved agent `memory_remember` persists; `memory_search` recalls it | **Verified** | [`tui_memory_tools_roundtrip_persists_approved_record`](../crates/darius-cli/src/tui_runtime/tests/memory_roundtrip.rs) |
| Agent task board completes the actually returned task id | **Verified** | [`tui_task_board_tools_complete_actual_returned_id`](../crates/darius-cli/src/tui_runtime/tests/task_roundtrip.rs) |
| `spill_read` recalls the marker beyond the real preview ceiling | **Verified** | [`spill_read_recalls_marker_beyond_real_preview_ceiling`](../crates/darius-tools/tests/spill_roundtrip.rs) |
| TUI `write_file` AllowOnce creates file on disk | **Verified** | [`tui_allow_once_write_file_creates_disk_file`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs), [`permission_lifecycle_once_does_not_persist_and_denial_is_correlated`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs) |
| TUI `write_file` AllowSession normalizes path across turns; Deny leaves no file; Plan mode denies before execution | **Verified** | [`permission_lifecycle_session_normalizes_path_across_turns_but_not_targets`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs), [`tui_deny_write_leaves_no_file`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs), [`execution_policy_plan_denies_before_permission_and_readonly_runs`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs) |
| TUI `shell` AllowOnce captures stdout in tool result preview | **Verified** | [`tui_allow_once_shell_echo_appears_in_tool_result`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs) |
| TUI `shell` AllowSession caches exact command and reprompts different command | **Verified** | [`tui_allow_session_shell_exact_command_caches_and_different_reprompts`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs), [`permission_lifecycle_session_exact_shell_command_and_complete_task_arguments`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs), [`permission_lifecycle_shell_key_binds_canonical_workspace_and_exact_command`](../crates/darius-cli/src/tui_runtime/tests/permission_keys.rs) |
| TUI `write_file` diff preview on overwrite in transcript (creates remain summary-only per Option A) | **Verified** | [`tui_write_diff_appears_in_transcript_on_overwrite`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs) |
| Profile `[[mcp.servers]]` parse (stdio and sse) | **Verified** | [`config_parses_mcp_servers_stdio_and_sse`](../crates/darius-cli/tests/config.rs) |
| `StdioMcpClient` initialize and tools/list via fixture | **Verified** | [`stdio_mcp_client_connect_and_list_tools`](../crates/darius-tools/tests/stdio_mcp.rs) |
| `StdioMcpClient` tools/call echo and output spill | **Verified** | [`stdio_mcp_client_call_tool_echo_success`](../crates/darius-tools/tests/stdio_mcp.rs), [`stdio_mcp_client_spill_large_output`](../crates/darius-tools/tests/stdio_mcp.rs) |
| Runtime registers `mcp_{server}_{tool}` names | **Verified** | [`runtime_starts_and_registers_mcp_servers_from_profile`](../crates/darius-cli/src/runtime.rs) |
| Session-scoped dynamic allowlist admits discovered tools, rejects undiscovered `mcp_*` names | **Verified** | [`model_tool_allowlist_rejects_hidden_with_correlated_error`](../crates/darius-tools/src/model_tools.rs) |
| Mutating MCP tools gated in TUI and denied in Plan mode | **Verified** | [`tui_approval_gates_mutating_mcp_tools_and_plan_denies`](../crates/darius-cli/src/tui_runtime/tests/permission_lifecycle.rs) |

## Retained surfaces and policy

CLI: `tui`, `run <goal...>`, `serve`, `config show`, `config init`, `config preset`, `memory search <query>`, `memory pack`, `memory import <file>`, `memory export <file>`, `memory stats`. Missing required nested arguments fail; short command aliases and --session are unavailable. `serve` binds loopback by default and refuses to execute when no live provider is configured; offline demo never executes goals.

Slash registry: `/help`, `/clear`, `/compact`, `/model`, `/mode`, `/permissions`, `/memory`, `/pack`, `/tasks`, `/status`, `/config`, `/stop`, `/quit`. `/model` opens an interactive picker or selects from catalog. Auto gates mutating/shell tools on approval; Plan denies them. This is a tool policy, not a process sandbox or a guarantee that session storage is never written.

Tool inventory: `memory_search`, `memory_pack`, `memory_remember`, `task_add`, `task_list`, `task_complete`, `shell`, `read_file`, `search_files`, `write_file`, `spill_read`. Individual registrations are not blanket end-to-end verification. Shell authorization does not confine arbitrary subprocess effects. Cancellation/terminal restoration coverage is path-specific, without a universal latency bound.

## Unavailable integrations and removed names

Unavailable CLI: `daemon`, `status`, `start`, `stop`, `attach`, `eval`, `learn`, `session-smoke`, `a2a`, `cron`, `approval-check`, `help`, `doctor`, `config set`. Doctor means the config/status diagnostic workflow, not a new command.

Unavailable slash commands: `/plan`, `/effort`, `/skills`, `/a2a`, `/serve`; unavailable mode values: Manual and AcceptEdits. Use `/mode plan` instead.

Unavailable: `peer_send`, `subagent_spawn`, `subagent_list`, `subagent_steer`, `subagent_stop`, peer MCP fleet, cron scheduling, worktree management/rollback, browser integration, skill mutation and remote sandboxes. Internal Rust symbols are not public support claims.

Web execution is executor-gated: without an injected runtime the router answers 503 and advertises no capabilities. With the CLI runtime, goals run the same policy-aware agent loop as `run`, headless mutations are denied, and per-task SSE replays the journal to a terminal event. Unavailable: peer messaging and multi-peer fleets. `GET /a2a/peer`, `/a2a/inbox/{handle}` return 404. `/health`, `/status` return 404.

## Provider and platform boundaries

OpenAI-compatible wire behavior is tested with local fake providers. Credentialed OpenAI, OpenRouter, Groq, Ollama, native Anthropic, automatic failover and role-model overrides are not verified integrations. A `live` diagnostic means configuration/key selection, not a connectivity or health probe. Only `--offline` selects the demo. See [setup and diagnostics](TROUBLESHOOTING.md).

Release target naming: `macos-x86_64`, `macos-aarch64`, `linux-x86_64`. Historical `darwin-x86_64`/`darwin-aarch64` names are not current asset names. Published assets, credentialed downloads, cross-platform installation and Windows support are unavailable as claims from this local audit. Local installer tests are not publication evidence.

## Audit boundary

`bash scripts/audit-public-claims.sh` rebuilds the CLI, traverses generated nested help, checks diagnostics/installer help, authoritative prose and manifest descriptions, and executes negative fixtures plus web/slash/CLI contracts. It verifies named capability proof functions exist; it does not execute all linked PTY, installer or provider journey suites. Those remain separate release gates. Release workflow literals are checked, but the workflow itself is not executed locally. Pattern rules are a regression guard, not an exhaustive semantic proof of arbitrary future copy.
