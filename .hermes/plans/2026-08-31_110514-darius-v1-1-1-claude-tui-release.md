# Darius v1.1.1 Claude-Code-Style TUI Completion Plan

> **For Hermes:** Execute with `software-development:subagent-driven-development`; use `software-development:test-driven-development` for every code task and `software-development:requesting-code-review` before release. Keep implementation/status replies in `/caveman` + `/ponytail` style.

## Goal

Make `darius tui` a production-usable, Darius-native recreation of Claude Code’s terminal interaction grammar—including composer, slash/dash command palette, modes, effort, streaming turns, tools, diffs, todos, and permission prompts—then prove the full harness, tag `v1.1.1`, push it, and publish verified release assets.

## Current context / assumptions

- Repository: `/Users/cmd/workspace/Darius`; branch `master`; inspected HEAD `b75b051ed9a7fb1bcd500c055a8d83f2cd9e96e6`; tag `v1.1.0` points at HEAD; working tree was clean at planning time.
- Workspace version is `1.1.0` in `Cargo.toml`; target release is `1.1.1`.
- Current `crates/darius-tui/src/lib.rs` is a bordered four-panel demo, only accepts text while `slash_mode` is active, hard-codes profile/model, and does not subscribe to `darius_cognitive::UiEvent`.
- Current `crates/darius-cli/src/lib.rs::cmd_tui()` only calls `darius_tui::run_tui()`; it does not build memory/tools/model/CognitiveLoop dependencies. `cmd_serve()` only prints URLs.
- `crates/darius-web/src/lib.rs` duplicates `UiEvent` instead of importing the canonical event type and emits synthetic events rather than running the cognitive loop.
- `crates/darius-cognitive/src/lib.rs` contains both an event-emitting `CognitiveLoop::run` and a duplicate legacy `run_loop`; this must be collapsed to one implementation.
- `ModelRouter::route` and `IpKernelConnection` contain stub behavior. No release claim may call those production/live until real I/O is implemented and tested.
- Brainless is **UX inspiration only**. Canonical references inspected: `llms.txt`, `claude-session`, `claude-header`, `claude-message`, `claude-thinking`, `claude-tool-call`, `claude-todo-list`, `claude-diff`, `claude-permission`, `claude-slash-menu`, and `claude-prompt`. Do not vendor or translate its React source line-for-line.
- “Dash commands” is interpreted as Claude-style slash commands, with a Darius convenience alias: the command parser accepts both `/command` and `-command`; help and docs present `/command` as canonical.
- Lean caps remain invariants: MemoryPack ≤ 3500 chars, tool preview ≤ 32 KiB + spill, TaskBoard ≤ 15, ReAct ≤ 12 iterations/task.
- Default TUI remains local/offline-capable. Mock is explicit (`mock` badge), not silently presented as a live model.
- Default server bind remains `127.0.0.1`; public/LAN bind requires an explicit flag and warning.

## Architecture / proposed approach

Create one canonical interaction spine: `darius-cognitive` owns `UiEvent` and the session runner; `darius-tui` owns a pure reducer (`UiEvent`/key input → `AppState`/`Action`) plus rendering; `darius-cli` constructs runtime dependencies and drives async work on a background Tokio runtime. Rebuild the TUI as a single flowing Claude-like terminal canvas—welcome card, turn stream, tool disclosures/diffs/todos, contextual permission chooser, slash palette, and pinned dual-rule composer—while retaining a small Darius copper sigil as the product signature.

Before release, close truth gaps that affect the TUI: real OpenAI-compatible provider HTTP, real `darius serve`, canonical shared events, and honest feature/status labels. Optional IPyKernel stays feature-gated; either its execute path passes an integration smoke test or it is labeled experimental and excluded from “production-ready” claims.

## Design contract

### Visual grammar

- No dashboard-style bordered grid. The main screen is a single vertical terminal transcript with tight `1.5–1.6` line rhythm.
- Transparent/default terminal background; truecolor where supported, ANSI fallback otherwise.
- Palette:
  - primary text `#c0caf5`
  - muted `#949494`
  - active slash row `#afd7ff`
  - prompt rule `#808080`
  - auto mode `#ffd700`
  - accept-edits `#afafd7`
  - plan mode `#5fafaf`
  - permission rose `#cd694a`
  - add `#9ece6a`, delete `#f7768e`
  - Darius-only brand accent `#e8a54b` used only for the `◆ darius` mark and release badge
- Signature element: `◆ darius` launch legend plus one copper context indicator; all operational UI follows the quieter Claude terminal grammar.

### Interaction contract

- User message: `❯ text`; assistant text has no role chip.
- Thinking: pulsing `✦` + rotating verb + elapsed time + `esc to interrupt`.
- Tool: `⏺ Tool(arg)` followed by indented `⎿ result`; Enter toggles expanded output.
- Todos: `Update Todos` with `✓` done, `◐` active, `○` pending; completed labels are dim/struck where terminal support allows.
- Diff: filename + summary, then line-numbered context/add/delete rows.
- Permission: rose box with arrow-key options: `Yes`, `Yes, and don’t ask again this session`, `No, and tell Darius what to do (esc)`.
- Composer: two horizontal rules, `❯` input, effort chip above, mode/hints below.
- Shift+Tab cycles `auto → manual → accept-edits → plan`.
- `/` opens a filtered palette above composer; `-` at column zero opens the same palette; Up/Down moves, Tab/Enter accepts, Esc closes.
- Canonical command list: `/help`, `/clear`, `/compact`, `/model`, `/mode`, `/effort`, `/permissions`, `/memory`, `/pack`, `/tasks`, `/plan`, `/status`, `/config`, `/skills`, `/a2a`, `/serve`, `/stop`, `/quit`. Every visible command must execute or return an explicit “not available in this build” result; no dead menu rows.

---

# Work Unit 0 — Baseline, release truth, and test harness

## Task 0.1: Freeze the exact baseline

**Objective:** Record the starting branch/tag/remote state before any v1.1.1 work.

**Files:** none.

1. Run:
   ```bash
   cd /Users/cmd/workspace/Darius
   git fetch --tags origin
   git checkout master
   git pull --ff-only origin master
   git status --short
   git rev-parse HEAD
   git tag --points-at HEAD
   gh repo view galaxycoils/darius --json defaultBranchRef,url
   gh release view v1.1.0 --json tagName,targetCommitish,url,isDraft,isPrerelease
   ```
2. Expected: empty status; branch `master`; default branch `master`; `v1.1.0` exists. Record actual SHA in the execution log.
3. Run baseline gates:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo run -q -p darius-cli -- --version
   ```
4. Expected: all exit `0`; version output is `darius 1.1.0`.
5. If baseline is red, fix only the baseline failure and commit:
   ```bash
   git add <exact-fixed-files>
   git commit -m "fix: restore v1.1.0 baseline"
   ```

## Task 0.2: Add PTY and snapshot test dependencies

**Objective:** Give the TUI deterministic reducer/render tests plus a real terminal smoke test.

**Files:**
- Modify: `crates/darius-tui/Cargo.toml`
- Modify: `tests/harness_e2e/Cargo.toml`

1. Add to `crates/darius-tui/Cargo.toml`:
   ```toml
   [dependencies]
   ratatui = "0.27"
   crossterm = "0.27"
   unicode-width = "0.1"
   darius-cognitive = { path = "../darius-cognitive" }
   serde = { workspace = true, features = ["derive"] }

   [dev-dependencies]
   insta = "1"
   pretty_assertions = "1"
   ```
2. Add to `tests/harness_e2e/Cargo.toml`:
   ```toml
   assert_cmd = "2"
   predicates = "3"
   portable-pty = "0.8"
   ```
3. Run:
   ```bash
   cargo check -p darius-tui
   cargo test -p harness_e2e --no-run
   ```
4. Expected: both exit `0`.
5. Commit:
   ```bash
   git add crates/darius-tui/Cargo.toml tests/harness_e2e/Cargo.toml Cargo.lock
   git commit -m "test(tui): add snapshot and PTY harness"
   ```

---

# Work Unit 1 — One canonical event/session spine

## Task 1.1: Remove duplicated cognitive loop implementations

**Objective:** Make every surface call the same event-emitting runner.

**Files:**
- Modify: `crates/darius-cognitive/src/lib.rs`
- Test: `crates/darius-cognitive/src/lib.rs` (`#[cfg(test)]`)

1. Write a failing compatibility test:
   ```rust
   #[test]
   fn run_loop_wrapper_emits_the_same_terminal_acceptance() {
       let dir = temp_profile("wrapper");
       let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
       let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();
       let model = Box::new(MockModel::new(
           r#"{"tasks":[{"title":"finish"}]}"#.into(),
           vec!["DONE".into()],
       ));
       let (_plan, acceptance) = run_loop(
           &LoopPolicy::default(),
           "finish",
           model,
           &mut tools,
           &memory,
       ).unwrap();
       assert!(matches!(acceptance, Acceptance::Accepted));
   }
   ```
2. Run:
   ```bash
   cargo test -p darius-cognitive run_loop_wrapper_emits_the_same_terminal_acceptance -- --exact
   ```
3. Expected before refactor: test passes, establishing compatibility.
4. Replace the legacy body of `run_loop` with a wrapper around the canonical implementation:
   ```rust
   pub fn run_loop(
       policy: &LoopPolicy,
       goal: &str,
       model: Box<dyn Model>,
       tools: &mut darius_tools::ToolRegistry,
       memory: &darius_memory::MemoryEngine,
   ) -> Result<(Plan, Acceptance), CognitiveError> {
       let (runner, _events) = CognitiveLoop::new();
       runner.run(policy, goal, model, tools, memory)
   }
   ```
5. Run:
   ```bash
   cargo test -p darius-cognitive
   ```
6. Expected: all cognitive tests pass; no duplicated execution loop remains.
7. Commit:
   ```bash
   git add crates/darius-cognitive/src/lib.rs
   git commit -m "refactor(cognitive): use one event-emitting loop"
   ```

## Task 1.2: Move UI events into a dedicated module

**Objective:** Make `UiEvent` reusable without duplicating it in TUI/web.

**Files:**
- Create: `crates/darius-cognitive/src/ui_events.rs`
- Modify: `crates/darius-cognitive/src/lib.rs`
- Modify: `crates/darius-web/Cargo.toml`
- Modify: `crates/darius-web/src/lib.rs`

1. Create `crates/darius-cognitive/src/ui_events.rs` with complete canonical types:
   ```rust
   use serde::{Deserialize, Serialize};

   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
   #[serde(tag = "type", rename_all = "snake_case")]
   pub enum UiEvent {
       Header { profile: String, model: String, goal: String },
       UserMessage { text: String },
       AssistantDelta { text: String },
       Thinking { text: String, elapsed_ms: u64 },
       ToolStart { id: String, name: String, args_preview: String },
       ToolEnd { id: String, ok: bool, preview: String, spilled: Option<String> },
       Diff { file: String, summary: String, lines: Vec<DiffLine> },
       TaskBoard(Vec<TaskSnapshot>),
       PermissionRequired { id: String, title: String, command: String, reason: String },
       PermissionResolved { id: String, choice: PermissionChoice },
       Accept { passed: bool, notes: String },
       Status { line: String },
       A2aTask { task_id: String, state: String },
       Done,
   }

   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
   pub struct TaskSnapshot { pub id: String, pub title: String, pub status: String }

   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
   pub struct DiffLine { pub kind: DiffKind, pub old: Option<u32>, pub new: Option<u32>, pub text: String }

   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
   #[serde(rename_all = "snake_case")]
   pub enum DiffKind { Context, Add, Delete }

   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
   #[serde(rename_all = "snake_case")]
   pub enum PermissionChoice { AllowOnce, AllowSession, Deny }
   ```
2. In `crates/darius-cognitive/src/lib.rs`, add:
   ```rust
   pub mod ui_events;
   pub use ui_events::*;
   ```
   Remove the old inline definitions.
3. Add `darius-cognitive = { path = "../darius-cognitive" }` to `crates/darius-web/Cargo.toml` and replace the duplicate web event enum with `use darius_cognitive::UiEvent;`.
4. Run:
   ```bash
   cargo test -p darius-cognitive -p darius-web
   ```
5. Expected: all tests pass and `rg "pub enum UiEvent" crates` returns exactly one definition.
6. Commit:
   ```bash
   git add crates/darius-cognitive crates/darius-web Cargo.lock
   git commit -m "refactor(ui): canonicalize UiEvent contract"
   ```

## Task 1.3: Make event emission configurable and metadata-correct

**Objective:** Stop hard-coding `profile: default` and `model: mock`.

**Files:**
- Modify: `crates/darius-cognitive/src/lib.rs`
- Test: `crates/darius-cognitive/src/lib.rs`

1. Add:
   ```rust
   #[derive(Debug, Clone)]
   pub struct RunMetadata {
       pub profile: String,
       pub model: String,
       pub mode: String,
   }
   ```
2. Change `CognitiveLoop::run` to accept `metadata: &RunMetadata` and emit it in `Header`.
3. Write a failing test asserting the first event contains `profile = "work"`, `model = "gpt-4o-mini"`.
4. Run failing test; expected mismatch against current hard-coded values.
5. Implement minimally and update all callers (`crates/darius-cli/src/lib.rs`, `tests/harness_e2e/src/lib.rs`).
6. Run:
   ```bash
   cargo test -p darius-cognitive -p darius-cli -p harness_e2e
   ```
7. Expected: all pass.
8. Commit:
   ```bash
   git add crates/darius-cognitive crates/darius-cli tests/harness_e2e
   git commit -m "fix(ui): emit real profile and model metadata"
   ```

---

# Work Unit 2 — TUI domain model and command system

## Task 2.1: Split the monolithic TUI module

**Objective:** Establish maintainable reducer/render/input boundaries before visual work.

**Files:**
- Replace: `crates/darius-tui/src/lib.rs`
- Create: `crates/darius-tui/src/app.rs`
- Create: `crates/darius-tui/src/commands.rs`
- Create: `crates/darius-tui/src/input.rs`
- Create: `crates/darius-tui/src/render.rs`
- Create: `crates/darius-tui/src/terminal.rs`
- Create: `crates/darius-tui/src/theme.rs`

1. Replace `lib.rs` with:
   ```rust
   pub mod app;
   pub mod commands;
   pub mod input;
   pub mod render;
   pub mod terminal;
   pub mod theme;

   pub use app::{Action, AppState, Effort, Mode, TurnItem};
   pub use terminal::run_tui;
   ```
2. Create empty compilable modules with public stubs only.
3. Run:
   ```bash
   cargo check -p darius-tui
   ```
4. Expected: exit `0`.
5. Commit:
   ```bash
   git add crates/darius-tui/src
   git commit -m "refactor(tui): split app input render and terminal modules"
   ```

## Task 2.2: Define mode, effort, turn, and permission state

**Objective:** Encode all visual/interactive state as testable plain Rust.

**Files:**
- Modify: `crates/darius-tui/src/app.rs`
- Test: `crates/darius-tui/src/app.rs`

1. Add complete core enums:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Mode { Auto, Manual, AcceptEdits, Plan }

   impl Mode {
       pub fn next(self) -> Self {
           match self {
               Self::Auto => Self::Manual,
               Self::Manual => Self::AcceptEdits,
               Self::AcceptEdits => Self::Plan,
               Self::Plan => Self::Auto,
           }
       }
       pub fn label(self) -> &'static str {
           match self {
               Self::Auto => "⏵⏵ auto mode on",
               Self::Manual => "⏸ manual mode on",
               Self::AcceptEdits => "⏵⏵ accept edits on",
               Self::Plan => "⏸ plan mode on",
           }
       }
   }

   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Effort { Low, Medium, High, XHigh, Max, Ultracode }

   impl Effort {
       pub fn chip(self) -> &'static str {
           match self {
               Self::Low => "○ low",
               Self::Medium => "◐ medium",
               Self::High => "● high",
               Self::XHigh => "◉ xhigh",
               Self::Max => "◈ max",
               Self::Ultracode => "✦ ultracode",
           }
       }
   }
   ```
2. Add `TurnItem` variants matching canonical `UiEvent`, `PermissionState`, `ComposerState`, and `AppState` with profile/model/cwd/context usage/scroll/selection.
3. Write tests for `Mode::next()` cycle and effort chips.
4. Run:
   ```bash
   cargo test -p darius-tui app::tests
   ```
5. Expected: pass.
6. Commit:
   ```bash
   git add crates/darius-tui/src/app.rs
   git commit -m "feat(tui): model modes effort turns and permissions"
   ```

## Task 2.3: Implement the pure UiEvent reducer

**Objective:** Make live events deterministically update transcript/tasks/permissions.

**Files:**
- Modify: `crates/darius-tui/src/app.rs`
- Test: `crates/darius-tui/src/app.rs`

1. Write failing tests:
   - `Header` updates metadata.
   - adjacent `AssistantDelta` events coalesce into one assistant turn.
   - `ToolStart` then `ToolEnd` updates one tool item.
   - `TaskBoard` replaces current todos.
   - `PermissionRequired` activates chooser and blocks submission.
   - `Done` sets `running = false`.
2. Run:
   ```bash
   cargo test -p darius-tui reducer_ -- --nocapture
   ```
3. Expected: failures because `AppState::apply_event` does not exist.
4. Implement:
   ```rust
   impl AppState {
       pub fn apply_event(&mut self, event: UiEvent) {
           match event {
               UiEvent::Header { profile, model, goal } => {
                   self.profile = profile;
                   self.model = model;
                   self.goal = Some(goal);
                   self.running = true;
               }
               UiEvent::AssistantDelta { text } => self.append_assistant_delta(text),
               UiEvent::ToolStart { id, name, args_preview } => self.start_tool(id, name, args_preview),
               UiEvent::ToolEnd { id, ok, preview, spilled } => self.finish_tool(&id, ok, preview, spilled),
               UiEvent::TaskBoard(tasks) => self.tasks = tasks,
               UiEvent::PermissionRequired { id, title, command, reason } => {
                   self.permission = Some(PermissionState::new(id, title, command, reason));
               }
               UiEvent::Done => self.running = false,
               other => self.push_event(other),
           }
       }
   }
   ```
5. Run the tests again; expected pass.
6. Commit:
   ```bash
   git add crates/darius-tui/src/app.rs
   git commit -m "feat(tui): reduce UiEvents into session state"
   ```

## Task 2.4: Build one shared slash/dash registry

**Objective:** Drive palette, help, parser, and execution from one source of truth.

**Files:**
- Modify: `crates/darius-tui/src/commands.rs`
- Test: `crates/darius-tui/src/commands.rs`

1. Create:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum CommandId {
       Help, Clear, Compact, Model, Mode, Effort, Permissions,
       Memory, Pack, Tasks, Plan, Status, Config, Skills, A2a,
       Serve, Stop, Quit,
   }

   #[derive(Debug, Clone, Copy)]
   pub struct CommandSpec {
       pub id: CommandId,
       pub name: &'static str,
       pub description: &'static str,
       pub accepts_args: bool,
   }

   pub const COMMANDS: &[CommandSpec] = &[
       CommandSpec { id: CommandId::Help, name: "/help", description: "Show commands and keyboard shortcuts", accepts_args: false },
       CommandSpec { id: CommandId::Clear, name: "/clear", description: "Clear the visible transcript", accepts_args: false },
       CommandSpec { id: CommandId::Compact, name: "/compact", description: "Compact session context into memory", accepts_args: false },
       CommandSpec { id: CommandId::Model, name: "/model", description: "Show or select the session model", accepts_args: true },
       CommandSpec { id: CommandId::Mode, name: "/mode", description: "Cycle or select auto/manual/accept-edits/plan", accepts_args: true },
       CommandSpec { id: CommandId::Effort, name: "/effort", description: "Select low/medium/high/xhigh/max/ultracode", accepts_args: true },
       CommandSpec { id: CommandId::Permissions, name: "/permissions", description: "Show the current permission policy", accepts_args: true },
       CommandSpec { id: CommandId::Memory, name: "/memory", description: "Search durable memory", accepts_args: true },
       CommandSpec { id: CommandId::Pack, name: "/pack", description: "Show the bounded MemoryPack", accepts_args: false },
       CommandSpec { id: CommandId::Tasks, name: "/tasks", description: "Show the current task board", accepts_args: false },
       CommandSpec { id: CommandId::Plan, name: "/plan", description: "Enter plan mode", accepts_args: false },
       CommandSpec { id: CommandId::Status, name: "/status", description: "Show profile/model/context/kernel status", accepts_args: false },
       CommandSpec { id: CommandId::Config, name: "/config", description: "Show effective profile configuration", accepts_args: false },
       CommandSpec { id: CommandId::Skills, name: "/skills", description: "List or search installed skills", accepts_args: true },
       CommandSpec { id: CommandId::A2a, name: "/a2a", description: "Show A2A card and task status", accepts_args: true },
       CommandSpec { id: CommandId::Serve, name: "/serve", description: "Start or show the local web/A2A server", accepts_args: true },
       CommandSpec { id: CommandId::Stop, name: "/stop", description: "Interrupt the active turn", accepts_args: false },
       CommandSpec { id: CommandId::Quit, name: "/quit", description: "Exit Darius", accepts_args: false },
   ];
   ```
2. Add `filter(query)`, `parse(input)`, and `dash_alias_to_slash`.
3. Tests must prove:
   - `/mo` filters to `/model`, `/mode`.
   - `-status` parses as `/status`.
   - descriptions and palette use `COMMANDS` (no second list).
   - unknown commands return an error with `/help` hint.
4. Run:
   ```bash
   cargo test -p darius-tui commands::tests
   ```
5. Expected: pass.
6. Commit:
   ```bash
   git add crates/darius-tui/src/commands.rs
   git commit -m "feat(tui): add shared slash and dash command registry"
   ```

## Task 2.5: Map keys to actions without terminal I/O

**Objective:** Make keyboard behavior testable independently from crossterm.

**Files:**
- Modify: `crates/darius-tui/src/input.rs`
- Test: `crates/darius-tui/src/input.rs`

1. Define:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Action {
       Insert(char), Backspace, Submit, Quit, Cancel, Interrupt,
       OpenPalette, PaletteNext, PalettePrev, PaletteAccept,
       CycleMode, CycleEffort, Scroll(i16), ToggleTool,
       PermissionNext, PermissionPrev, PermissionChoose,
   }
   ```
2. Implement `map_key(KeyEvent, &AppState) -> Option<Action>` with priority: permission chooser → slash palette → composer → transcript.
3. Write tests for `/`, `-` at column zero, Up/Down, Enter, Esc, Ctrl+C, Shift+Tab, PageUp/PageDown.
4. Run:
   ```bash
   cargo test -p darius-tui input::tests
   ```
5. Expected: pass.
6. Commit:
   ```bash
   git add crates/darius-tui/src/input.rs
   git commit -m "feat(tui): map Claude-style terminal keybindings"
   ```

---

# Work Unit 3 — Claude-like rendering

## Task 3.1: Implement the palette and terminal color fallback

**Objective:** Centralize visual tokens and guarantee readable fallback terminals.

**Files:**
- Modify: `crates/darius-tui/src/theme.rs`
- Test: `crates/darius-tui/src/theme.rs`

1. Add semantic colors (`brand`, `text`, `muted`, `active`, `rule`, mode colors, permission, add/delete) with truecolor and ANSI fallback constructors.
2. Add `Theme::detect(env: &impl Env)`; `COLORTERM=truecolor|24bit` enables RGB.
3. Tests: truecolor maps exact hex; fallback maps to named/indexed ANSI colors.
4. Run:
   ```bash
   cargo test -p darius-tui theme::tests
   ```
5. Commit:
   ```bash
   git add crates/darius-tui/src/theme.rs
   git commit -m "feat(tui): add Claude-like semantic terminal theme"
   ```

## Task 3.2: Render the Darius launch/welcome block

**Objective:** Replace the generic header panel with a compact launch card.

**Files:**
- Modify: `crates/darius-tui/src/render.rs`
- Test: `crates/darius-tui/src/render.rs`

1. Add `render_welcome(frame, area, state, theme)` with:
   ```text
   ╭─ ◆ darius v1.1.1 ─────────────────────────────────────────╮
   │ Welcome back                                              │
   │ model   gpt-4o-mini                                       │
   │ cwd     ~/dev/project                                     │
   │ profile default  ·  kernel rust  ·  /help                 │
   ╰────────────────────────────────────────────────────────────╯
   ```
2. Use one border only around the welcome card; do not border transcript/tasks/composer as separate dashboards.
3. Snapshot at 80×24 and 120×36:
   ```rust
   insta::assert_snapshot!(render_to_string(80, 24, fixture_state()), @"...");
   ```
4. Run:
   ```bash
   cargo insta test -p darius-tui --accept
   cargo insta test -p darius-tui
   ```
5. Expected: second run reports no pending snapshots.
6. Commit snapshots and code.

## Task 3.3: Render messages, thinking, tools, todos, and diffs

**Objective:** Recreate the full Claude-like turn grammar from canonical Darius events.

**Files:**
- Modify: `crates/darius-tui/src/render.rs`
- Test: `crates/darius-tui/src/render.rs`

1. Write one snapshot fixture containing user message, assistant text, thinking, three todos, collapsed + expanded tool calls, and a diff.
2. Expected initial snapshot failure.
3. Implement render functions:
   ```rust
   fn render_user(text: &str, theme: &Theme) -> Vec<Line<'static>>;
   fn render_assistant(text: &str, theme: &Theme) -> Vec<Line<'static>>;
   fn render_thinking(text: &str, elapsed_ms: u64, theme: &Theme) -> Vec<Line<'static>>;
   fn render_tool(tool: &ToolView, expanded: bool, theme: &Theme) -> Vec<Line<'static>>;
   fn render_tasks(tasks: &[TaskSnapshot], theme: &Theme) -> Vec<Line<'static>>;
   fn render_diff(diff: &DiffView, theme: &Theme) -> Vec<Line<'static>>;
   ```
4. Required glyphs: user `❯`, tool `⏺`, result `⎿`, thinking `✦`, done `✓`, active `◐`, todo `○`, diff `+/-`.
5. Run snapshots twice (accept, then verify).
6. Commit:
   ```bash
   git add crates/darius-tui/src/render.rs crates/darius-tui/src/snapshots
   git commit -m "feat(tui): render Claude-style session transcript"
   ```

## Task 3.4: Render interactive permission chooser

**Objective:** Make permission approval keyboard-native and unambiguous.

**Files:**
- Modify: `crates/darius-tui/src/render.rs`
- Modify: `crates/darius-tui/src/app.rs`
- Test: `crates/darius-tui/src/render.rs`

1. Add snapshot with the rose permission box and selected option marker `❯`.
2. Implement three options exactly from the interaction contract.
3. Reducer actions Up/Down wrap selection; Enter emits `PermissionChoice`; Esc chooses Deny and returns focus to composer with corrective prompt text.
4. Tests prove selection wrap, allow-session persistence, deny behavior.
5. Run:
   ```bash
   cargo test -p darius-tui permission_
   cargo insta test -p darius-tui
   ```
6. Commit:
   ```bash
   git add crates/darius-tui/src
   git commit -m "feat(tui): add interactive permission chooser"
   ```

## Task 3.5: Render slash palette and dual-rule composer

**Objective:** Match the Claude-like prompt and discoverable command menu.

**Files:**
- Modify: `crates/darius-tui/src/render.rs`
- Test: `crates/darius-tui/src/render.rs`

1. Snapshot states:
   - blank composer, `xhigh` effort, auto mode;
   - `/mo` filter showing `/model` and `/mode` with active light-blue row;
   - plan mode footer;
   - ultracode effort rainbow approximated with six colored rule spans.
2. Render command names in a stable `20ch` column followed by description.
3. Composer lines:
   ```text
                                                ◉ xhigh · /effort
   ──────────────────────────────────────────────────────────────
   ❯ prompt text
   ──────────────────────────────────────────────────────────────
   ⏵⏵ auto mode on (shift+tab to cycle) · ? for shortcuts
   ```
4. Run/accept/verify snapshots.
5. Commit:
   ```bash
   git add crates/darius-tui/src/render.rs crates/darius-tui/src/snapshots
   git commit -m "feat(tui): add Claude-style composer and command palette"
   ```

## Task 3.6: Handle resize, Unicode width, scroll, and tiny terminals

**Objective:** Prevent clipping/panics from terminal geometry and wide glyphs.

**Files:**
- Modify: `crates/darius-tui/src/render.rs`
- Modify: `crates/darius-tui/src/app.rs`
- Test: `crates/darius-tui/src/render.rs`

1. Add render tests at 40×12, 80×24, 160×50 and with CJK/emoji text.
2. Use `unicode_width::UnicodeWidthStr`; never slice strings by byte index for visible width.
3. Under 60 columns, hide secondary metadata and use one-line mode footer.
4. Under 12 rows, render compact transcript + composer only.
5. Preserve scroll position unless user is already at bottom; new events auto-follow only at bottom.
6. Expected: no panic, no line wider than buffer width.
7. Commit:
   ```bash
   git add crates/darius-tui/src
   git commit -m "fix(tui): handle resize unicode and transcript scrolling"
   ```

---

# Work Unit 4 — Runtime wiring, commands, provider, and server truth

## Task 4.1: Add safe terminal lifecycle guard

**Objective:** Always restore raw mode, cursor, and alternate screen—even on errors/panic.

**Files:**
- Modify: `crates/darius-tui/src/terminal.rs`
- Test: `crates/darius-tui/src/terminal.rs`

1. Implement `TerminalGuard`:
   ```rust
   pub struct TerminalGuard;

   impl TerminalGuard {
       pub fn enter() -> io::Result<Self> {
           crossterm::terminal::enable_raw_mode()?;
           crossterm::execute!(io::stdout(), EnterAlternateScreen, Hide)?;
           Ok(Self)
       }
   }

   impl Drop for TerminalGuard {
       fn drop(&mut self) {
           let _ = crossterm::terminal::disable_raw_mode();
           let _ = crossterm::execute!(io::stdout(), Show, LeaveAlternateScreen);
       }
   }
   ```
2. Abstract crossterm calls behind a test backend and test cleanup order.
3. Run:
   ```bash
   cargo test -p darius-tui terminal::tests
   ```
4. Commit.

## Task 4.2: Introduce a controller channel between TUI and runtime

**Objective:** Keep model/tool execution off the draw/input loop.

**Files:**
- Create: `crates/darius-tui/src/controller.rs`
- Modify: `crates/darius-tui/src/lib.rs`
- Modify: `crates/darius-tui/Cargo.toml`

1. Add Tokio and cancellation dependencies:
   ```toml
   tokio = { workspace = true }
   tokio-util = "0.7"
   ```
2. Define:
   ```rust
   pub enum RuntimeCommand {
       SubmitGoal { text: String, mode: Mode, effort: Effort },
       ResolvePermission { id: String, choice: PermissionChoice },
       ExecuteSlash(ParsedCommand),
       Interrupt,
       Shutdown,
   }

   pub struct TuiController {
       pub commands: tokio::sync::mpsc::Sender<RuntimeCommand>,
       pub events: tokio::sync::broadcast::Receiver<UiEvent>,
   }
   ```
3. Test channel ordering and cancellation.
4. Run:
   ```bash
   cargo test -p darius-tui controller::tests
   ```
5. Commit.

## Task 4.3: Build one reusable session runtime in `darius-cli`

**Objective:** Construct profile/config/memory/tools/model once for CLI/TUI/web/A2A.

**Files:**
- Create: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cli/src/lib.rs`
- Test: `crates/darius-cli/src/runtime.rs`

1. Define `RuntimeConfig` and `SessionRuntime::from_profile(profile)`; it must:
   - load `ProfileConfig`;
   - create `MemoryEngine` and `ToolRegistry`;
   - register memory/task/coding tools;
   - select Mock only when no model config exists;
   - return a clear `MissingApiKey { env }` error when config exists but key is absent;
   - expose an event broadcaster and cancellation token.
2. Write failing tests for offline Mock and configured-without-key error.
3. Implement minimally.
4. Run:
   ```bash
   cargo test -p darius-cli runtime::tests
   ```
5. Commit:
   ```bash
   git add crates/darius-cli/src/runtime.rs crates/darius-cli/src/lib.rs
   git commit -m "refactor(cli): share one session runtime across surfaces"
   ```

## Task 4.4: Implement real OpenAI-compatible model HTTP

**Objective:** Make “live provider” a truthful capability instead of a formatted stub.

**Files:**
- Modify: `crates/darius-daemon/Cargo.toml`
- Modify: `crates/darius-daemon/src/model_router.rs`
- Test: `crates/darius-daemon/src/model_router.rs`

1. Add non-optional HTTP test/runtime dependencies:
   ```toml
   reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "blocking"] }

   [dev-dependencies]
   wiremock = "0.6"
   ```
2. Extend `Provider` with `api_key_env: String`; never store the key itself.
3. Write wiremock tests asserting:
   - POST `${base_url}/chat/completions`;
   - `Authorization: Bearer <env-key>`;
   - request model/messages;
   - parsed `choices[0].message.content`;
   - 401/429/5xx map to explicit `RouterError` variants;
   - secret never appears in Display/Debug/error text.
4. Run the failing tests first; expected response remains current stub.
5. Implement `OpenAiCompatibleClient` and call it from `ModelRouter::route`.
6. Run:
   ```bash
   cargo test -p darius-daemon model_router
   cargo clippy -p darius-daemon --all-targets -- -D warnings
   ```
7. Commit:
   ```bash
   git add crates/darius-daemon Cargo.lock
   git commit -m "feat(model): call OpenAI-compatible providers"
   ```

## Task 4.5: Wire `darius tui` to the real runtime

**Objective:** Submitting text actually runs Darius and streams events into the TUI.

**Files:**
- Modify: `crates/darius-cli/src/lib.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-tui/src/terminal.rs`
- Test: `crates/darius-cli/src/runtime.rs`

1. Replace no-argument `darius_tui::run_tui()` with `run_tui(TuiController, InitialMetadata)`.
2. `cmd_tui(args)` resolves `--profile`, builds runtime, spawns it, and passes controller.
3. On normal Enter: push `UserMessage`, submit goal, clear composer, set running.
4. On Ctrl+C/Esc while running: cancel only the turn; second Ctrl+C within 1 second exits.
5. Test with MockModel: submit → receive Header/TaskBoard/Accept/Done in order.
6. Run:
   ```bash
   cargo test -p darius-cli runtime::tests
   cargo test -p darius-tui
   ```
7. Commit:
   ```bash
   git add crates/darius-cli crates/darius-tui
   git commit -m "feat(tui): run and stream real Darius sessions"
   ```

## Task 4.6: Execute every visible command

**Objective:** Eliminate fake slash menu entries.

**Files:**
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-tui/src/commands.rs`
- Test: `crates/darius-cli/src/runtime.rs`

1. Add table-driven test iterating every `COMMANDS` row and asserting execution returns `CommandResult`, not `Unknown`.
2. Implement behavior:
   - `/help`: command + shortcut list.
   - `/clear`: transcript-only reset.
   - `/compact`: persist a compact episode; show record ID.
   - `/model [id]`: show current or update session provider selection.
   - `/mode`, `/effort`: parse/select; no model call.
   - `/permissions`: show/set `manual|accept-edits|auto` policy.
   - `/memory q`, `/pack`, `/tasks`, `/status`, `/config`, `/skills q`, `/a2a`, `/serve`, `/stop`, `/quit` call existing domain APIs.
   - `/plan`: set mode `Plan`.
3. `-command` aliases normalize before dispatch.
4. Run:
   ```bash
   cargo test -p darius-cli every_visible_tui_command_executes
   ```
5. Commit:
   ```bash
   git add crates/darius-cli/src/runtime.rs crates/darius-tui/src/commands.rs
   git commit -m "feat(tui): execute complete slash command surface"
   ```

## Task 4.7: Replace `darius serve` stub with a real localhost server

**Objective:** Make `/serve` and `darius serve` truthful and reusable by the TUI.

**Files:**
- Modify: `crates/darius-web/src/lib.rs`
- Modify: `crates/darius-cli/src/lib.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Test: `crates/darius-web/src/lib.rs`

1. Add `pub async fn serve(listener: TcpListener, runtime: Arc<SessionRuntime>)`.
2. `POST /api/goal` and `POST /a2a/tasks` must enqueue on the same runtime used by TUI; remove synthetic Header/Done emission.
3. `cmd_serve` builds Tokio runtime, binds `127.0.0.1:<port>`, prints the actual bound URL, then awaits Axum.
4. Add explicit `--bind 0.0.0.0` support with stderr warning; default remains localhost.
5. Add Axum oneshot tests for `/`, card, task create/get, and goal validation.
6. Run:
   ```bash
   cargo test -p darius-web
   cargo test -p darius-cli
   ```
7. Commit:
   ```bash
   git add crates/darius-web crates/darius-cli
   git commit -m "fix(serve): run web and A2A on the shared session runtime"
   ```

## Task 4.8: Make permission gating real

**Objective:** Block side-effecting tools until the selected permission policy resolves them.

**Files:**
- Modify: `crates/darius-tools/src/lib.rs`
- Modify: `crates/darius-cognitive/src/lib.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Test: `crates/darius-cognitive/src/lib.rs`

1. Add `ToolRisk::{ReadOnly, WorkspaceWrite, Shell, Network}` metadata at registration.
2. Add `PermissionBroker` trait returning a future/receiver for `PermissionChoice`.
3. In manual mode, CognitiveLoop emits `PermissionRequired` and waits before `ToolRegistry::execute` for `WorkspaceWrite|Shell|Network`.
4. `AllowSession` caches a scoped approval by tool/risk; Deny emits failed ToolEnd and does not execute.
5. Tests use atomic counters to prove denied handler count remains zero and approved count becomes one.
6. Run:
   ```bash
   cargo test -p darius-cognitive permission_
   cargo test -p darius-tools
   ```
7. Commit:
   ```bash
   git add crates/darius-tools crates/darius-cognitive crates/darius-cli
   git commit -m "feat(safety): gate side-effecting tools in TUI sessions"
   ```

---

# Work Unit 5 — Optional IPyKernel truth gate

## Task 5.1: Decide the v1.1.1 IPyKernel claim from executable evidence

**Objective:** Prevent a stub backend from being marketed as complete.

**Files:**
- Modify: `crates/darius-rlm/src/ipykernel.rs`
- Modify: `crates/darius-rlm/Cargo.toml`
- Modify: `README.md`
- Modify: `.github/workflows/ci.yml`

1. Add a feature-build test that parses a real temporary `kernel.json` and validates endpoint/key/signature configuration.
2. Add ignored integration test:
   ```rust
   #[test]
   #[ignore = "requires python -m ipykernel and libzmq"]
   fn ipykernel_executes_print_expression() {
       let mut kernel = IpKernelBackend::spawn().unwrap();
       let reply = kernel.execute("print(6 * 7)").unwrap();
       assert!(reply.stdout.contains("42"));
       kernel.shutdown().unwrap();
   }
   ```
3. Run default:
   ```bash
   cargo test -p darius-rlm
   ```
   Expected: pure-Rust/default pass, no ZMQ link.
4. Run feature build:
   ```bash
   cargo test -p darius-rlm --features rlm-ipykernel
   ```
5. If the ignored live test can pass locally, implement Jupyter wire execute + HMAC and enable an optional CI job. If it cannot, label the backend **experimental** in TUI/README and do not claim “fully wired” in v1.1.1 notes.
6. Commit:
   ```bash
   git add crates/darius-rlm README.md .github/workflows/ci.yml Cargo.lock
   git commit -m "fix(rlm): truth-gate the optional ipykernel backend"
   ```

---

# Work Unit 6 — End-to-end and visual verification

## Task 6.1: Add deterministic golden-screen snapshots

**Objective:** Lock the intended Claude-like visual grammar.

**Files:**
- Create: `crates/darius-tui/tests/golden.rs`
- Create: `crates/darius-tui/tests/snapshots/*.snap`

1. Build fixture state with welcome, user, assistant, todos, tool call, diff, permission, palette, composer.
2. Snapshot 80×24 and 120×36.
3. Run:
   ```bash
   cargo insta test -p darius-tui --accept
   cargo insta test -p darius-tui
   ```
4. Expected: stable snapshots, zero pending.
5. Commit:
   ```bash
   git add crates/darius-tui/tests
   git commit -m "test(tui): lock Claude-like golden screens"
   ```

## Task 6.2: Add PTY keyboard-flow E2E

**Objective:** Prove the actual binary handles commands and restores the terminal.

**Files:**
- Modify: `tests/harness_e2e/src/lib.rs`

1. Add PTY test sequence:
   - spawn `target/debug/darius tui --profile e2e` with `TERM=xterm-256color`;
   - wait for `◆ darius`;
   - type `/help`, Enter; assert `/model` and `/quit` appear;
   - type `/mode plan`, Enter; assert `plan mode on`;
   - type `hello`, Enter; assert `❯ hello` and `Done`/accept state;
   - type `/quit`, Enter;
   - assert exit `0` and PTY is restored.
2. Run:
   ```bash
   cargo build -p darius-cli
   cargo test -p harness_e2e tui_pty_ -- --nocapture
   ```
3. Expected: pass under 10 seconds.
4. Commit.

## Task 6.3: Add web/A2A shared-runtime E2E

**Objective:** Prove TUI/web/A2A consume one event spine.

**Files:**
- Modify: `tests/harness_e2e/src/lib.rs`
- Modify: `tests/harness_e2e/Cargo.toml`

1. Start server on `127.0.0.1:0` with Mock runtime.
2. Subscribe SSE; POST `/api/goal`; assert ordered Header → TaskBoard → Accept → Done.
3. POST `/a2a/tasks`; poll GET until Completed; assert SSE includes matching A2aTask transitions.
4. Run:
   ```bash
   cargo test -p harness_e2e web_a2a_shared_runtime -- --nocapture
   ```
5. Expected: pass.
6. Commit:
   ```bash
   git add tests/harness_e2e
   git commit -m "test: verify TUI web and A2A share one runtime"
   ```

## Task 6.4: Manual visual critique in a real terminal

**Objective:** Catch layout/interaction problems snapshots miss.

**Files:** none unless defects are found.

1. Run at three sizes:
   ```bash
   cargo run -p darius-cli -- tui
   ```
   Test approximately 60×16, 80×24, 140×40.
2. Checklist:
   - launch card only bordered region;
   - transcript reads as one continuous turn;
   - palette never covers composer footer;
   - selection is obvious without relying only on color;
   - permission chooser owns focus;
   - Shift+Tab cycles mode;
   - wide Unicode does not corrupt edges;
   - Ctrl+C interrupts first, exits second;
   - terminal is restored after `/quit`, panic injection, and Ctrl+C.
3. Compare against the Brainless reference grammar, not its source. Remove one unnecessary border/decorative element if the screen still looks dashboard-like.
4. Commit each defect fix separately (`fix(tui): ...`) with its regression test.

---

# Work Unit 7 — CI, documentation, version, release

## Task 7.1: Add a real CI workflow

**Objective:** Gate the release on fmt, clippy, tests, and build rather than only a tag workflow.

**Files:**
- Create: `.github/workflows/ci.yml`

Use:
```yaml
name: CI
on:
  pull_request:
  push:
    branches: [master]
permissions:
  contents: read
jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - run: cargo build --release -p darius-cli
```
1. Validate YAML:
   ```bash
   ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml")'
   ```
2. Expected: exit `0`.
3. Commit:
   ```bash
   git add .github/workflows/ci.yml
   git commit -m "ci: gate v1.1.1 on full Rust checks"
   ```

## Task 7.2: Fix release workflow tag and asset integrity

**Objective:** Ensure tag-triggered builds create the right GitHub release and verifiable archives.

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `install.sh`

1. Remove `tag_name: "$TAG"` (action inputs do not expand shell env). Use:
   ```yaml
   with:
     tag_name: ${{ github.ref_name }}
     files: |
       artifacts/*/*.tar.gz
       artifacts/*/*.sha256
     generate_release_notes: true
     fail_on_unmatched_files: true
   ```
2. Produce `sha256` files in each matrix job and upload them.
3. Update `install.sh` to download and verify the checksum before extraction; abort on mismatch.
4. Add shell syntax test:
   ```bash
   bash -n install.sh
   ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")'
   ```
5. Expected: both exit `0`.
6. Commit:
   ```bash
   git add .github/workflows/release.yml install.sh
   git commit -m "fix(release): publish verified v1.1.1 assets"
   ```

## Task 7.3: Update README and changelog honestly

**Objective:** Document exactly what v1.1.1 does and does not do.

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

1. Change README title/version to v1.1.1.
2. Add TUI quickstart, screenshot/golden transcript, keyboard table, slash command table, mode/effort table, provider config, localhost server, A2A, and permission policy.
3. Add “Design inspiration” credit linking `https://brainless.swerdlow.dev/`; state no source was copied.
4. Correct stale “What’s NOT in v1” claims based on verified capabilities.
5. Add `CHANGELOG.md` section:
   ```markdown
   ## [1.1.1] - YYYY-MM-DD
   ### Added
   - Claude-Code-style Darius TUI with streaming turns, command palette, modes, effort, todos, diffs, and permission chooser.
   ### Fixed
   - Unified UiEvent/runtime across CLI, TUI, web, and A2A.
   - Real OpenAI-compatible provider requests and localhost server startup.
   - Verified release checksums and tag handling.
   ### Notes
   - Brainless was used as visual/interaction inspiration only.
   ```
6. Run doc command examples using a temporary profile; all must exit `0`.
7. Commit:
   ```bash
   git add README.md CHANGELOG.md
   git commit -m "docs: prepare honest v1.1.1 TUI release"
   ```

## Task 7.4: Bump every workspace package to 1.1.1

**Objective:** Align binary, agent card, manifests, docs, and tag.

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

1. Change `[workspace.package] version = "1.1.1"`.
2. Run:
   ```bash
   cargo check --workspace
   cargo run -q -p darius-cli -- --version
   ```
3. Expected: output `darius 1.1.1`; lockfile package versions are 1.1.1.
4. Commit:
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "chore: version 1.1.1"
   ```

## Task 7.5: Final double-check and adversarial review

**Objective:** Prove every acceptance criterion before tagging.

**Files:** none unless fixes are needed.

1. Run exact local gates without pipes masking exit codes:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo test -p darius-rlm --features rlm-ipykernel
   cargo build --release -p darius-cli
   cargo run -q -p darius-cli -- --version
   cargo run -q -p darius-cli -- session-smoke
   cargo run -q -p darius-cli -- memory stats
   cargo run -q -p darius-cli -- a2a card
   git diff --check
   git status --short
   ```
2. Expected: all commands exit `0`; version `1.1.1`; status empty; release binary exists.
3. Run a two-stage review:
   - spec reviewer checks every plan acceptance criterion against code/tests;
   - code-quality reviewer checks panic safety, secret leakage, terminal cleanup, event ordering, and release workflow.
4. Any failure creates a focused regression test + `fix:` commit, then rerun the whole gate.
5. Record:
   ```bash
   git rev-parse HEAD
   ls -lh target/release/darius
   ```

## Task 7.6: Push commit and wait for exact CI success

**Objective:** Verify the exact release candidate SHA remotely before tagging.

1. Run:
   ```bash
   git push origin master
   SHA=$(git rev-parse HEAD)
   gh run list --workflow=ci.yml --branch master --limit 5
   ```
2. Identify the run whose `headSha` equals `$SHA`; do not use an older green run.
3. Wait:
   ```bash
   gh run watch <run-id> --exit-status
   ```
4. Expected: exact-SHA run concludes `success`.
5. If CI fails, fix/test/commit/push and repeat with the new SHA.

## Task 7.7: Tag, publish, and verify v1.1.1

**Objective:** Complete the requested public release only after all gates pass.

1. Create annotated tag:
   ```bash
   git tag -a v1.1.1 -m "Darius v1.1.1"
   git push origin v1.1.1
   ```
2. Watch the release workflow for tag SHA:
   ```bash
   gh run list --workflow=release.yml --limit 5
   gh run watch <release-run-id> --exit-status
   ```
3. Expected: all matrix builds and release job succeed.
4. Read back external state:
   ```bash
   gh release view v1.1.1 --json tagName,targetCommitish,url,assets,isDraft,isPrerelease
   git ls-remote --tags origin refs/tags/v1.1.1
   ```
5. Verify assets include macOS arm64, macOS x86_64, Linux x86_64 archives and matching `.sha256` files.
6. Download one matching local asset and verify:
   ```bash
   TMP=$(mktemp -d)
   gh release download v1.1.1 --dir "$TMP"
   cd "$TMP"
   shasum -a 256 -c darius-macos-aarch64.sha256
   tar -xzf darius-macos-aarch64.tar.gz
   ./darius --version
   ```
7. Expected checksum `OK`; version `darius 1.1.1`.
8. Final report must include release URL, exact SHA, exact successful CI/release run IDs, binary sizes, and any deliberately experimental feature (especially IPyKernel).

---

## Tests / validation matrix

| Layer | Exact command | Acceptance |
|---|---|---|
| Command registry | `cargo test -p darius-tui commands::tests` | slash + dash alias/filter/navigation pass |
| Reducer | `cargo test -p darius-tui reducer_` | deterministic UiEvent → state |
| Rendering | `cargo insta test -p darius-tui` | no pending snapshots |
| Terminal lifecycle | `cargo test -p darius-tui terminal::tests` | raw mode/alternate screen restored |
| Permissions | `cargo test -p darius-cognitive permission_` | denied tools never execute |
| Model HTTP | `cargo test -p darius-daemon model_router` | wiremock request/response/error/secret tests pass |
| TUI PTY | `cargo test -p harness_e2e tui_pty_ -- --nocapture` | keyboard flow exits cleanly |
| Web/A2A | `cargo test -p harness_e2e web_a2a_shared_runtime -- --nocapture` | same event stream/task state |
| Default workspace | `cargo test --workspace` | all pure-default tests pass |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | zero warnings/errors |
| Release | exact-SHA GitHub Actions | CI + tag release successful |
| Artifact | checksum + `./darius --version` | `OK`, version 1.1.1 |

## Risks, tradeoffs, and open questions

- **Similarity vs identity:** Copying Brainless/Claude source is prohibited; this plan copies interaction grammar only. Darius keeps one restrained copper brand mark and owns its Rust implementation.
- **Terminal portability:** Unicode glyphs and truecolor vary. ANSI fallback and width tests are mandatory; allow ASCII fallback via `NO_COLOR`/terminal capability detection.
- **Renderer snapshots are necessary but insufficient:** PTY tests and a real terminal critique are both release gates.
- **Current architectural debt:** duplicated `UiEvent`, duplicated cognitive loop, synthetic web events, stub server, stub provider, and stub IPyKernel create false confidence. v1.1.1 must close or explicitly de-scope each claim.
- **Blocking SQLite/tool handlers:** run them on the runtime worker, never the crossterm input/draw loop. If async concurrency is introduced, do not move non-`Send` SQLite connections across threads; construct per-worker connections or use `spawn_blocking` with owned profile paths.
- **Permission race/deadlock:** runtime must continue pumping input/events while a permission awaits resolution. Test denial, cancellation, and shutdown while blocked.
- **Release workflow:** do not create `v1.1.1` until the exact master SHA is green. A tag/release existing is not proof that artifacts are valid; verify assets and checksum after publication.
- **IPyKernel:** libzmq/Python availability is environment-dependent. “Experimental” is acceptable; “fully integrated” is not unless the ignored live test passes and shutdown is verified.
- **No unresolved product questions block implementation.** Canonical commands use `/`; leading `-` is an alias as stated under assumptions.
