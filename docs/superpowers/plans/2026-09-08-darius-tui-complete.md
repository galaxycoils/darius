# Darius TUI Complete — Every Command, Option & Easy Model Selection

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Skills:** writing-plans, tdd. Keep lean caps: MemoryPack ≤3500 chars, tool preview ≤32 KiB + spill, TaskBoard ≤15, ReAct ≤12 iters/task.

**Goal:** Make the **Darius TUI fully usable end-to-end**: every advertised slash command does real work, modes/permissions behave correctly, **model selection is interactive and easy** (picker + persist to profile), and agent turns with tools are proven through PTY recovery tests—not just unit stubs.

**Architecture:** Build on the honest **v1.2.0 recovery surface** (`docs/CAPABILITIES.md`). Do not re-advertise retired features (serve/A2A/cron/MCP/subagents) until they have recovery tests. Deepen `darius-tui` controller + `darius-cli` runtime + profile config so `/model` becomes a **writable picker**, config can be edited from the TUI, and every of the 13 canonical commands has a controller handler with tests.

**Tech Stack:** Rust workspace on `master` (version **1.2.0**, tip `9ead168+`); ratatui; `darius-core` command registry; `darius-cli` runtime; SQLite memory; MockModel + OpenAI-compatible client.

## Global Constraints

- Branch: **`origin/master` only**.
- Truth source: `docs/CAPABILITIES.md` — promote items to **Verified** only with executable recovery/PTY tests.
- Do not restore `serve` / `a2a` / `cron` / `mcp` / `subagent` / `/effort` / `/skills` in this plan unless a task explicitly re-verifies them (default: **out of scope**).
- Offline Mock path must work with zero keys; live path optional behind env.
- No API keys in git; profile config under `~/.darius/profiles/<name>/config.toml`.
- TDD: failing test → implement → pass → commit.
- Target release: **v1.2.1** (TUI complete) or **v1.3.0** if scope grows.

---

## 0. Current status (2026-09-08 audit)

| Item | State |
|------|--------|
| Tip | `9ead1689ee7ad59471fe7c28f879c6c3b2123ff0` |
| Version | **1.2.0** tagged |
| Public CLI | **Only** `tui`, `run`, `config`, `memory` (verified) |
| Slash registry | **13** commands in `darius-core` |
| `/model` | **Read-only** today (`accepts_args: false`, description says read-only) |
| Modes | **Auto** + **Plan** verified; Manual / AcceptEdits / effort **hidden** |
| Tools | Registered in source; **not** recovery-proven through public TUI session |
| Live provider | Config shape exists; CAPABILITIES marks live proof **Unavailable** |
| Honesty | Prior overclaims (MCP, A2A hub, cron, subagents) **retired** from public surface |

### What “TUI complete” means for this plan

1. **Every one of the 13 slash commands** has a real handler that mutates state or returns truthful output (no “not implemented” stubs).
2. **Easy model selection:** interactive picker (`/model` opens list → filter → select → write profile → next turn uses it).
3. **Modes & permissions:** Auto vs Plan fully enforced; permission chooser AllowOnce / AllowSession / Deny works under PTY.
4. **Agent journey:** type a goal → Mock or live model → tools may run → transcript + tasks update → `/stop` / `/quit` clean.
5. **Config from TUI:** `/config` shows effective config; optional set of model/base_url/api_key_env without leaving TUI.
6. **CAPABILITIES.md updated** so Verified rows match reality.

---

## File map

| Path | Responsibility |
|------|----------------|
| `crates/darius-core/src/commands.rs` | Registry: make `/model` accept args |
| `crates/darius-core/src/config.rs` (or cli profile module) | ModelConfig load/save + catalog |
| `crates/darius-tui/src/app.rs` | AppState: model picker, permission, mode |
| `crates/darius-tui/src/controller.rs` | Dispatch every CommandId |
| `crates/darius-tui/src/render.rs` | Picker overlay, status, permission modal |
| `crates/darius-tui/src/input.rs` | Picker keybindings |
| `crates/darius-cli/src/runtime.rs` | Session runtime: bind model client |
| `crates/darius-cli/tests/tui_pty.rs` | PTY recovery journeys |
| `docs/CAPABILITIES.md` | Promote Verified after tests |
| `README.md` | Model picker UX, accurate command table |

---

## Task 0: Preflight

- [ ] **Step 1:** Fetch and checkout master

```bash
git fetch origin && git checkout master && git pull origin master
```

- [ ] **Step 2:** Confirm tip is at or after `9ead168`

```bash
git rev-parse HEAD
git merge-base --is-ancestor 9ead1689ee7ad59471fe7c28f879c6c3b2123ff0 HEAD && echo OK
git tag -l 'v1.2*'
```

- [ ] **Step 3:** Format check

```bash
cargo fmt --all -- --check
```

- [ ] **Step 4:** Clippy

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 5:** Full test suite

```bash
cargo test --workspace
```

- [ ] **Step 6:** CLI help surface

```bash
cargo run -p darius-cli -- --help
cargo run -p darius-cli -- tui --help 2>/dev/null || true
```

- [ ] **Step 7:** Read inventory files (no edits yet)

```bash
# skim only
sed -n '1,80p' docs/CAPABILITIES.md
sed -n '1,120p' crates/darius-core/src/commands.rs
```

- [ ] **Step 8:** If any step 3–5 failed, fix minimally and commit `fix: tui-complete preflight`
- [ ] **Step 9:** Stop here if still red — do not start Task 1 until green

---

## Task 1: Inventory controller handlers (gap list)

**Files:** `crates/darius-tui/src/controller.rs`, `crates/darius-tui/src/app.rs`

- [ ] **Step 1:** Open `controller.rs` and list every `CommandId` match arm
- [ ] **Step 2:** Mark each as `real` / `stub` / `missing` in a short comment or scratch note
- [ ] **Step 3:** Write failing exhaustiveness test in `controller.rs` (or `tests` module):

```rust
#[test]
fn every_command_id_has_handler() {
    for spec in darius_core::commands::COMMANDS {
        assert!(
            crate::controller::handles(spec.id),
            "missing handler for {}",
            spec.name
        );
    }
}
```

- [ ] **Step 4:** Run the test — expect FAIL if `handles` missing

```bash
cargo test -p darius-tui every_command_id_has_handler -- --nocapture
```

- [ ] **Step 5:** Add `pub fn handles(id: CommandId) -> bool` that matches all 13 ids
- [ ] **Step 6:** Re-run test — expect PASS
- [ ] **Step 7:** Commit

```bash
git add crates/darius-tui
git commit -m "test(tui): exhaustiveness for slash command handlers"
```

---

## Task 2: Model config load/save + catalog

**Files:** prefer `crates/darius-core/src/config.rs` or `crates/darius-cli/src/config.rs` (follow existing layout)

**Interfaces to implement:**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    pub model: &'static str,
    pub base_url: &'static str,
}

pub fn default_model_catalog() -> Vec<ModelCatalogEntry>;
pub fn load_model_config(profile_dir: &Path) -> Result<ModelConfig, ConfigError>;
pub fn save_model_config(profile_dir: &Path, cfg: &ModelConfig) -> Result<(), ConfigError>;
pub fn catalog_entry_to_config(entry: &ModelCatalogEntry) -> ModelConfig;
```

- [ ] **Step 1:** Create module file if missing; export from lib
- [ ] **Step 2:** Write failing round-trip test with `tempfile::tempdir`

```rust
#[test]
fn model_config_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ModelConfig {
        provider: "openai_compatible".into(),
        base_url: "https://api.openai.com/v1".into(),
        model: "gpt-4o-mini".into(),
        api_key_env: "DARIUS_API_KEY".into(),
    };
    save_model_config(dir.path(), &cfg).unwrap();
    let loaded = load_model_config(dir.path()).unwrap();
    assert_eq!(loaded, cfg);
}
```

- [ ] **Step 3:** Run test — expect FAIL

```bash
cargo test -p darius-core model_config_round_trip -- --nocapture
# or -p darius-cli if module lives there
```

- [ ] **Step 4:** Implement `save_model_config` (write `config.toml` under profile dir, create dirs)
- [ ] **Step 5:** Implement `load_model_config` (defaults if missing file)
- [ ] **Step 6:** Re-run round-trip — PASS
- [ ] **Step 7:** Write failing test `catalog_contains_mock_and_gpt4o_mini`
- [ ] **Step 8:** Implement `default_model_catalog` with at least: mock, gpt-4o-mini, gpt-4o, and one custom-friendly placeholder
- [ ] **Step 9:** Implement `catalog_entry_to_config`
- [ ] **Step 10:** Run all config tests — PASS
- [ ] **Step 11:** Commit

```bash
git add crates/darius-core crates/darius-cli
git commit -m "feat(config): model config load/save and catalog"
```

---

## Task 3: Registry — `/model` accepts arguments

**Files:** `crates/darius-core/src/commands.rs`

- [ ] **Step 1:** Change CommandSpec for Model:

```rust
CommandSpec {
    id: CommandId::Model,
    name: "/model",
    description: "Show or select provider/model",
    accepts_args: true,
},
```

- [ ] **Step 2:** Update `parse_invocation` if it special-cases model args (allow free text)
- [ ] **Step 3:** Update unit test `command_registry_exact_surface` if it assumed read-only description
- [ ] **Step 4:** Add test:

```rust
#[test]
fn model_accepts_args() {
    let inv = parse_invocation("/model gpt-4o-mini").unwrap();
    assert_eq!(inv.id, CommandId::Model);
    assert_eq!(inv.args, "gpt-4o-mini");
}
```

- [ ] **Step 5:**

```bash
cargo test -p darius-core model_accepts_args command_registry -- --nocapture
```

- [ ] **Step 6:** Commit

```bash
git add crates/darius-core
git commit -m "feat(core): /model accepts args for selection"
```

---

## Task 4: Model picker state + open on `/model`

**Files:** `crates/darius-tui/src/app.rs`, `controller.rs`

- [ ] **Step 1:** Add to AppState:

```rust
pub struct ModelPickerState {
    pub filter: String,
    pub cursor: usize,
    pub entries: Vec<ModelCatalogEntry>, // or owned clones
}

pub struct AppState {
    // existing fields...
    pub model_picker: Option<ModelPickerState>,
    pub active_model: ModelConfig,
}
```

- [ ] **Step 2:** Write failing test `model_command_opens_picker_when_no_args`
- [ ] **Step 3:** Run — FAIL
- [ ] **Step 4:** In controller, on `CommandId::Model` with empty args → set `model_picker = Some(...)`
- [ ] **Step 5:** Run — PASS
- [ ] **Step 6:** Write failing test `model_command_with_unique_arg_selects_without_picker`
- [ ] **Step 7:** Implement unique-prefix match against catalog → apply config, no picker
- [ ] **Step 8:** Ambiguous/unknown arg → open picker with filter pre-filled
- [ ] **Step 9:** Run tests — PASS
- [ ] **Step 10:** Commit

```bash
git add crates/darius-tui
git commit -m "feat(tui): open model picker from /model"
```

---

## Task 5: Render model picker + keyboard

**Files:** `render.rs`, `input.rs`

- [ ] **Step 1:** Write a render unit test or snapshot assert that picker block title contains “model”
- [ ] **Step 2:** Implement overlay: filtered list, cursor highlight, filter line (copper accent)
- [ ] **Step 3:** Write failing input test: Down increases cursor; Enter selects; Esc clears picker
- [ ] **Step 4:** Implement key handling when `model_picker.is_some()` (do not send to composer)
- [ ] **Step 5:** On Enter: `save_model_config`, update `active_model`, push system transcript line, close picker
- [ ] **Step 6:** On Esc: close picker, no config write
- [ ] **Step 7:**

```bash
cargo test -p darius-tui -- --nocapture
```

- [ ] **Step 8:** Commit

```bash
git add crates/darius-tui
git commit -m "feat(tui): model picker render and keyboard"
```

---

## Task 6: Runtime rebuilds client after model change

**Files:** `crates/darius-cli/src/runtime.rs`, tui controller bridge

- [ ] **Step 1:** Write failing test: after applying mock catalog entry, runtime `model_id()` is mock
- [ ] **Step 2:** Write failing test: after applying openai entry without key, runtime stays mock and surfaces warning string
- [ ] **Step 3:** Implement client factory: mock if provider==mock OR missing env key; else OpenAI-compatible
- [ ] **Step 4:** Ensure TUI session holds a shared handle that can swap client
- [ ] **Step 5:** Run tests — PASS
- [ ] **Step 6:** Commit

```bash
git add crates/darius-cli crates/darius-tui
git commit -m "feat(runtime): rebuild model client after /model select"
```

---

## Task 7: Complete slash handlers (batch A — session chrome)

### 7.1 `/help`

- [ ] **Step 1:** Failing test: dispatch Help → transcript contains `/model` and `/quit`
- [ ] **Step 2:** Implement: append static help from `COMMANDS` + key table
- [ ] **Step 3:** PASS → continue

### 7.2 `/clear`

- [ ] **Step 1:** Failing test: transcript len ≤ 1 after clear
- [ ] **Step 2:** Implement clear (keep optional welcome line)
- [ ] **Step 3:** PASS

### 7.3 `/status`

- [ ] **Step 1:** Failing test: status text includes mode and model name
- [ ] **Step 2:** Implement from AppState + memory stats if available
- [ ] **Step 3:** PASS

### 7.4 `/config`

- [ ] **Step 1:** Failing test: shows provider and model; never prints raw API key values
- [ ] **Step 2:** Implement redacted display
- [ ] **Step 3:** PASS

### 7.5 `/quit` and `/stop`

- [ ] **Step 1:** Failing test: Quit sets exit flag
- [ ] **Step 2:** Failing test: Stop clears `turn_active` without exit
- [ ] **Step 3:** Implement both
- [ ] **Step 4:** PASS

- [ ] **Step 5:** Commit batch A

```bash
git add crates/darius-tui
git commit -m "feat(tui): help clear status config stop quit handlers"
```

---

## Task 8: Complete slash handlers (batch B — memory & tasks)

### 8.1 `/memory [query]`

- [ ] **Step 1:** Create temp profile with memory.db; insert one record in test harness
- [ ] **Step 2:** Failing test: `/memory <token>` appends hit lines to transcript
- [ ] **Step 3:** Failing test: `/memory` with no args appends stats (count + path)
- [ ] **Step 4:** Implement via memory engine search/stats APIs
- [ ] **Step 5:** PASS

### 8.2 `/pack`

- [ ] **Step 1:** Failing test: pack output char len ≤ 3500
- [ ] **Step 2:** Implement MemoryPack build + show in transcript
- [ ] **Step 3:** PASS

### 8.3 `/tasks`

- [ ] **Step 1:** Failing test: empty board → truthful “no tasks” message
- [ ] **Step 2:** Failing test: after injecting TaskBoard event, `/tasks` lists titles
- [ ] **Step 3:** Implement
- [ ] **Step 4:** PASS

- [ ] **Step 5:** Commit batch B

```bash
git add crates/darius-tui crates/darius-cli
git commit -m "feat(tui): memory pack tasks handlers"
```

---

## Task 9: Complete slash handlers (batch C — mode, permissions, compact)

### 9.1 `/mode`

- [ ] **Step 1:** Failing test: `/mode plan` sets Mode::Plan
- [ ] **Step 2:** Failing test: `/mode auto` sets Mode::Auto
- [ ] **Step 3:** Failing test: `/mode weird` returns error message, mode unchanged
- [ ] **Step 4:** Failing test: Shift+Tab cycles auto ↔ plan only
- [ ] **Step 5:** Implement
- [ ] **Step 6:** PASS

### 9.2 `/permissions`

- [ ] **Step 1:** Failing test: in Plan mode, permissions text mentions deny/mutating
- [ ] **Step 2:** Implement show-only policy summary (truthful)
- [ ] **Step 3:** PASS

### 9.3 `/compact`

- [ ] **Step 1:** Seed AppState with 50 large transcript items
- [ ] **Step 2:** Failing test: after compact, context char estimate drops OR middle replaced with marker
- [ ] **Step 3:** Implement lean-tail keep head+tail (pure function OK)
- [ ] **Step 4:** Wire `/compact` to call it and report “compacted”
- [ ] **Step 5:** PASS

- [ ] **Step 6:** Commit batch C

```bash
git add crates/darius-tui crates/darius-cognitive
git commit -m "feat(tui): mode permissions compact handlers"
```

---

## Task 10: Handler exhaustiveness green

- [ ] **Step 1:** Re-run `every_command_id_has_handler` — must PASS
- [ ] **Step 2:** Manually confirm no CommandId arm is `todo!()` or empty stub returning “not implemented”
- [ ] **Step 3:** Commit if any last stubs fixed: `fix(tui): remove remaining command stubs`

---

## Task 11: Permission modal E2E

**Files:** `app.rs`, `controller.rs`, `render.rs`, runtime tool gate

- [ ] **Step 1:** Write failing unit test: tool with risk Write in Auto mode without session allow → `PermissionRequest` queued
- [ ] **Step 2:** Write failing test: choosing Deny does not execute tool
- [ ] **Step 3:** Write failing test: AllowOnce executes once; second call prompts again
- [ ] **Step 4:** Write failing test: AllowSession executes subsequent same tool without prompt
- [ ] **Step 5:** Implement modal state + render (Allow once / Allow session / Deny)
- [ ] **Step 6:** Wire input keys 1/2/3 or labeled shortcuts
- [ ] **Step 7:** Plan mode: mutating tools hard-deny without optional prompt (document)
- [ ] **Step 8:**

```bash
cargo test -p darius-tui permission -- --nocapture
```

- [ ] **Step 9:** Commit

```bash
git add crates/darius-tui crates/darius-cli crates/darius-safety
git commit -m "feat(tui): permission chooser AllowOnce AllowSession Deny"
```

---

## Task 12: PTY mock agent journey

**Files:** `crates/darius-cli/tests/tui_pty.rs`

- [ ] **Step 1:** Write failing PTY test `full_agent_journey_mock`:

  1. temp HOME
  2. spawn `darius tui`
  3. send goal + Enter
  4. wait for assistant text
  5. send `/status` + Enter — expect model/mode lines
  6. send `/quit` + Enter
  7. exit code 0; terminal restored

- [ ] **Step 2:** Run test — FAIL
- [ ] **Step 3:** Fix runtime/TUI event loop until PASS
- [ ] **Step 4:** Write PTY test `model_picker_then_run_mock`:

  1. `/model` → select mock via keys
  2. send short goal
  3. expect completion

- [ ] **Step 5:** PASS both
- [ ] **Step 6:** Commit

```bash
git add crates/darius-cli/tests
git commit -m "test(tui): PTY mock agent journey with model picker"
```

---

## Task 13: Live model safe fallback

- [ ] **Step 1:** Unit test: config openai + missing env → client is Mock + warning
- [ ] **Step 2:** Unit test: config openai + env set → client is live adapter (mock HTTP with wiremock if available)
- [ ] **Step 3:** Implement without panics
- [ ] **Step 4:** Add `#[ignore]` test `live_key_smoke` documented in README
- [ ] **Step 5:** Commit

```bash
git add crates/darius-cli
git commit -m "feat(runtime): live openai-compatible client with mock fallback"
```

---

## Task 14: Tool events in transcript (recovery proof)

- [ ] **Step 1:** Script MockModel (or loop) to emit one tool call `memory_stats` / `task_list`
- [ ] **Step 2:** Failing test: transcript contains tool start/end markers
- [ ] **Step 3:** Implement UI mapping from UiEvent::ToolStart/ToolEnd if missing
- [ ] **Step 4:** PASS
- [ ] **Step 5:** Update CAPABILITIES.md for those tools if public path proven
- [ ] **Step 6:** Commit

```bash
git add crates/darius-tui crates/darius-cli docs/CAPABILITIES.md
git commit -m "test(tui): tool events appear in session transcript"
```

---

## Task 15: UX polish (header, footer, arg hints)

- [ ] **Step 1:** Header always `darius · {profile} · {model} · {mode}`
- [ ] **Step 2:** Footer `Enter send · / commands · Shift+Tab mode · q quit`
- [ ] **Step 3:** If command accepts_args and args empty (e.g. `/memory`), show input hint line
- [ ] **Step 4:** Unit test: header string contains active model name after select
- [ ] **Step 5:** Commit

```bash
git add crates/darius-tui
git commit -m "feat(tui): header footer and arg hints polish"
```

---

## Task 16: Docs, CAPABILITIES, release v1.2.1

- [ ] **Step 1:** Update README `/model` section with picker steps
- [ ] **Step 2:** Sync slash command table to 13 real behaviors
- [ ] **Step 3:** Update `docs/CAPABILITIES.md` Verified rows (model picker, handlers, PTY)
- [ ] **Step 4:** Write CHANGELOG entry **1.2.1**
- [ ] **Step 5:** Bump workspace version to `1.2.1` if not already
- [ ] **Step 6:** Final gate

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p darius-cli
```

- [ ] **Step 7:** Tag and push

```bash
git add README.md docs CHANGELOG.md Cargo.toml crates/*/Cargo.toml
git commit -m "docs: v1.2.1 TUI complete — model picker and full slash handlers"
git tag -a v1.2.1 -m "v1.2.1: TUI complete — model picker, all slash handlers, PTY journeys"
git push origin master --tags
```

---

## Definition of done

- [ ] `/model` opens picker, filters, selects, **persists**, next turn uses selection
- [ ] All **13** slash commands have non-stub handlers + tests
- [ ] Auto / Plan enforced; permission modal works
- [ ] PTY mock agent journey green
- [ ] Missing API key does not crash; falls back with clear message
- [ ] CAPABILITIES + README match behavior
- [ ] **v1.2.1** tagged

## Out of scope (this plan)

- Restoring `darius serve` / A2A / cron / MCP / subagents without new recovery tests
- Desktop GUI / Bot Mode
- Native Anthropic API (OpenAI-compatible only)
- `/effort` multi-level selector (unless added with tests in a follow-up)

---

## Agent kickoff (copy-paste)

```
Implement docs/superpowers/plans/2026-09-08-darius-tui-complete.md on master @ 9ead168+.
Priority: interactive /model picker + persist, then every one of the 13 slash handlers real, then PTY journeys.
Respect docs/CAPABILITIES.md honesty — promote to Verified only with tests.
Do not re-enable serve/a2a/cron/mcp in this pass.
Follow every Step checkbox in order. TDD. Tag v1.2.1 when definition of done is met.
```

---

**Plan complete (writing-plans skill).**  
**Path:** `docs/superpowers/plans/2026-09-08-darius-tui-complete.md`
