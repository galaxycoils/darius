# Darius TUI Functional Repair Implementation Plan

> **For Hermes:** Execute with `software-development:subagent-driven-development`. Every code task follows strict RED → GREEN → REFACTOR using `software-development:test-driven-development`; diagnose deviations using `software-development:systematic-debugging`. Do not push, tag, or release unless the user separately requests it.

**Goal:** Make `darius tui` a genuinely functional agent interface where ordinary text submission, model execution, live progress, every visible slash/dash command, permissions, cancellation, scrolling, and terminal cleanup work end-to-end rather than only rendering disconnected demos.

**Architecture:** Keep `darius-tui` as a deterministic input/reducer/render shell and place runtime ownership in `darius-cli`, which already depends on TUI, cognitive, tools, memory, daemon, and web crates. A CLI-owned worker will hold one reusable `SessionRuntime`, receive typed `RuntimeCommand`s, and emit canonical `UiEvent`s back to the TUI; the cognitive loop will expose event/cancellation/permission hooks instead of executing tools blindly or hiding events in an abandoned channel.

**Tech stack:** Rust 2024, ratatui 0.27, crossterm 0.27, Tokio channels/runtime, portable-pty, insta snapshots, existing `darius-cognitive`, `darius-tools`, `darius-memory`, `darius-daemon`, and `darius-web` crates.

---

## Current context / assumptions

- Repository: `/Users/cmd/workspace/Darius`; branch `master`; inspected clean HEAD `c7f65060ff905f8c4769bf8740b3a4c728fd52d3`; tag `v1.1.1` points at HEAD.
- Baseline unit tests pass, but they do **not** prove a functional TUI.
- Reproduction evidence:
  - `crates/darius-cli/src/lib.rs:369-372` calls `darius_tui::run_tui()` without constructing `SessionRuntime`, parsing `--profile`, or starting a runtime worker.
  - `crates/darius-tui/src/terminal.rs:84-145` contains a second, hard-coded event loop. It never calls `input::map_key`, never applies most `Action`s, never uses `TuiController`, never receives `UiEvent`, and accepts characters only while `slash_mode` is true.
  - Ordinary Enter has no branch; a user cannot submit a goal. Normal `q`, `j`, and `k` are reserved as shortcuts in `input.rs`, so even the unused mapper would prevent typing common text.
  - `crates/darius-tui/src/app.rs:221-249` reduces only permission actions. Insert, backspace, submit, palette navigation, mode, effort, scrolling, cancel, and tool expansion are inert.
  - `crates/darius-tui/src/render.rs:449-522` still draws the old bordered Header/Stream/Tasks/Input dashboard. The welcome, transcript, permission, palette, and dual-rule composer helpers above it are snapshot-only and are never used by the live app.
  - `crates/darius-tui/src/commands.rs` advertises 18 commands, but `terminal.rs` implements only `/help`, `/approve`, `/deny`, and `/quit`.
  - `crates/darius-cli/src/runtime.rs` constructs memory/tools/model/event broadcast state but has no method that runs a goal, receives permissions, changes model, compacts context, or serves command requests.
  - `darius-cognitive::CognitiveLoop` owns a private `std::sync::mpsc::Sender`; `run_loop` discards its receiver, so CLI/TUI/web cannot consume actual progress.
  - The cognitive loop executes `shell` and `write_file` immediately. `PermissionRequired` exists as a UI event but is never emitted by the execution path.
  - `tests/harness_e2e/src/lib.rs:304-374` returns early when the binary is missing; `cargo test -p harness_e2e tui_` reports PASS while printing two `SKIP` lines. These are not E2E tests.
  - `cmd_serve()` and the web `/api/goal` endpoint are stubs/synthetic; `/serve` cannot honestly claim to start a shared agent runtime.
- Preserve resource caps: MemoryPack ≤ 3500 chars, tool preview ≤ 32 KiB + spill, TaskBoard ≤ 15, ReAct ≤ 12 iterations/task.
- Default behavior remains offline and deterministic using `MockModel` when no provider is configured. A configured profile with a missing API key must fail visibly, never silently fall back.
- Default working directory is the process current directory. File writes must stay under it unless a future explicit policy expands the boundary.
- This repair should be a patch release candidate (`1.1.2`) only after all acceptance gates pass; do not change the version or publish in this implementation unless the user explicitly asks.

## Non-goals

- Do not redesign the daemon, memory schema, web dashboard aesthetics, or A2A protocol.
- Do not add a plugin system, vector database, multi-agent orchestration, or new model protocol.
- Do not retain duplicate input loops or duplicate command implementations.
- Do not mark unavailable features successful; return a visible error/status event.

---

# Work Unit 0 — Replace false-green tests with real reproduction

## Task 0.1: Record baseline and reproduce the disconnected TUI

**Objective:** Capture exact pre-fix behavior and prevent accidental work on a dirty or stale branch.

**Files:** none.

1. Run read-only baseline checks:
   ```bash
   cd /Users/cmd/workspace/Darius
   git status --short
   git branch --show-current
   git rev-parse HEAD
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
2. Expected before implementation: clean status, `master`, HEAD `c7f65060...`, and current gates green.
3. Run the deceptive test explicitly:
   ```bash
   cargo test -p harness_e2e tui_ -- --nocapture
   ```
4. Expected pre-fix output includes `SKIP: darius binary not found`; record this as a test defect, not a pass.
5. Do not commit this evidence-only step.

## Task 0.2: Move binary E2E coverage to the binary package

**Objective:** Ensure Cargo always exposes the real `darius` executable to the test instead of allowing skips.

**Files:**
- Create: `crates/darius-cli/tests/tui_pty.rs`
- Modify: `crates/darius-cli/Cargo.toml`
- Modify: `tests/harness_e2e/src/lib.rs:304-374`

**Step 1 — RED:** Add `portable-pty = "0.8"` and `wait-timeout = "0.2"` under `[dev-dependencies]` in `crates/darius-cli/Cargo.toml`. Create this exact helper shape in `crates/darius-cli/tests/tui_pty.rs`:

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn cargo_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_darius")
        .map(Into::into)
        .expect("Cargo must provide CARGO_BIN_EXE_darius for CLI integration tests")
}

fn read_until(reader: &mut dyn Read, needle: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut all = String::new();
    let mut buf = [0_u8; 4096];
    while Instant::now() < deadline {
        if let Ok(n) = reader.read(&mut buf) {
            if n > 0 {
                all.push_str(&String::from_utf8_lossy(&buf[..n]));
                if all.contains(needle) { return all; }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {needle:?}; output={all:?}");
}

#[test]
fn tui_accepts_plain_text_and_quits() {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24, cols: 80, pixel_width: 0, pixel_height: 0,
    }).unwrap();
    let mut cmd = CommandBuilder::new(cargo_bin());
    cmd.args(["tui", "--profile", "tui-e2e"]);
    cmd.env("TERM", "xterm-256color");
    cmd.env("DARIUS_TUI_TEST_MODE", "1");
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    read_until(&mut *reader, "◆ darius", Duration::from_secs(5));
    writer.write_all(b"hello world\r").unwrap();
    let output = read_until(&mut *reader, "Done", Duration::from_secs(5));
    assert!(output.contains("hello world"));
    writer.write_all(b"/quit\r").unwrap();
    assert!(child.wait().unwrap().success());
}
```

Delete the two skip-based tests from `tests/harness_e2e/src/lib.rs`.

**Step 2 — verify RED:** Run:
```bash
cargo test -p darius-cli --test tui_pty tui_accepts_plain_text_and_quits -- --nocapture
```
Expected: FAIL because the live TUI ignores ordinary text/Enter or times out waiting for `Done`.

**Step 3 — commit test only:**
```bash
git add crates/darius-cli/Cargo.toml crates/darius-cli/tests/tui_pty.rs tests/harness_e2e/src/lib.rs Cargo.lock
git commit -m "test(tui): reproduce inert plain-text submission"
```

## Task 0.3: Add reducer contract tests before implementation

**Objective:** Express the missing state transitions independently of terminal I/O.

**Files:**
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-tui/src/input.rs`
- Modify: `crates/darius-tui/src/commands.rs`

Add failing tests for exactly these behaviors:

```rust
#[test]
fn ordinary_letters_edit_the_composer() {
    let state = AppState::default();
    assert_eq!(map_key(key('q'), &state), Some(Action::Insert('q')));
    assert_eq!(map_key(key('j'), &state), Some(Action::Insert('j')));
    assert_eq!(map_key(key('k'), &state), Some(Action::Insert('k')));
}

#[test]
fn submitting_text_returns_runtime_goal_and_clears_input() {
    let mut state = AppState::default();
    state.composer.input = "hello".into();
    assert_eq!(state.reduce(Action::Submit), Some(Effect::SubmitGoal("hello".into())));
    assert!(state.composer.input.is_empty());
}

#[test]
fn slash_command_preserves_arguments() {
    assert_eq!(parse_invocation("/mode plan").unwrap().args, "plan");
    assert_eq!(parse_invocation("-memory brakes").unwrap().args, "brakes");
}
```

Run:
```bash
cargo test -p darius-tui ordinary_letters_edit_the_composer submitting_text_returns_runtime_goal_and_clears_input slash_command_preserves_arguments
```
Expected: compile/test failures because `Effect`, `reduce`, and `parse_invocation` do not exist and current mappings treat `q/j/k` as shortcuts.

Commit:
```bash
git add crates/darius-tui/src/app.rs crates/darius-tui/src/input.rs crates/darius-tui/src/commands.rs
git commit -m "test(tui): define reducer and command contracts"
```

---

# Work Unit 1 — One reducer and one input path

## Task 1.1: Introduce explicit effects and complete the reducer

**Objective:** Make every key action deterministic and testable without terminal or runtime side effects.

**Files:**
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-tui/src/lib.rs`

Add these public types (use these exact variants; do not invent parallel command types):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    SubmitGoal(String),
    ExecuteCommand(CommandInvocation),
    Interrupt,
    ResolvePermission { id: String, choice: PermissionChoice },
    Quit,
}

#[derive(Debug, Clone, Default)]
pub struct PaletteState {
    pub open: bool,
    pub selected: usize,
}
```

Extend `ComposerState` with `cursor: usize`; extend `AppState` with `palette`, `exit_requested`, `interrupt_armed`, `status_line`, and structured transcript state. Implement `AppState::reduce(Action) -> Option<Effect>` for every `Action` variant:

- `Insert` inserts at a UTF-8 character boundary.
- `/` or leading `-` can open palette without making input disappear.
- `Backspace` removes the previous Unicode scalar.
- `Submit` returns `SubmitGoal` for ordinary text and `ExecuteCommand` for slash/dash input.
- `PaletteNext/Prev/Accept` wrap against the filtered result list.
- `CycleMode`, `CycleEffort`, `Scroll`, `ToggleTool`, permission actions, `Cancel`, `Interrupt`, and `Quit` mutate state or produce one effect.
- Empty Enter is a no-op.

Run:
```bash
cargo test -p darius-tui app:: input:: commands::
```
Expected: all reducer contract tests pass.

Commit:
```bash
git add crates/darius-tui/src/app.rs crates/darius-tui/src/lib.rs
git commit -m "fix(tui): implement complete reducer effects"
```

## Task 1.2: Make ordinary typing ordinary

**Objective:** Remove modal/key conflicts that make normal prompts impossible to enter.

**Files:**
- Modify: `crates/darius-tui/src/input.rs`

Implement these rules:

```rust
match key.code {
    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
        Some(if state.running { Action::Interrupt } else { Action::Quit })
    }
    KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => Some(Action::CycleMode),
    KeyCode::PageUp => Some(Action::Scroll(-1)),
    KeyCode::PageDown => Some(Action::Scroll(1)),
    KeyCode::Enter => Some(Action::Submit),
    KeyCode::Backspace => Some(Action::Backspace),
    KeyCode::Esc => Some(Action::Cancel),
    KeyCode::Char(c) => Some(Action::Insert(c)),
    _ => None,
}
```

Permission and palette focus remain higher-priority branches. Remove bare `q`, `j`, and `k` global shortcuts; quitting is `/quit` or Ctrl+C while idle.

Run:
```bash
cargo test -p darius-tui input::
```
Expected: all input tests pass, including typing q/j/k.

Commit:
```bash
git add crates/darius-tui/src/input.rs
git commit -m "fix(tui): allow normal prompt editing"
```

## Task 1.3: Parse full command invocations

**Objective:** Preserve command arguments and reject invalid argument combinations visibly.

**Files:**
- Modify: `crates/darius-tui/src/commands.rs`

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    pub id: CommandId,
    pub name: String,
    pub args: String,
}

pub fn parse_invocation(input: &str) -> Result<CommandInvocation, String> {
    let canonical = dash_alias_to_slash(input.trim());
    let mut parts = canonical.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default();
    let args = parts.next().unwrap_or_default().trim().to_string();
    let spec = COMMANDS.iter().find(|item| item.name == name)
        .ok_or_else(|| format!("unknown command: {name}"))?;
    if !spec.accepts_args && !args.is_empty() {
        return Err(format!("{} does not accept arguments", spec.name));
    }
    Ok(CommandInvocation { id: spec.id, name: spec.name.into(), args })
}
```

Derive `Copy`/`Clone` as needed for `CommandId`. Make `filter()` normalize a leading `/` or `-` so palette matching works for both aliases.

Run:
```bash
cargo test -p darius-tui commands::
```
Expected: slash, dash, args, unknown, no-args validation, and filtering tests pass.

Commit:
```bash
git add crates/darius-tui/src/commands.rs crates/darius-tui/src/app.rs
git commit -m "fix(tui): parse executable command invocations"
```

---

# Work Unit 2 — Make the live renderer use the implemented design

## Task 2.1: Move transcript view types into app state

**Objective:** Ensure live `UiEvent`s produce the same structured items used by rendering and snapshots.

**Files:**
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-tui/src/render.rs`
- Modify: `crates/darius-tui/src/lib.rs`

Move `TranscriptItem`, `ToolView`, `DiffView`, `DiffLineView`, `DiffLineKind`, `TaskDisplay`, and `TaskStatus` from `render.rs` into `app.rs`. Change `AppState.messages: Vec<String>` and `tasks: Vec<String>` into:

```rust
pub transcript: Vec<TranscriptItem>,
pub tasks: Vec<TaskDisplay>,
```

Update `apply_event()` so:

- `UserMessage` appends one `TranscriptItem::User`.
- consecutive `AssistantDelta` events coalesce into one assistant item.
- `Thinking`, `ToolStart`, `ToolEnd`, `Diff`, and `TaskBoard` update their corresponding structured items.
- `PermissionRequired` sets `permission` and does not depend on the unused `permission_queue` focus check.
- `Status`, `Accept`, and errors become visible assistant/status items.
- `Done` clears `running` and updates `status_line` to `Done`.

Add tests for delta coalescing, tool start/end pairing, task status mapping, diff mapping, permission focus, and Done.

Run:
```bash
cargo test -p darius-tui app::tests::apply_event
```
Expected: all event-to-view tests pass.

Commit:
```bash
git add crates/darius-tui/src/app.rs crates/darius-tui/src/render.rs crates/darius-tui/src/lib.rs
git commit -m "refactor(tui): make transcript state canonical"
```

## Task 2.2: Replace the old live dashboard draw path

**Objective:** Render the actual welcome/transcript/palette/permission/composer interface instead of the stale four-panel demo.

**Files:**
- Modify: `crates/darius-tui/src/render.rs:449-522`
- Update snapshots under: `crates/darius-tui/src/snapshots/`

Replace `draw()` with a single-canvas layout:

1. Optional 6-row welcome card only while the transcript has no user turn.
2. Scrollable transcript consuming all flexible rows.
3. Palette above composer only while `state.palette.open`.
4. Permission chooser above composer and taking focus while active.
5. Fixed 5-row dual-rule composer at the bottom.
6. Cursor placed at `composer_area.x + 2 + display_width(input[..cursor])`.

Use `Theme::detect()` rather than hard-coded truecolor. Use `unicode_width::UnicodeWidthStr` for cursor and clipping. The only bordered boxes may be the welcome card, palette, and permission chooser; remove Header/Stream/Tasks/Input dashboard borders.

Add/refresh snapshots for 60×16, 80×24, and 140×40 with:

- empty launch;
- typed ordinary prompt;
- running turn with tasks/tools;
- palette selection;
- permission prompt;
- long Unicode text;
- error status.

Run:
```bash
INSTA_UPDATE=always cargo test -p darius-tui render::tests
cargo test -p darius-tui render::tests
```
Expected: snapshots stable with zero `.snap.new` files.

Commit:
```bash
git add crates/darius-tui/src/render.rs crates/darius-tui/src/snapshots
git commit -m "fix(tui): render functional single-canvas interface"
```

---

# Work Unit 3 — Connect cognitive execution to reusable runtime events

## Task 3.1: Make model ownership reusable across turns

**Objective:** Allow one TUI session to submit multiple goals without consuming and losing its model.

**Files:**
- Modify: `crates/darius-cognitive/src/lib.rs`
- Update callers in: `crates/darius-cli/src/lib.rs`, `tests/harness_e2e/src/lib.rs`

Change:

```rust
model: Box<dyn Model>
```

to:

```rust
model: &mut dyn Model
```

in `CognitiveLoop::run` and `run_loop`. Update tests to create `let mut model = MockModel::new(...)` and pass `&mut model`. Add a regression test that calls the loop twice with the same model object and receives a terminal `Done` event for each turn.

Run:
```bash
cargo test -p darius-cognitive
cargo test -p harness_e2e cognitive_integration
```
Expected: both pass.

Commit:
```bash
git add crates/darius-cognitive/src/lib.rs crates/darius-cli/src/lib.rs tests/harness_e2e/src/lib.rs
git commit -m "refactor(cognitive): reuse model across TUI turns"
```

## Task 3.2: Add an event sink and cancellation hook

**Objective:** Let the live worker forward progress immediately and stop safely.

**Files:**
- Modify: `crates/darius-cognitive/src/lib.rs`

Add:

```rust
pub trait EventSink: Send + Sync {
    fn emit(&self, event: UiEvent);
}

pub trait RunControl: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn approve_tool(&self, call: &darius_tools::ToolCall, risk: darius_tools::ToolRisk)
        -> Result<PermissionChoice, CognitiveError>;
}
```

Provide `NoopRunControl` for CLI tests and a channel-backed sink implementation. Replace the private discarded sender with `Arc<dyn EventSink>`. Check `is_cancelled()` before planning, before each ReAct iteration, and before each tool. Add `CognitiveError::Cancelled`, emit `Status { line: "Interrupted" }`, then `Done` on cancellation.

RED tests:

- sink receives Header/TaskBoard/Tool/Accept/Done as they happen;
- cancellation before a tool prevents handler execution;
- cancellation always ends with Done;
- two turns do not leak events into each other.

Run:
```bash
cargo test -p darius-cognitive event_sink cancellation
```
Expected: pass.

Commit:
```bash
git add crates/darius-cognitive/src/lib.rs crates/darius-cognitive/src/ui_events.rs
git commit -m "feat(cognitive): expose live events and cancellation"
```

## Task 3.3: Add `SessionRuntime::run_goal`

**Objective:** Make the runtime object actually execute work and publish canonical events.

**Files:**
- Modify: `crates/darius-cli/src/runtime.rs`

Implement:

```rust
impl SessionRuntime {
    pub fn run_goal(
        &mut self,
        goal: &str,
        mode: &str,
        sink: std::sync::Arc<dyn darius_cognitive::EventSink>,
        control: std::sync::Arc<dyn darius_cognitive::RunControl>,
    ) -> Result<darius_cognitive::Acceptance, darius_cognitive::CognitiveError> {
        self.metadata.mode = mode.to_string();
        let loop_ = darius_cognitive::CognitiveLoop::with_sink(sink, control);
        let (_, acceptance) = loop_.run(
            &self.metadata,
            &self.policy,
            goal,
            self.model.as_mut(),
            &mut self.tools,
            &self.memory,
        )?;
        Ok(acceptance)
    }
}
```

Do not keep a second broadcast sender inside `SessionRuntime` unless it is the sink used by `run_goal`. Add tests for offline goal execution, repeated turns, configured missing-key error, and exact metadata.

Run:
```bash
cargo test -p darius-cli runtime::tests
```
Expected: pass.

Commit:
```bash
git add crates/darius-cli/src/runtime.rs
git commit -m "feat(runtime): execute reusable goals with live events"
```

---

# Work Unit 4 — Wire CLI worker, terminal event loop, and real commands

## Task 4.1: Define one controller protocol

**Objective:** Make TUI-to-runtime messages complete and eliminate duplicate command dispatch.

**Files:**
- Modify: `crates/darius-tui/src/controller.rs`
- Modify: `crates/darius-tui/src/lib.rs`

Use this protocol:

```rust
pub enum RuntimeCommand {
    SubmitGoal { text: String, mode: Mode, effort: Effort },
    ExecuteSlash(CommandInvocation),
    ResolvePermission { id: String, choice: PermissionChoice },
    Interrupt,
    Shutdown,
}

pub struct TuiController {
    pub commands: tokio::sync::mpsc::Sender<RuntimeCommand>,
    pub events: tokio::sync::broadcast::Receiver<UiEvent>,
}
```

Remove `ParsedCommand`; use canonical `CommandInvocation`. Add channel tests for every command variant and lag/closed-channel handling.

Run:
```bash
cargo test -p darius-tui controller::
```
Expected: pass.

Commit:
```bash
git add crates/darius-tui/src/controller.rs crates/darius-tui/src/lib.rs
git commit -m "refactor(tui): define one runtime controller protocol"
```

## Task 4.2: Replace the hard-coded terminal loop

**Objective:** Route every key through `map_key` → `reduce` → runtime effect and consume live events without blocking redraws.

**Files:**
- Modify: `crates/darius-tui/src/terminal.rs`
- Modify: `crates/darius-tui/src/lib.rs`

Change the entry point to:

```rust
pub fn run_tui(mut state: AppState, mut controller: TuiController) -> io::Result<()>;
```

The loop must:

1. Draw.
2. Drain `controller.events.try_recv()` and call `state.apply_event(event)`.
3. Poll crossterm with `poll(Duration::from_millis(25))`, not blocking `read()` indefinitely.
4. Ignore key-release events; process press/repeat only.
5. Call `map_key(key, &state)` then `state.reduce(action)`.
6. Send corresponding `RuntimeCommand` using `blocking_send` or a runtime handle.
7. Break only on `Effect::Quit`, controller closure after a visible error, or fatal terminal error.
8. Restore terminal through `TerminalGuard` on normal exit, panic, channel failure, and Ctrl+C.

Delete the entire command `match cmd.as_str()` block from `terminal.rs`. There must be one command executor in the CLI worker.

Add a test backend/event-source harness proving normal input, resize redraw, event consumption, command effect, permission effect, and shutdown.

Run:
```bash
cargo test -p darius-tui terminal::
```
Expected: pass.

Commit:
```bash
git add crates/darius-tui/src/terminal.rs crates/darius-tui/src/lib.rs
git commit -m "fix(tui): drive terminal through reducer and controller"
```

## Task 4.3: Build and own the runtime in `cmd_tui`

**Objective:** Make `darius tui --profile NAME` start a functional worker and surface startup errors.

**Files:**
- Modify: `crates/darius-cli/src/lib.rs:24-38,356-372`
- Create: `crates/darius-cli/src/tui_runtime.rs`
- Modify: `crates/darius-cli/Cargo.toml`

Change dispatch to:

```rust
"tui" => cmd_tui(&args[2..]),
```

`cmd_tui(args)` must parse `--profile` and optional `--cwd`, build `SessionRuntime::from_profile`, initialize `AppState.profile/model`, create the controller channels, spawn one named worker thread, and call `darius_tui::run_tui(state, controller)`. The worker owns `SessionRuntime`; it handles commands serially and uses `spawn_blocking` only around synchronous model/tool execution.

Startup failure behavior:

- missing configured API key: print exact env variable and exit nonzero before alternate-screen entry;
- invalid profile/path: print error and exit nonzero;
- no config: show `mock` badge and remain functional.

Add CLI integration tests for `--profile`, missing key, mock startup, and worker shutdown.

Run:
```bash
cargo test -p darius-cli tui_runtime
```
Expected: pass.

Commit:
```bash
git add crates/darius-cli/src/lib.rs crates/darius-cli/src/tui_runtime.rs crates/darius-cli/Cargo.toml
git commit -m "feat(cli): wire TUI to live session runtime"
```

## Task 4.4: Implement local commands

**Objective:** Make commands that only affect the UI/session state execute synchronously and predictably.

**Files:**
- Modify: `crates/darius-cli/src/tui_runtime.rs`
- Modify: `crates/darius-tui/src/app.rs`

Implement:

- `/help`: emit complete command + shortcut help.
- `/clear`: clear transcript, keep welcome/status.
- `/mode [auto|manual|accept-edits|plan]`: validate and set mode; no arg cycles.
- `/effort [low|medium|high|xhigh|max|ultracode]`: validate/set; no arg reports current.
- `/plan`: set plan mode.
- `/tasks`: render current task state (or `No active tasks`).
- `/stop`: cancel active turn; no active turn emits `Nothing is running`.
- `/quit`: graceful Shutdown then exit.
- `/permissions`: show current session allow-list and policy.

For every invalid argument, emit `UiEvent::Status` with exact usage; never silently ignore.

Add one test per command and one table-driven test asserting every local command returns a terminal status/effect.

Run:
```bash
cargo test -p darius-cli tui_runtime::tests::local_command
```
Expected: pass.

Commit:
```bash
git add crates/darius-cli/src/tui_runtime.rs crates/darius-tui/src/app.rs
git commit -m "feat(tui): execute local slash commands"
```

## Task 4.5: Implement runtime-backed commands

**Objective:** Make every remaining visible menu row perform real work or return a truthful error.

**Files:**
- Modify: `crates/darius-cli/src/tui_runtime.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cli/src/config.rs`
- Modify: `crates/darius-web/src/lib.rs`

Implement exact behavior:

- `/memory [query]`: no query returns count/path; query returns top memory matches.
- `/pack`: emits bounded MemoryPack text and record count.
- `/compact`: stores a bounded transcript summary as a memory note, replaces old visible turns with one compact marker, and reports saved record ID. Never call a model solely for compaction.
- `/status`: profile, model, mode, effort, cwd, running state, memory count, server state.
- `/config`: effective config with API key value redacted; show env variable name only.
- `/skills [query]`: list/search installed SKILL.md metadata using existing skills crate; if external `npx skills find` integration is not present, do not claim it is.
- `/model`: show active model; `/model mock` selects fresh offline model; `/model <configured-name>` reinitializes the configured provider or emits missing-key/unknown-model error.
- `/a2a`: emit real `darius_web::agent_card()` and active task states.
- `/serve [port]`: start Axum on `127.0.0.1`, default port 3000; repeated calls report existing URL; invalid/busy port emits error. Bind LAN only with explicit future flag—not in this repair.

Refactor `darius-web::create_router` as needed so `/api/goal` sends the same `RuntimeCommand::SubmitGoal` path rather than synthetic Header/Status/Done events. Change the hard-coded dashboard version to `env!("CARGO_PKG_VERSION")`.

Add command tests with a temporary profile and server tests on `127.0.0.1:0`.

Run:
```bash
cargo test -p darius-cli tui_runtime::tests::runtime_command
cargo test -p darius-web
```
Expected: all pass.

Commit:
```bash
git add crates/darius-cli/src/tui_runtime.rs crates/darius-cli/src/runtime.rs crates/darius-cli/src/config.rs crates/darius-web/src/lib.rs
git commit -m "feat(tui): execute memory model skills A2A and serve commands"
```

---

# Work Unit 5 — Real permission gating and safe tool boundaries

## Task 5.1: Add tool risk metadata

**Objective:** Classify tools before execution instead of inferring danger from names in the UI.

**Files:**
- Modify: `crates/darius-tools/src/lib.rs`

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk { ReadOnly, Mutating, Shell }

struct RegisteredTool {
    risk: ToolRisk,
    handler: ToolHandler,
}
```

Change `register()` to accept risk (or add `register_with_risk` and make the existing method default to read-only only for internal tests). Classify:

- `memory_search`, `read_file`, `glob`, task list: `ReadOnly`;
- `memory_remember`, task mutations, `write_file`: `Mutating`;
- `shell`: `Shell`.

Expose `ToolRegistry::risk(name) -> Option<ToolRisk>`.

Add tests covering every builtin and asserting no registered tool lacks risk metadata.

Run:
```bash
cargo test -p darius-tools tool_risk
```
Expected: pass.

Commit:
```bash
git add crates/darius-tools/src/lib.rs
git commit -m "feat(tools): classify tool execution risk"
```

## Task 5.2: Gate mutating and shell tools before execution

**Objective:** Make the visible permission chooser block the actual operation until the user decides.

**Files:**
- Modify: `crates/darius-cognitive/src/lib.rs`
- Modify: `crates/darius-cli/src/tui_runtime.rs`
- Modify: `crates/darius-tui/src/app.rs`

Worker behavior:

1. Cognitive loop asks `RunControl::approve_tool(call, risk)` before `tools.execute` for `Mutating` and `Shell`.
2. Channel-backed control emits `UiEvent::PermissionRequired` with ID/title/command/reason and blocks only the worker thread on a one-shot response.
3. TUI remains responsive; arrow keys select and Enter sends `ResolvePermission`.
4. `AllowOnce` executes once.
5. `AllowSession` caches `(tool_name, normalized target)` for this TUI session only.
6. `Deny` emits `PermissionResolved`, a failed `ToolEnd`, and allows model execution to continue with the denial evidence.
7. `/stop` and shutdown release any blocked approval wait with cancellation.

Add tests with handlers that increment an atomic counter. Assert counter remains zero before approval/after denial and becomes one after approval; assert session approval suppresses the second prompt but not a different target.

Run:
```bash
cargo test -p darius-cognitive permission
cargo test -p darius-cli permission
cargo test -p darius-tui permission
```
Expected: pass.

Commit:
```bash
git add crates/darius-cognitive/src/lib.rs crates/darius-cli/src/tui_runtime.rs crates/darius-tui/src/app.rs
git commit -m "feat(tui): gate real tools through permission chooser"
```

## Task 5.3: Constrain coding tools to the TUI working directory

**Objective:** Prevent approved file operations from escaping the selected workspace accidentally.

**Files:**
- Modify: `crates/darius-tools/src/lib.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cli/src/lib.rs`

Change `register_coding_builtins` to receive canonical `workspace_root: &Path`. Resolve relative paths beneath it; reject `..`, symlink escape, and absolute paths outside it for read/write/glob. Run shell with `.current_dir(workspace_root)`. Keep shell permission-required because shell commands can still reference external paths.

Tests:

- relative read/write succeeds;
- `../outside` fails;
- absolute outside fails;
- symlink escape fails;
- shell sees expected `pwd`;
- `--cwd` changes root and invalid cwd fails before terminal mode.

Run:
```bash
cargo test -p darius-tools workspace_boundary
cargo test -p darius-cli cwd
```
Expected: pass.

Commit:
```bash
git add crates/darius-tools/src/lib.rs crates/darius-cli/src/runtime.rs crates/darius-cli/src/lib.rs
git commit -m "fix(tools): enforce TUI workspace boundary"
```

---

# Work Unit 6 — Honest end-to-end verification

## Task 6.1: Complete PTY happy-path coverage

**Objective:** Prove the real binary works as a user experiences it.

**Files:**
- Modify: `crates/darius-cli/tests/tui_pty.rs`

Add independent PTY tests (one behavior each):

1. `tui_accepts_plain_text_and_quits` — type `qjk hello`, Enter; assert user text, task/tool progress, acceptance, Done; `/quit`; exit 0.
2. `tui_help_and_dash_alias_execute` — `/help`; assert `/model` and `/quit`; `-status`; assert profile/model/cwd.
3. `tui_mode_effort_and_palette_work` — `/mode plan`, `/effort max`, `/` + arrows + Enter; assert selected values.
4. `tui_permission_blocks_then_allows` — use a deterministic test model requesting `write_file`; assert file absent before approval, present after `Yes`.
5. `tui_permission_deny_prevents_write` — assert absent after Deny.
6. `tui_ctrl_c_interrupts_then_exits` — first Ctrl+C interrupts active turn; second while idle exits 0.
7. `tui_missing_key_fails_before_raw_mode` — temporary configured profile with missing key; assert nonzero and readable error.
8. `tui_unicode_resize_and_terminal_restore` — resize PTY and type wide characters; assert no panic and normal shell output works after exit.

Never `return` or print `SKIP` when binary is missing. Missing `CARGO_BIN_EXE_darius` must fail test setup.

Run:
```bash
cargo test -p darius-cli --test tui_pty -- --nocapture
```
Expected: all tests pass under 30 seconds total.

Commit:
```bash
git add crates/darius-cli/tests/tui_pty.rs
git commit -m "test(tui): prove real interactive workflows"
```

## Task 6.2: Verify runtime event ordering and repeated turns

**Objective:** Prove controller/runtime/cognitive integration without relying only on terminal snapshots.

**Files:**
- Create: `crates/darius-cli/tests/tui_runtime_integration.rs`

With a temporary profile and deterministic model, assert:

- SubmitGoal emits Header → UserMessage → TaskBoard → ToolStart → PermissionRequired (when applicable) → ToolEnd → Accept → Done.
- a second SubmitGoal works after the first Done.
- Stop ends only the current turn.
- model/tool errors emit visible Status + Done and leave worker alive.
- Shutdown joins worker and closes channels.
- web `/api/goal` and TUI SubmitGoal traverse the same worker command path.

Run:
```bash
cargo test -p darius-cli --test tui_runtime_integration -- --nocapture
```
Expected: pass.

Commit:
```bash
git add crates/darius-cli/tests/tui_runtime_integration.rs
git commit -m "test(tui): verify shared runtime event ordering"
```

## Task 6.3: Remove false-positive tests and dead code

**Objective:** Ensure the suite cannot pass while production wiring is disconnected again.

**Files:**
- Modify: `tests/harness_e2e/src/lib.rs`
- Modify: `crates/darius-tui/src/terminal.rs`
- Modify: `crates/darius-tui/src/render.rs`
- Modify: `crates/darius-tui/src/controller.rs`

Delete:

- binary tests that skip;
- old hard-coded terminal command loop;
- old four-panel live renderer;
- unused `permission_queue` if canonical `permission`/broker supersedes it;
- duplicate command structs;
- dead event sender fields;
- `#![allow(dead_code, unused_imports)]` hiding stale harness code where practical.

Add a static contract test or targeted search assertion documenting that production `run_tui` calls `map_key`, `reduce`, and consumes `controller.events`; prefer behavioral tests over source-string tests.

Run:
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: zero warnings, zero skipped TUI tests, zero failures.

Commit:
```bash
git add tests/harness_e2e/src/lib.rs crates/darius-tui/src
git commit -m "test(tui): remove false-green and dead paths"
```

---

# Work Unit 7 — Documentation, manual critique, and release readiness

## Task 7.1: Update honest usage documentation

**Objective:** Make documented interactions match verified behavior exactly.

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

Document:

- `cargo run -p darius-cli -- tui --profile default --cwd .`;
- plain-text goal submission;
- `/` and `-` command aliases;
- full command table with exact args and outputs;
- Ctrl+C interrupt/exit semantics;
- permission choices and session scope;
- configured-provider missing-key behavior;
- offline mock behavior;
- localhost-only `/serve`;
- workspace write boundary;
- no claim that a command works unless covered by PTY/runtime integration tests.

Add an Unreleased `1.1.2` changelog section; do not change version/tag in this task.

Run examples against a disposable profile and capture exit status:
```bash
cargo run -q -p darius-cli -- --version
cargo run -q -p darius-cli -- session-smoke --profile docs-smoke
cargo run -q -p darius-cli -- a2a card
```
Expected: exit 0.

Commit:
```bash
git add README.md CHANGELOG.md
git commit -m "docs: document functional TUI behavior"
```

## Task 7.2: Manual terminal critique

**Objective:** Catch real-terminal issues that TestBackend and snapshots miss.

**Files:** none unless defects are found.

Run:
```bash
cargo run -p darius-cli -- tui --profile manual-smoke --cwd /Users/cmd/workspace/Darius
```

Test at approximately 60×16, 80×24, and 140×40:

- type sentences containing `q`, `j`, `k`, `/`, emoji, and CJK;
- backspace/cursor behavior;
- submit two goals;
- open/filter/navigate palette;
- execute every visible command;
- approve once, approve session, deny;
- interrupt a turn;
- resize during streaming;
- quit and verify cursor/raw mode restored.

Any defect requires a failing automated regression test, focused fix, and its own `fix(tui): ...` commit. Do not accept “looks okay” without checking every row.

## Task 7.3: Final local gates and adversarial reviews

**Objective:** Prove functionality and safety before calling the repair complete.

Run without output-masking pipes:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p darius-cli --test tui_pty -- --nocapture
cargo test -p darius-cli --test tui_runtime_integration -- --nocapture
cargo build --release -p darius-cli
cargo run -q -p darius-cli -- --version
git diff --check
git status --short
```

Expected:

- every command exits 0;
- no `SKIP` in TUI test output;
- release binary exists;
- only intentionally uncommitted plan/changes appear, then commit them;
- no secrets in logs/snapshots.

Run two independent reviews:

1. **Spec review:** verify every command in `COMMANDS` has a tested executor and every acceptance criterion above maps to code + test.
2. **Code-quality/security review:** inspect terminal restoration, worker lifetime, cancellation, permission deadlocks, shell/write boundaries, secret redaction, channel lag, Unicode, and panic paths.

Any FAIL creates a test-first fix and reruns all gates. Timeout/inconclusive review is rerun, never counted as PASS.

## Task 7.4: Prepare (but do not publish) a patch release candidate

**Objective:** Leave a clean, verifiable candidate without taking unrequested external actions.

**Files:**
- Modify only after user approval: `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`

If the user asks to release:

1. Bump workspace version to `1.1.2`.
2. Run all Task 7.3 gates again.
3. Commit `chore: version 1.1.2`.
4. Push `master` only when explicitly requested.
5. Wait for exact-HEAD CI success.
6. Tag and publish only after exact-SHA CI is green.
7. Verify every expected archive and checksum by downloading one locally.

Without explicit release instruction, stop after a clean local candidate and report the exact HEAD plus verification results.

---

# Acceptance criteria

The repair is complete only when all are true:

- A user can type arbitrary ordinary text, including q/j/k and Unicode, then press Enter to run it.
- The TUI receives real model/cognitive/tool events while the turn runs and remains responsive.
- Multiple goals work in one session.
- Every command in `COMMANDS` has an executor test and produces real output or a truthful visible error.
- `/serve` starts a real localhost server and web goals use the shared worker path.
- Mutating/shell tools cannot execute before permission resolution; deny and cancellation prevent side effects.
- File tools respect the chosen working-directory boundary.
- Ctrl+C interrupts an active turn and exits only when idle; `/quit` exits cleanly.
- Terminal state restores after normal exit, startup error, panic, and interrupt.
- No TUI E2E test skips or returns early.
- PTY, runtime integration, snapshots, full workspace tests, fmt, clippy, and release build all pass.
- Documentation contains no unsupported capability claims.

# Risks and tradeoffs

- **Synchronous model/tool APIs:** Keep them on a worker thread; do not block the terminal draw/input loop. Converting the entire cognitive stack to async is unnecessary for this repair.
- **Permission wait deadlocks:** Approval waits must observe cancellation/shutdown and have deterministic channel closure behavior. Tests must cover quitting while a prompt is open.
- **Tokio vs std channels:** Use Tokio for TUI/controller events already present; use blocking/oneshot mechanisms only inside the worker. Do not add another global bus.
- **Model reuse:** Changing `Box<dyn Model>` to `&mut dyn Model` touches callers but is smaller and safer than rebuilding providers every turn.
- **Mock behavior:** Mock proves plumbing, not intelligence. The UI must label it `mock` and live-provider tests must use a local HTTP fixture, never a real billable API.
- **Shell safety:** Working-directory constraints do not sandbox shell commands. Permission gating is mandatory; stronger sandboxing is outside this repair.
- **Command breadth:** Implement the existing 18 rows; do not add more commands until all existing ones work.
- **Versioning:** Current `v1.1.1` claims functionality that is absent. Correct code first; publish `v1.1.2` only under explicit user instruction and exact-SHA green CI.

# Open questions resolved by default

- “Everything” means every currently visible TUI interaction and command, not every crate feature in Darius.
- Bare `q/j/k` are text, not global shortcuts.
- `/quit` or idle Ctrl+C exits; active Ctrl+C interrupts.
- `--profile` and `--cwd` are supported by `darius tui`.
- Server binds only to `127.0.0.1`.
- Session permission grants are memory-only and vanish on exit.
- Offline mock remains the no-config default; configured-but-broken provider fails visibly.
