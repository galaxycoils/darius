# Graph Report - Darius  (2026-08-30)

## Corpus Check
- 60 files · ~30,684 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1033 nodes · 1755 edges · 57 communities (54 shown, 3 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 13 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `643f7743`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 8|Community 8]]
- [[_COMMUNITY_Community 9|Community 9]]
- [[_COMMUNITY_Community 10|Community 10]]
- [[_COMMUNITY_Community 11|Community 11]]
- [[_COMMUNITY_Community 12|Community 12]]
- [[_COMMUNITY_Community 13|Community 13]]
- [[_COMMUNITY_Community 14|Community 14]]
- [[_COMMUNITY_Community 15|Community 15]]
- [[_COMMUNITY_Community 16|Community 16]]
- [[_COMMUNITY_Community 17|Community 17]]
- [[_COMMUNITY_Community 18|Community 18]]
- [[_COMMUNITY_Community 19|Community 19]]
- [[_COMMUNITY_Community 20|Community 20]]
- [[_COMMUNITY_Community 21|Community 21]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 23|Community 23]]
- [[_COMMUNITY_Community 24|Community 24]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 27|Community 27]]
- [[_COMMUNITY_Community 28|Community 28]]
- [[_COMMUNITY_Community 29|Community 29]]
- [[_COMMUNITY_Community 30|Community 30]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 36|Community 36]]
- [[_COMMUNITY_Community 37|Community 37]]
- [[_COMMUNITY_Community 38|Community 38]]
- [[_COMMUNITY_Community 39|Community 39]]
- [[_COMMUNITY_Community 40|Community 40]]
- [[_COMMUNITY_Community 41|Community 41]]
- [[_COMMUNITY_Community 42|Community 42]]
- [[_COMMUNITY_Community 43|Community 43]]
- [[_COMMUNITY_Community 45|Community 45]]
- [[_COMMUNITY_Community 46|Community 46]]
- [[_COMMUNITY_Community 47|Community 47]]
- [[_COMMUNITY_Community 48|Community 48]]
- [[_COMMUNITY_Community 49|Community 49]]

## God Nodes (most connected - your core abstractions)
1. `ContinualHarness` - 18 edges
2. `Daemon` - 17 edges
3. `KanbanBoard` - 15 edges
4. `CronScheduler` - 14 edges
5. `EventLog` - 14 edges
6. `MemoryEngine` - 14 edges
7. `QuotaManager` - 13 edges
8. `run()` - 12 edges
9. `SkillCurator` - 12 edges
10. `Profile` - 12 edges

## Surprising Connections (you probably didn't know these)
- `rlm_returns_handle()` --calls--> `rlm()`  [INFERRED]
  tests/harness_e2e/src/lib.rs → crates/darius-rlm/src/lib.rs
- `rlm_evaluate_returns_grade()` --calls--> `rlm_evaluate()`  [INFERRED]
  tests/harness_e2e/src/lib.rs → crates/darius-rlm/src/evaluate.rs
- `write_file()` --calls--> `compute_anchor()`  [INFERRED]
  crates/darius-daemon/src/tools.rs → crates/darius-hashline/src/anchors.rs
- `rlm_returns_handle_with_running_status()` --calls--> `rlm()`  [INFERRED]
  crates/darius-core/tests/smoke.rs → crates/darius-rlm/src/lib.rs
- `rlm_evaluate_returns_structured_grade()` --calls--> `rlm_evaluate()`  [INFERRED]
  crates/darius-core/tests/smoke.rs → crates/darius-rlm/src/evaluate.rs

## Communities (57 total, 3 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.05
Nodes (25): Grade, IsolationTier, RubricScore, evaluate_returns_grade_with_scores(), rlm_evaluate(), handle_schema_bound(), handle_send_requires_running_state(), handle_survives_compaction() (+17 more)

### Community 1 - "Community 1"
Cohesion: 0.09
Nodes (16): current_timestamp(), Skill, curator_archives_to_backup_without_deleting_registry_entry(), curator_auto_archives_stale_skills(), curator_does_not_archive_builtin_or_user_skills(), curator_does_not_mark_archived_when_backup_fails(), curator_pin_exempts_from_archival(), CuratorMetrics (+8 more)

### Community 2 - "Community 2"
Cohesion: 0.06
Nodes (13): AdapterError, AdapterManager, discord_adapter_platform_name(), DiscordAdapter, IncomingMessage, PlatformAdapter, slack_adapter_platform_name(), SlackAdapter (+5 more)

### Community 3 - "Community 3"
Cohesion: 0.06
Nodes (11): GVisorBackend, MicroVmBackend, NamespaceBackend, plugin_spawns_t3(), ProcessBackend, sandbox_manager_selects_correct_backends(), SandboxBackend, SandboxError (+3 more)

### Community 4 - "Community 4"
Cohesion: 0.13
Nodes (21): extract_tool_calls(), extract_tool_calls_from_prose(), large_payload_spills_to_disk(), memory_remember_builtin_stores_record(), memory_search_builtin_returns_results(), parse_tool_line(), parse_tool_line_valid(), register_memory_builtins() (+13 more)

### Community 5 - "Community 5"
Cohesion: 0.12
Nodes (17): base_prompt_never_mutated(), ContinualHarness, current_timestamp(), emit_skills_returns_list(), harness_creates_with_base_prompt(), HarnessSnapshot, ingest_target_adds_memory(), memories_crud() (+9 more)

### Community 6 - "Community 6"
Cohesion: 0.12
Nodes (16): blake3_hash(), body_over_32_kib_rejected(), content_hash_stable_for_same_body(), distill_handoff_creates_records(), import_jsonl_dedupes_duplicates(), memory_pack_respects_bounds(), MemoryEngine, MemoryError (+8 more)

### Community 7 - "Community 7"
Cohesion: 0.10
Nodes (16): Breakpoint, dap_pause_resume(), dap_set_and_remove_breakpoint(), dap_step_builds_call_stack(), DapDebugger, Diagnostic, DiagnosticSeverity, FormatRequest (+8 more)

### Community 8 - "Community 8"
Cohesion: 0.10
Nodes (16): discover_mcp_servers(), discover_mcp_servers_returns_plugins(), hook_set_add_and_get(), HookSet, HookType, mcp_server_plugin_metadata(), McpServerPlugin, Plugin (+8 more)

### Community 9 - "Community 9"
Cohesion: 0.11
Nodes (15): budget_enforcer_exceeds_limit(), budget_enforcer_records_usage(), budget_enforcer_within_limit(), BudgetEnforcer, BudgetScope, model_router_budget_exceeded(), model_router_fails_over_when_primary_is_disabled(), model_router_routes_by_role() (+7 more)

### Community 10 - "Community 10"
Cohesion: 0.19
Nodes (13): created_session_survives_dropped_handle(), current_timestamp(), Daemon, daemon_a2a_server_serves_card(), daemon_attach_detach_session(), daemon_double_start_fails(), daemon_end_session_emits_handoff(), daemon_start_stop() (+5 more)

### Community 11 - "Community 11"
Cohesion: 0.10
Nodes (15): current_timestamp(), IsolationTier, ApprovalTier, audit_log_records_checks(), AuditEntry, AuditLog, AuditStatus, Capability (+7 more)

### Community 12 - "Community 12"
Cohesion: 0.16
Nodes (14): AgentQuota, AgentState, can_accept_within_limits(), cannot_accept_unknown_agent(), complete_task_decrements_active(), concurrency_limit_enforced(), current_timestamp(), queue_policy_drop_oldest_makes_room() (+6 more)

### Community 13 - "Community 13"
Cohesion: 0.18
Nodes (13): append_and_replay(), append_batch_is_transactional(), current_timestamp(), Event, EventLog, EventLogError, integrity_check_passes_on_clean_db(), open_creates_db_with_wal_and_full() (+5 more)

### Community 14 - "Community 14"
Cohesion: 0.14
Nodes (11): collector_starts_and_ends_span(), current_timestamp(), otlp_exporter_with_headers(), OtlpExporter, Span, span_handle_end_with_error(), SpanCategory, SpanHandle (+3 more)

### Community 15 - "Community 15"
Cohesion: 0.14
Nodes (20): anchor_does_not_match_different_content(), anchor_does_not_match_different_path(), anchor_for(), anchor_matches_same_content(), AstBoundary, compute_anchor(), different_content_different_hash(), FileAnchor (+12 more)

### Community 16 - "Community 16"
Cohesion: 0.16
Nodes (9): add_and_get_task(), claim_and_promote(), current_timestamp(), failure_circuit_breaker(), KanbanBoard, KanbanError, KanbanTask, reclaim_stale_claim() (+1 more)

### Community 17 - "Community 17"
Cohesion: 0.16
Nodes (9): A2aServer, agent_card_serve_json(), AgentCard, create_and_get_task(), current_timestamp(), list_tasks_by_session(), Task, TaskState (+1 more)

### Community 18 - "Community 18"
Cohesion: 0.19
Nodes (11): audit_log_records_events(), AuditEvent, ComplianceError, ComplianceManager, current_timestamp(), delete_profile_data(), export_profile_data(), is_event_expired() (+3 more)

### Community 19 - "Community 19"
Cohesion: 0.16
Nodes (8): add_and_get_job(), circuit_breaker_disables_after_max_failures(), context_chain_a_to_b(), CronError, CronJob, CronScheduler, current_timestamp(), success_resets_failure_count()

### Community 20 - "Community 20"
Cohesion: 0.17
Nodes (15): Acceptance, CognitiveError, loop_completes_with_mock_model(), loop_rejects_empty_plan(), loop_respects_max_tasks(), LoopPolicy, MockModel, Model (+7 more)

### Community 21 - "Community 21"
Cohesion: 0.15
Nodes (6): DiskFilesystem, Filesystem, FilesystemError, InMemoryFilesystem, resolve_path(), walk_dir()

### Community 22 - "Community 22"
Cohesion: 0.21
Nodes (9): compress_and_recall_session(), cross_session_decision_recall(), current_timestamp(), HindsightMemory, MemoryError, MentalModel, no_cross_profile_leak(), search_memories() (+1 more)

### Community 23 - "Community 23"
Cohesion: 0.24
Nodes (9): ContinuousLearner, current_timestamp(), failure_creates_fixture_that_fails_eval_until_refined(), LearnConfig, learner_captures_failure(), learner_gated_does_not_save(), learner_generates_fixture(), learner_learn_saves_fixture() (+1 more)

### Community 24 - "Community 24"
Cohesion: 0.18
Nodes (10): e2e_full_session_pipeline(), E2EError, E2EReport, mock_llm_round_trip(), MockLlm, rlm_kernel_lifecycle(), rlm_returns_handle(), run_e2e() (+2 more)

### Community 25 - "Community 25"
Cohesion: 0.20
Nodes (6): Profile, profile_creates_isolated_directories(), profile_rejects_path_traversal(), ProfileError, temp_profile_dir(), two_profiles_do_not_share_data()

### Community 26 - "Community 26"
Cohesion: 0.15
Nodes (14): Channel, ChannelReceiver, ChannelSender, ConnectionFile, Endpoint, execute_code(), execute_code_returns_message(), generate_connection_file_has_valid_ports() (+6 more)

### Community 27 - "Community 27"
Cohesion: 0.28
Nodes (9): BackupError, BackupManager, copy_dir_all(), create_and_restore_backup(), current_timestamp(), delete_backup(), list_backups_newest_first(), temp_dirs() (+1 more)

### Community 28 - "Community 28"
Cohesion: 0.23
Nodes (6): cache_coordinator_records_turns(), CacheCoordinator, CacheMetrics, clear_stats_resets(), recent_stats_returns_last_n(), version_bump_increments()

### Community 29 - "Community 29"
Cohesion: 0.33
Nodes (8): handoff_round_trip_via_store(), HandoffError, HandoffStore, store_delete(), store_list_sessions(), store_load_or_default_returns_new_when_missing(), store_save_and_load(), temp_dir()

### Community 30 - "Community 30"
Cohesion: 0.20
Nodes (9): Benchmark, BenchmarkCategory, BenchmarkMetrics, BenchmarkResult, BenchmarkRunner, BenchmarkSuite, runner_executes_benchmarks(), runner_returns_results() (+1 more)

### Community 31 - "Community 31"
Cohesion: 0.19
Nodes (12): bash(), bash_echo(), glob(), glob_dir(), glob_finds_files(), grep(), grep_finds_pattern(), GrepMatch (+4 more)

### Community 32 - "Community 32"
Cohesion: 0.29
Nodes (14): cmd_attach(), cmd_daemon(), cmd_eval(), cmd_learn(), cmd_memory(), cmd_run(), cmd_session_smoke(), cmd_start() (+6 more)

### Community 33 - "Community 33"
Cohesion: 0.23
Nodes (5): chaos_tester_force_kill(), ChaosError, ChaosTester, managed_process_spawn_and_kill(), ManagedProcess

### Community 34 - "Community 34"
Cohesion: 0.24
Nodes (4): EvalFixture, FixtureStore, store_add_and_get(), store_by_category()

### Community 35 - "Community 35"
Cohesion: 0.24
Nodes (8): Skill, parse(), parse_frontmatter(), parse_metadata_fields(), parse_simple_markdown(), parse_simple_markdown_heading(), parse_standard_frontmatter(), ParseError

### Community 36 - "Community 36"
Cohesion: 0.18
Nodes (9): Grade, IsolationTier, RubricScore, ArtifactRef, DariusError, Decision, SessionHandoff, SubagentId (+1 more)

### Community 37 - "Community 37"
Cohesion: 0.27
Nodes (11): build_skill_preface(), build_skill_preface_default(), build_skill_preface_respects_cap(), find_matching_skills(), find_matching_skills_by_keyword(), find_matching_skills_no_match(), load_skills_from_dir(), load_skills_from_dir_loads_md_files() (+3 more)

### Community 38 - "Community 38"
Cohesion: 0.18
Nodes (10): Build and test, code:sh (darius run --goal "..."          # full cognitive loop with ), code:sh (cargo fmt --all -- --check), code:sh (cargo run -p darius-cli -- session-smoke), code:sh (cargo test -p darius-rlm), Darius, Deferred, Lean cognitive phase status (+2 more)

### Community 39 - "Community 39"
Cohesion: 0.35
Nodes (6): AutoRater, rater_path_differs_from_optimizer(), rater_rejects_optimizer_identity(), rater_returns_grade(), rater_uses_rater_role(), RaterConfig

### Community 40 - "Community 40"
Cohesion: 0.29
Nodes (4): Worktree, worktree_manager_new(), WorktreeError, WorktreeManager

### Community 41 - "Community 41"
Cohesion: 0.27
Nodes (7): parse_empty_rubric(), parse_multiple_criteria(), parse_single_criterion(), Rubric, RubricCriterion, RubricDSL, RubricScore

### Community 42 - "Community 42"
Cohesion: 0.42
Nodes (4): project_running_daemon_with_sessions(), project_stopped_daemon(), StatusProjector, temp_data_dir()

### Community 43 - "Community 43"
Cohesion: 0.39
Nodes (5): Grade, grade_empty_rubric_passes(), grade_with_criteria(), Grader, score_criterion()

### Community 46 - "Community 46"
Cohesion: 0.40
Nodes (4): EventType, get_events_dir(), log_event(), SessionEvent

### Community 48 - "Community 48"
Cohesion: 0.50
Nodes (3): ApprovalTier, Capability, CapabilityScope

## Knowledge Gaps
- **133 isolated node(s):** `SessionEvent`, `EventType`, `SkillSource`, `CuratorMetrics`, `LspError` (+128 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **3 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `compute_anchor()` connect `Community 15` to `Community 31`, `Community 7`?**
  _High betweenness centrality (0.003) - this node is a cross-community bridge._
- **Why does `write_file()` connect `Community 31` to `Community 15`?**
  _High betweenness centrality (0.002) - this node is a cross-community bridge._
- **What connects `SessionEvent`, `EventType`, `SkillSource` to the rest of the system?**
  _133 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.05454545454545454 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.08973172987974098 - nodes in this community are weakly interconnected._
- **Should `Community 2` be split into smaller, more focused modules?**
  _Cohesion score 0.06086956521739131 - nodes in this community are weakly interconnected._
- **Should `Community 3` be split into smaller, more focused modules?**
  _Cohesion score 0.06401137980085349 - nodes in this community are weakly interconnected._