# Darius Fully Complete — v1.3.0 (Verified Against master @ b1f002a)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Skills:** writing-plans, tdd. Lean caps: MemoryPack ≤3500 chars, tool preview ≤32 KiB + spill, TaskBoard ≤15, ReAct ≤12 iters/task.

**Goal:** Ship **v1.3.0** where the **core product is fully complete and proven**: every registered model tool has an E2E recovery test; TUI permission allow-paths for write/shell work; `darius serve` runs a real CognitiveLoop with SSE; A2A card/tasks complete on localhost; install staged path works; CAPABILITIES matches reality.

**Architecture:** Do **not** rebuild what v1.2.1 already Verified. Close only real gaps found by reading `lib.rs`, `darius-web`, `run_e2e.rs`, `CAPABILITIES.md`, and tool registration source.

**Tech Stack:** Rust workspace **1.2.1** / tip `b1f002af297baf6d2eca179dc5323ac70ef8a2ce`; FakeProvider already in `tests/support/fake_provider.rs`; axum web crate present but **intentionally unavailable**.

## Global Constraints

- Branch: **`origin/master` only**.
- Truth: `docs/CAPABILITIES.md` + `scripts/audit-public-claims.sh`.
- Promote to Verified **only** with named tests in the same change.
- Noninteractive `run` continues to **deny** mutations/shell (already tested)—TUI is the approval surface.
- Serve binds **127.0.0.1** only by default.
- No API keys in git.
- TDD. Tag **v1.3.0** only when Definition of Done is met.

---

## 0. Verified audit (tested against source 2026-09-08)

### Already DONE — do not rebuild

| Surface | Evidence |
|---------|----------|
| CLI tokens only `tui`/`run`/`config`/`memory` | `crates/darius-cli/src/lib.rs` match arms |
| `read_file` agent E2E | `test_run_read_goal_succeeds` in `run_e2e.rs` |
| Noninteractive write denied, no file | `test_run_mutation_goal_denied_with_tui_guidance` |
| Noninteractive shell denied, no side effect | `binary_shell_denied_without_execution_or_prompt` |
| Memory CLI import/export/search/pack/stats + isolation | `test_memory_cli_lifecycle`, `binary_memory_roundtrip_and_profile_isolation` |
| Provider 401/429/500/503/invalid/timeout sanitized | `binary_*_is_*` tests in `run_e2e.rs` |
| Config init/show | `test_config_show_and_init` |
| Model picker + 13 slash handlers + PTY journeys | CAPABILITIES Verified rows |
| Tool **event** pairing in transcript | `tool_events_appear_in_session_transcript` |
| Web router **intentionally** unavailable | `darius-web/src/lib.rs`: empty capabilities, 503 fallback, no serve CLI |

### Real gaps for “fully complete”

| Gap | Why it matters |
|-----|----------------|
| `search_files` not in `run_e2e` FakeProvider tool path | Read works; search unproven via agent |
| `write_file` / `shell` **AllowOnce/AllowSession** only partially unit-tested; need TUI/runtime proof file/shell actually runs after approve | Approval UI exists; successful mutation path must be proven |
| `memory_remember` / `memory_search` / `task_*` via **agent tool calls** (not only CLI) | Tools registered; agent-loop E2E missing |
| `spill_read` after large tool output | Spill exists in registry; E2E thin |
| **No `serve` CLI** | Web crate is dead surface |
| A2A card empty; tasks never run | Must implement real loop hookup |
| Install staged-asset E2E | release.yml exists; local staged proof needed |
| Diff preview for writes in TUI | May be incomplete |

---

## File map

| Path | Change role |
|------|-------------|
| `crates/darius-cli/tests/run_e2e.rs` | More FakeProvider tool E2Es |
| `crates/darius-cli/tests/tui_pty.rs` | Approve write/shell journeys |
| `crates/darius-cli/src/args.rs` + `lib.rs` | Restore `serve` |
| `crates/darius-web/src/lib.rs` | Real dashboard, SSE, goal, A2A |
| `crates/darius-tools/**` | Fixes only if tests fail |
| `crates/darius-tui/**` | Diff + permission success path |
| `install.sh` + scripts | Staged install test |
| `docs/CAPABILITIES.md` | Promote with test names |

---

## Task 0: Preflight (must pass)

- [ ] **Step 1:** Sync

```bash
git fetch origin && git checkout master && git pull origin master
git rev-parse HEAD
git merge-base --is-ancestor b1f002af297baf6d2eca179dc5323ac70ef8a2ce HEAD && echo ANCESTOR_OK
```

- [ ] **Step 2:** Audit + quality

```bash
bash scripts/audit-public-claims.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 3:** Confirm known-good tests still exist

```bash
cargo test -p darius-cli test_run_read_goal_succeeds -- --nocapture
cargo test -p darius-cli test_run_mutation_goal_denied -- --nocapture
cargo test -p darius-cli binary_shell_denied -- --nocapture
cargo test -p darius-cli binary_401 -- --nocapture
```

- [ ] **Step 4:** Confirm serve is absent

```bash
cargo run -p darius-cli -- serve 2>&1 | head -5
# expect parse error / exit 2 — not a server start
```

- [ ] **Step 5:** If red → `fix: fully-complete preflight` only
- [ ] **Step 6:** Green only → continue

---

## Task 1: `search_files` agent E2E

**Files:** `crates/darius-cli/tests/run_e2e.rs`

- [ ] **Step 1:** Create failing test `test_run_search_files_goal_succeeds`
- [ ] **Step 2:** Workspace: `src/alpha.rs` containing unique token `SEARCH_TOKEN_Z9`
- [ ] **Step 3:** FakeProvider: tool_call `search_files` with query `SEARCH_TOKEN_Z9`, then text reply including path
- [ ] **Step 4:** Assert second request has role=tool content with path/token
- [ ] **Step 5:** Run — expect FAIL if wiring broken
- [ ] **Step 6:** Fix `search_files` / model schema / registration until PASS
- [ ] **Step 7:** Commit

```bash
git add crates/darius-cli/tests/run_e2e.rs crates/darius-tools
git commit -m "test: search_files agent E2E via FakeProvider"
```

---

## Task 2: `memory_remember` + `memory_search` agent E2E

- [ ] **Step 1:** Failing test `test_run_memory_tools_roundtrip`
- [ ] **Step 2:** FakeProvider sequence: `memory_remember` body=`AGENT_MEM_UNIQUE` → `memory_search` text=`AGENT_MEM_UNIQUE` → final text
- [ ] **Step 3:** Assert search tool result contains the body
- [ ] **Step 4:** Assert profile memory.db record_count ≥ 1 after run
- [ ] **Step 5:** PASS + commit `test: memory_remember/search agent E2E`

---

## Task 3: `task_add` / `task_list` / `task_complete` agent E2E

- [ ] **Step 1:** Failing test `test_run_task_board_tools`
- [ ] **Step 2:** FakeProvider: task_add title → task_list → task_complete id → text
- [ ] **Step 3:** Assert list preview shows title; complete succeeds
- [ ] **Step 4:** Commit `test: task board tools agent E2E`

---

## Task 4: `spill_read` E2E

- [ ] **Step 1:** Force a tool result > PREVIEW_CEILING (32KiB) via FakeProvider-driven tool or direct registry test
- [ ] **Step 2:** Assert outcome has `spilled_path: Some`
- [ ] **Step 3:** `spill_read` with that path returns full content marker
- [ ] **Step 4:** Commit `test: spill_read after large tool output`

---

## Task 5: TUI AllowOnce write creates file

**Files:** `tui_pty.rs` and/or `tui_runtime` tests

- [ ] **Step 1:** Failing PTY or controller test: mode Auto, model requests `write_file` path=`approved.txt` content=`ALLOWED_WRITE`
- [ ] **Step 2:** Simulate permission key for AllowOnce
- [ ] **Step 3:** Assert file exists on disk with content
- [ ] **Step 4:** Second identical write without session allow → prompt again (AllowOnce)
- [ ] **Step 5:** Commit `test: TUI AllowOnce write_file creates file`

---

## Task 6: TUI AllowSession + Deny + Plan

- [ ] **Step 1:** AllowSession → second write no re-prompt, both files exist
- [ ] **Step 2:** Deny → no file
- [ ] **Step 3:** Plan mode → write never executes even if user would approve
- [ ] **Step 4:** Commit `test: TUI write AllowSession Deny Plan`

---

## Task 7: TUI shell AllowOnce

- [ ] **Step 1:** Auto + shell `echo SHELL_OK_TOKEN` → approve
- [ ] **Step 2:** Tool preview/transcript contains `SHELL_OK_TOKEN`
- [ ] **Step 3:** Plan + shell → denied, no execution
- [ ] **Step 4:** Commit `test: TUI shell AllowOnce and Plan deny`

---

## Task 8: Write diff preview in TUI

- [ ] **Step 1:** After successful write, transcript includes DiffLineKind add/remove or unified hunk
- [ ] **Step 2:** Cap at N lines + truncated marker
- [ ] **Step 3:** Unit test on AppState apply_event
- [ ] **Step 4:** Commit `feat(tui): write_file diff preview`

---

## Task 9: Restore `darius serve` CLI

**Files:** `args.rs`, `lib.rs`, `cli_contract.rs`

- [ ] **Step 1:** Add `Command::Serve { host, port }` default host `127.0.0.1` port `7432`
- [ ] **Step 2:** Update `cli_contract` — `serve` no longer in removed_tokens exit-2 list
- [ ] **Step 3:** Failing test: `darius serve --port 0` starts (or test harness binds)
- [ ] **Step 4:** Implement `cmd_serve` calling web server builder
- [ ] **Step 5:** Commit `feat(cli): restore darius serve`

---

## Task 10: Serve runs real CognitiveLoop + SSE

**Files:** `darius-web`, runtime bridge

- [ ] **Step 1:** Replace unavailable dashboard HTML with working UI (goal form + event log)
- [ ] **Step 2:** `POST /api/goal` `{ "goal": "..." }` spawns loop on SessionRuntime (offline or configured)
- [ ] **Step 3:** `GET /api/events` SSE streams UiEvent JSON until Done/Error
- [ ] **Step 4:** E2E test with reqwest/eventsource against ephemeral port:
  - post goal under `--offline` or FakeProvider
  - receive at least one event and terminal Done
- [ ] **Step 5:** Fallback routes that must stay 503: none of the execution routes
- [ ] **Step 6:** Commit `feat(web): serve goal + SSE CognitiveLoop`

---

## Task 11: A2A card + tasks complete

- [ ] **Step 1:** `agent_card().capabilities` = `["cognitive_loop","memory_search","tool_execution","task_board"]` (truthful)
- [ ] **Step 2:** `POST /a2a/tasks` body goal → creates id, state Running→Completed|Failed
- [ ] **Step 3:** `GET /a2a/tasks/{id}` returns state + output summary
- [ ] **Step 4:** E2E test full cycle
- [ ] **Step 5:** Peer routes may remain 503 unless separately specified
- [ ] **Step 6:** Commit `feat(a2a): card and task completion`

---

## Task 12: CAPABILITIES promote serve/tools

- [ ] **Step 1:** Move serve/A2A/tool rows from Unavailable → Verified with **exact test function names**
- [ ] **Step 2:** Update `scripts/audit-public-claims.sh` expectations for `serve` token
- [ ] **Step 3:** Run audit — PASS
- [ ] **Step 4:** Commit `docs: CAPABILITIES v1.3 verified surfaces`

---

## Task 13: Install staged asset E2E

- [ ] **Step 1:** Script packs `target/release/darius` into `darius-<os>-<arch>.tar.gz` + sha256
- [ ] **Step 2:** Point `install.sh` at file:// or local path mode for test
- [ ] **Step 3:** Install to temp prefix; `darius --version` matches Cargo version
- [ ] **Step 4:** Commit `test: install.sh staged tarball E2E`

---

## Task 14: README complete quickstart

- [ ] **Step 1:** Document: build → config init → tui → /model → approve write → serve curl examples
- [ ] **Step 2:** Keep offline demo section explicit
- [ ] **Step 3:** Remove any leftover “unavailable” wording for serve/A2A if now true
- [ ] **Step 4:** Commit `docs: README fully complete quickstart`

---

## Task 15: Release v1.3.0

- [ ] **Step 1:** CHANGELOG 1.3.0 entry listing tool E2Es, TUI allow paths, serve, A2A, install
- [ ] **Step 2:** Bump workspace version `1.3.0`
- [ ] **Step 3:** Final gate

```bash
bash scripts/audit-public-claims.sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p darius-cli
```

- [ ] **Step 4:** Tag + push

```bash
git add -A
git commit -m "release: v1.3.0 fully complete core product"
git tag -a v1.3.0 -m "v1.3.0: tool E2Es, TUI approvals, serve+A2A, install"
git push origin master --tags
```

---

## Definition of done

- [ ] `search_files`, memory tools, task tools, `spill_read` have FakeProvider/agent tests
- [ ] TUI AllowOnce write creates file; AllowSession/Deny/Plan proven
- [ ] TUI shell AllowOnce proven; Plan denies shell
- [ ] `darius serve` is a real CLI command
- [ ] POST goal + SSE Done works in automated test
- [ ] A2A card non-empty; task reaches Completed/Failed in test
- [ ] Staged install test passes
- [ ] CAPABILITIES/audit green and honest
- [ ] **v1.3.0** tagged

## Explicitly not required for v1.3.0 “complete”

- Hosted OpenAI/Anthropic SLA claims
- MCP multi-server product UI
- Peer A2A messaging fleet
- Firecracker / gVisor
- Windows official binaries
- Native Anthropic API

---

## Agent kickoff

```
Implement docs/superpowers/plans/2026-09-08-darius-fully-complete.md on master @ b1f002a+.
This plan was rewritten after reading lib.rs, darius-web, run_e2e, CAPABILITIES.
DO NOT rebuild read_file E2E, mutation denial, provider error UX, memory CLI, or model picker — already Verified.
DO implement: search/memory/task/spill agent E2Es, TUI allow write/shell, serve+SSE+A2A, install staged, CAPABILITIES.
TDD. Tag v1.3.0 only when Definition of Done is complete.
```

---

**Verification note:** Audit confirmed `serve` is absent from CLI, web is a 503 compatibility stub, `read_file` + denial paths + provider errors already tested. This plan targets only remaining gaps.

**Path:** `docs/superpowers/plans/2026-09-08-darius-fully-complete.md`

## Source-grounded execution contract

This addendum resolves imprecise steps above without broadening the product or weakening its existing policy. It takes precedence over conflicting implementation details.

- Preflight passed at `b1f002a`: audit, formatting, clippy, 627 tests passed and one ignored. Known-good functions were included; the reported absent serve command was not redundantly launched.
- Tasks 1–4 use the actual registered argument schemas. Task 2 includes `memory_pack`, so all eleven model tools have named agent evidence. Memory/task mutations run with FakeProvider through the production TUI worker's permission channel, not an auto-approved noninteractive `run`.
- Spill recovery must be model-usable: include the generated spill path in bounded model-facing tool content, then issue `spill_read` using that observed path and recover a marker beyond the original preview. A direct registry test alone is insufficient.
- Tasks 5–6 reuse `blocked_control_permission_resolves_actual_write`, `permission_lifecycle_once_does_not_persist_and_denial_is_correlated`, `permission_lifecycle_session_normalizes_path_across_turns_but_not_targets`, `execution_policy_plan_denies_before_permission_and_readonly_runs`, and `full_agent_journey`. AllowSession authorizes one canonical target: repeated writes to that target skip the prompt; a different target still prompts. Do not introduce blanket write grants.
- Task 7 adds only missing shell-output evidence; reuse existing Plan-denial proof. Task 8 extends the existing diff producer/renderer with bounded lines/bytes and an explicit truncation marker, not another diff implementation.
- One integration owner implements Tasks 9–11. The web crate accepts an injected runner; CLI supplies a real `SessionRuntime`/`AgentLoop` implementation without a dependency cycle. Serialize execution or isolate runtime state per task. Web has no approval surface and denies all mutation/shell requests.
- `POST /api/goal` and `POST /a2a/tasks` return a task id. `GET /api/events?task_id=<id>` replays bounded task-specific events before live delivery. Bound active work and retained history; reject overload rather than silently losing work. Test late subscription and task isolation.
- Persist Completed/Failed state and output before publishing exactly one terminal Done. Error/Interrupted precedes Done; Error does not prematurely close SSE. Test failure terminal ordering and immediate task status after Done.
- Bind loopback by default; reject non-loopback exposure rather than expose an unauthenticated runner. Reject foreign browser origins/hosts before execution. Tests use isolated homes and local FakeProvider, never hosted credentials.
- Serve/A2A proof must execute a configured FakeProvider read tool, verify the correlated result and terminal SSE, and prove write/shell denial without side effects. Offline mode remains explicitly labelled demonstration, not evidence of real analysis. Dashboard is a local, dependency-free goal form and event log with safe text rendering.
- Main owns the claims cutover in `scripts/claims_runtime.py`, `scripts/claims_policy.py`, `tests/generated_claims_test.py`, README, CAPABILITIES, TROUBLESHOOTING, and CHANGELOG. Web owner updates web crate metadata, web public contracts, CLI contract, and `tests/harness_e2e/src/lib.rs` obsolete web assertions. Peer/fleet/MCP claims remain unavailable.
- Task 13 builds or accepts the actual release binary before packaging, uses existing `scripts/release-target.sh` names/checksums and unchanged `install.sh --artifact-dir`, installs into a temporary prefix, and compares installed version with Cargo metadata. No new installer transport.
- Final quality gates and real CLI/TUI/web/install smoke evidence precede the release tag. Push only `master` and the exact `v1.3.0` tag, never all local tags. A successful push/tag is not proof of published platform artifacts; that requires successful release workflow evidence.
