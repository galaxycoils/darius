# Darius Harness Startup + Functional Recovery Plan

> **For Hermes:** Execute with `software-development:subagent-driven-development`. Every behavior change uses strict RED → GREEN → REFACTOR. Diagnose unexpected failures with `software-development:systematic-debugging`. Extract only cohesive touched responsibilities when needed for ownership/testability; do not enlarge or fragment unrelated legacy modules. Do not push, install into user PATH, tag, or publish without explicit approval.

## Goal

Make `darius` launch directly into a reliable coding-agent TUI—like Claude Code or Codex—with working first-run setup, live OpenAI-compatible inference, multi-turn context, safe coding tools, permissions, cancellation, truthful commands, terminal cleanup, and verified installation artifacts.

## Current context / assumptions

- Canonical root: `/Users/cmd/workspace/Darius`; branch `master`; implementation baseline `de2e37f55038a8c01a47f552c217b70377035f89`. Workspace version is `1.2.0`; latest existing tag is `v1.1.2`. Before WU0, abort if `git rev-parse HEAD` differs or `git status --porcelain --untracked-files=no` is nonempty. The plan file may remain untracked.
- `/Users/cmd/workspace/2026-08-26-darius-v3.md` remains architecture source. This plan repairs its user-facing startup/runtime path only.
- Proven root causes at baseline:
  - no args prints incomplete help instead of opening TUI: `crates/darius-cli/src/lib.rs:23-81`;
  - runtime blocks inside same loop that must receive permission/interrupt commands: `crates/darius-cli/src/tui_runtime.rs:155-189`;
  - cancellation token is session-global and permanently poisoned after first cancel: `crates/darius-cli/src/runtime.rs:145-158`;
  - 18 palette commands only echo `Command: ...`: `crates/darius-cli/src/tui_runtime.rs:171-175`;
  - cognitive loop has no valid multi-turn/tool-result conversation: `crates/darius-cognitive/src/lib.rs:92-297`;
  - configured provider registration is ignored by hard-coded `default`/`rater` routing: `crates/darius-daemon/src/model_router.rs:225-240`;
  - live plan wrapping and forced `DONE` corrupt normal provider output: `crates/darius-daemon/src/model_router.rs:290-335`;
  - tools lack explicit workspace ownership, robust search, atomic writes, exit handling, and cancellation: `crates/darius-tools/src/lib.rs:437-543`;
  - config parse/read errors silently become offline mock: `crates/darius-cli/src/config.rs:20-28`;
  - PTY tests do not submit a goal or prove permission/cancel/second-turn behavior: `crates/darius-cli/tests/tui_pty.rs`;
  - `serve`, web goals, and A2A are synthetic/stub paths: `crates/darius-cli/src/lib.rs`, `crates/darius-web/src/lib.rs`;
  - current v1.2.0 also exposes `cron`, `approval-check`, `peer_send`, MCP, subagent, worktree/rollback, and peer-A2A surfaces without proving the core startup path; recovery must close these from model/public reach;
  - installer asks for `darius-darwin-*`, while release publishes `darius-macos-*`; `v1.1.2` release lacks `install.sh` and most matrix assets.
- Context7 references checked for Clap and Ratatui. `Option<Subcommand>` supports intentional no-subcommand dispatch; terminal setup must have explicit restoration and resize handling.
- Deterministic integration uses a local fake OpenAI-compatible server. No real secret is needed. Production credential smoke remains optional and secret-safe.

## Architecture / proposed approach

Use one Tokio runtime actor that owns `SessionRuntime`, receives typed commands continuously, and spawns at most one cancellation-aware turn task. Session state owns conversation, task board, and approval cache; each turn owns a fresh token/deadline. Provider and shell operations receive that context, so Ctrl+C can abort in-flight work within two seconds. Hide unsupported web/A2A/legacy modes instead of blocking core startup recovery or emitting synthetic success.

## Scope commitment

### Included

- bare `darius` TTY launch;
- non-TTY help behavior;
- typed Clap surface and complete disposition tests;
- one `DariusPaths`/workspace authority;
- honest setup/live/offline-demo states;
- one valid OpenAI-compatible text/tool-call protocol;
- multi-turn context and correlated tool results;
- read/search/write/shell tools with workspace containment;
- Auto + Plan execution policies only;
- session permissions, responsive denial/cancel, recovery after cancellation;
- functional visible slash commands;
- TUI input/scroll/paste/resize/cleanup;
- PTY full journey, installer/release workflow integrity, truthful docs.

### Explicitly hidden/unavailable in this recovery

- `serve`, `/serve`, `a2a`, `/a2a`, dashboard goal execution, peer A2A;
- legacy `daemon`, top-level `status`, `start`, `stop`, `attach`, `eval`, `learn`, `session-smoke`, `cron`, `approval-check`, and synthetic `help` subcommand;
- Manual/Accept Edits modes, `/effort`, `/skills`, and `doctor`;
- `peer_send`, MCP-discovered tools, subagent controls, worktree/rollback tools, browser tool, messaging, remote sandboxes, autonomous skill mutation, Anthropic-native wire format, Windows release.

Their source may remain for compatibility, but generated help/palette/capability claims must not advertise them as working. Track server/A2A as a separate follow-up plan after core recovery passes.

## Public CLI disposition

| Current surface | Recovery state | Proof |
|---|---|---|
| no args | TTY opens TUI; non-TTY prints help, exit 0 | PTY + binary contract |
| `tui` | retained | PTY journey |
| `run <goal...>` | retained, read-only by default for non-TTY | fake-provider E2E |
| `config show/init` | retained; init requires provider/base-url/model/key-env | temp-home binary tests |
| `memory search <query...>/pack/import <file>/export <file>/stats` | retained | table-driven binary tests |
| `--profile`, `--cwd`, `--offline`, `--help`, `--version` | retained global flags before/after subcommand where Clap permits | table-driven tests |
| `daemon` | removed, exit 2 | negative binary test |
| `status` | removed, exit 2; `/status` remains | negative binary test |
| `start`, `stop`, `attach`, `eval`, `learn`, `session-smoke` | removed, exit 2 | one negative test/token |
| `serve`, `a2a`, `cron`, `approval-check`, `help` | removed, exit 2; Clap help subcommand disabled | one negative test/token |
| invalid command/args | Clap exit 2 | binary contract |
| runtime/provider failure | exit 1, sanitized message | E2E error tests |

---

# Work Unit 0 — Lock baseline failures and truth boundary

## Task 0.1 — Create RED-only public contract tests

**Files**
- Create: `crates/darius-cli/tests/cli_contract.rs`
- Modify: `crates/darius-cli/Cargo.toml`
- Modify: `Cargo.lock`

Add `assert_cmd = "2"`, `predicates = "3"`, and `tempfile = "3"` under CLI dev-dependencies. Tests must assert:

```rust
#[test]
fn public_help_matches_recovery_surface() {
    let help = run(["--help"]);
    assert_success(&help);
    for shown in ["tui", "run", "config", "memory"] {
        assert!(stdout(&help).contains(shown));
    }
    for hidden in ["daemon", "status", "start", "stop", "attach", "eval", "learn", "session-smoke", "serve", "a2a", "cron", "approval-check", "help"] {
        assert!(!stdout(&help).contains(hidden));
    }
}

#[test]
fn unknown_command_exits_two() {
    assert_eq!(run(["wat"]).status.code(), Some(2));
}
```

Also invoke every removed token and require exit 2, test `--version`, global flags before/after retained subcommands, malformed nested arguments, and no-arg non-TTY help. Keep this task RED-only.

Run: `cargo test -p darius-cli --test cli_contract -- --nocapture`

Expected RED: incomplete/manual help, hidden stubs still dispatched, wrong parse/exit behavior.

Commit test only:

```bash
git add crates/darius-cli/Cargo.toml crates/darius-cli/tests/cli_contract.rs Cargo.lock
git commit -m 'test(cli): expose broken public contract'
```

## Task 0.2 — Replace false PTY smoke with clean-home launch reproduction

**Files**
- Replace: `crates/darius-cli/tests/tui_pty.rs`

Build shared PTY helper with:
- `CARGO_BIN_EXE_darius` only—never skip;
- temp `DARIUS_HOME` and workspace;
- bounded `read_until` and child kill-on-drop;
- ANSI stripping only for assertions;
- no writes under real `~/.darius` (snapshot real path existence/metadata before and after).

First RED test launches **bare `darius`** with empty temp home and no key, expects setup guidance, sends `/quit`, and requires exit 0 + cursor/shell restoration within 5 seconds.

Run: `cargo test -p darius-cli --test tui_pty clean_home_bare_launch -- --nocapture --test-threads=1`

Expected RED: bare invocation prints help/exits instead of opening setup TUI.

Commit test only:

```bash
git add crates/darius-cli/tests/tui_pty.rs
git commit -m 'test(tui): reproduce broken bare launch'
```

## Task 0.3 — Create capability inventory as documentation only

**Files**
- Create: `docs/CAPABILITIES.md`

List every CLI command, nested command, global flag, slash command, mode, provider, tool, server/A2A route, installer target, and machine-visible claim as `verified`, `experimental`, or `unavailable`. Link each future `verified` row to an exact test name; initially mark unproven rows `unavailable`. This task does not claim GREEN for CLI tests.

Verification: `test -s docs/CAPABILITIES.md`

Expected: exit 0.

Commit: `git add docs/CAPABILITIES.md && git commit -m 'docs: inventory Darius capability truth'`

---

# Work Unit 1 — Establish paths, config, and typed startup

## Task 1.1 — Add one Darius path authority

**Files**
- Create: `crates/darius-cli/src/paths.rs`
- Modify: `crates/darius-cli/src/lib.rs`

Canonical API:

```rust
#[derive(Clone, Debug)]
pub struct DariusPaths {
    pub home: std::path::PathBuf,
    pub workspace: std::path::PathBuf,
}

impl DariusPaths {
    pub fn resolve(env: &dyn Env, cwd: Option<&std::path::Path>) -> Result<Self, PathError>;
    pub fn profile(&self, name: &str) -> Result<std::path::PathBuf, PathError>;
}
```

Rules: `DARIUS_HOME` override, else platform home + `.darius`; canonical workspace; validate profile `[A-Za-z0-9_-]+`; reject missing/non-directory cwd and profile traversal. Never call `set_current_dir`. Thread `DariusPaths` into later constructors.

RED test: override points to temp; profile traversal rejected; cwd remains unchanged.

Run RED: `cargo test -p darius-cli paths_ -- --nocapture`

Expected RED: module/API absent.

Implement minimally; run GREEN: same command, expected pass.

Commit: `git add crates/darius-cli/src && git commit -m 'feat(paths): centralize home profile and workspace roots'`

## Task 1.2 — Make config typed, explicit, and atomic

**Files**
- Replace touched behavior in: `crates/darius-cli/src/config.rs`
- Create: `crates/darius-cli/src/config_error.rs`
- Create: `crates/darius-cli/src/config_init.rs`
- Modify: `crates/darius-cli/Cargo.toml`
- Modify: `Cargo.lock`

Use `DariusPaths`; `load(paths, profile) -> Result<ProfileConfig, ConfigError>`. Missing file is default setup state. Invalid TOML, unreadable file, empty provider/model, invalid URL scheme, or invalid env-variable name is visible error. `config init` writes provider metadata only, mode 0600 on Unix, same-directory temp + atomic rename, refuses overwrite unless `--force`, and has no raw-key argument.

RED tests: malformed config no longer falls back; config init contains no secret; overwrite rejected; mode 0600; temp home isolated.

Run RED: `cargo test -p darius-cli config_ -- --nocapture`

Expected RED: silent default and missing init.

Implement; run GREEN: same command.

Commit: `git add crates/darius-cli/src crates/darius-cli/Cargo.toml Cargo.lock && git commit -m 'fix(config): load and initialize profiles safely'`

## Task 1.3 — Replace manual argv parsing with complete Clap surface

**Files**
- Create: `crates/darius-cli/src/args.rs`
- Modify: `crates/darius-cli/src/lib.rs`
- Modify: `crates/darius-cli/src/main.rs`
- Modify: `crates/darius-cli/tests/cli_contract.rs`
- Modify: `crates/darius-cli/Cargo.toml`
- Modify: `Cargo.lock`

Exact public shape:

```rust
#[derive(clap::Parser)]
#[command(name = "darius", version, about = "Local-first coding agent", disable_help_subcommand = true)]
pub struct Cli {
    #[arg(long, global = true, default_value = "default")]
    pub profile: String,
    #[arg(long, global = true)]
    pub cwd: Option<std::path::PathBuf>,
    #[arg(long, global = true)]
    pub offline: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand)]
pub enum Command {
    Tui,
    Run { #[arg(required = true)] goal: Vec<String> },
    Config { #[command(subcommand)] command: ConfigCommand },
    Memory { #[command(subcommand)] command: MemoryCommand },
}
```

Define `ConfigCommand::{Show,Init{provider: String,base_url: url::Url,model: String,key_env: String,force: bool}}` with all four values required, and `MemoryCommand::{Search{query: Vec<String> /* required, 1.. */},Pack,Import{file: PathBuf},Export{file: PathBuf},Stats}` explicitly. Add direct `url` dependency. No hidden dispatch for legacy commands. `run_with(cli, IoCaps)` routes no command to TUI only when stdin+stdout are TTY; otherwise generated help exit 0.

Run RED: `cargo test -p darius-cli --test cli_contract -- --nocapture`

Expected RED from Task 0.1.

Implement; run GREEN: same command, all tests pass.

Commit: `git add crates/darius-cli/src crates/darius-cli/tests/cli_contract.rs crates/darius-cli/Cargo.toml Cargo.lock && git commit -m 'feat(cli): launch typed agent surface by default'`

## Task 1.4 — Add honest runtime selection and setup diagnostics

**Files**
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cli/src/lib.rs`
- Test: `crates/darius-cli/tests/cli_contract.rs`

Runtime states:
- `--offline`: explicit `offline-demo`, never claims real file analysis;
- valid config + key env present: exact configured provider;
- valid config + missing key: setup/error names env variable only;
- no config + `OPENAI_API_KEY` or `DARIUS_API_KEY`: in-memory OpenAI default;
- no config/key: setup TUI; submitted goal emits guidance, not fake Done.

Setup diagnostics are exposed through first-run guidance, `config show`, and `/status`: version, paths, config parse, key presence by variable name, memory open, workspace, and provider URL; never print secret values. Without explicit config, non-empty `DARIUS_API_KEY` takes precedence over non-empty `OPENAI_API_KEY`; whitespace-only values are missing. Implicit defaults: `openai_compatible`, `https://api.openai.com/v1`, `gpt-4o-mini`.

RED tests cover all selection branches plus precedence, empty values, malformed config, unwritable home, and no-secret output.

Run RED: `cargo test -p darius-cli runtime_selection_ -- --nocapture`

Expected RED: selection silently mocks and setup diagnostics are absent.

Implement; run GREEN with same command.

Commit: `git add crates/darius-cli/src crates/darius-cli/tests && git commit -m 'fix(startup): expose setup live and offline states'`

---

# Work Unit 2 — Build safe tool foundation before runtime wiring

## Task 2.1 — Give ToolRegistry explicit roots and session state

**Files**
- Modify: `crates/darius-tools/src/lib.rs`
- Create: `crates/darius-tools/src/path_policy.rs`
- Modify: `crates/darius-cli/src/lib.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cognitive/src/lib.rs`
- Modify: `crates/darius-cognitive/src/subagent.rs`
- Modify: `crates/darius-tools/src/mcp.rs`
- Modify: `tests/harness_e2e/src/lib.rs`

Introduce `ToolRegistry::new_with_roots(workspace_root, spill_dir)` while temporarily retaining `new(profile_dir)`; migrate `crates/darius-cli/src/lib.rs`, `crates/darius-cli/src/runtime.rs`, `crates/darius-cognitive/src/lib.rs`, `crates/darius-cognitive/src/subagent.rs`, `crates/darius-tools/src/mcp.rs`, `tests/harness_e2e/src/lib.rs`, and all tool tests in this task. Remove legacy `new` only at Task 3.3 after a repository search finds no caller. `PathPolicy` canonicalizes parents, rejects `..`, absolute/symlink escape, permits missing final component only for create. `SessionRuntime` retains shared task-board handle instead of discarding it.

RED: traversal, symlink escape, valid create, explicit cwd, retained board.

Run RED: `cargo test -p darius-tools path_policy_ -- --nocapture && cargo test -p darius-cli runtime_retains_task_board -- --nocapture`

Expected RED: old one-root registry and discarded board.

Implement; run GREEN with same commands, then `cargo check --workspace --all-targets`.

Commit: `git add crates/darius-tools crates/darius-cli crates/darius-cognitive tests/harness_e2e && git commit -m 'fix(tools): bind registry and tasks to workspace'`

## Task 2.2 — Replace toy file/search/write tools

**Files**
- Create: `crates/darius-tools/src/read_file.rs`
- Create: `crates/darius-tools/src/search_files.rs`
- Create: `crates/darius-tools/src/write_file.rs`
- Create: `crates/darius-tools/src/spec.rs`
- Modify: `crates/darius-tools/src/lib.rs`
- Modify: `crates/darius-tools/Cargo.toml`
- Modify: `Cargo.lock`

Add `tempfile = "3"`. Implement line-paged UTF-8 read, recursive filename/content search with caps, atomic same-directory write+rename, binary rejection, JSON tool schemas, and one output finalizer enforcing 32 KiB spill.

Run RED: `cargo test -p darius-tools coding_file_ -- --nocapture`

Expected RED: APIs/behavior absent.

Implement; run GREEN: same command; expected pagination, recursive search, caps, atomicity, containment, binary rejection, spill pass.

Commit: `git add crates/darius-tools Cargo.lock && git commit -m 'feat(tools): add bounded coding file operations'`

## Task 2.3 — Make shell cancellable and truthful

**Files**
- Create: `crates/darius-tools/src/execution.rs`
- Create: `crates/darius-tools/src/shell.rs`
- Modify: `crates/darius-tools/src/lib.rs`
- Modify: `crates/darius-tools/Cargo.toml`

Contract:

```rust
pub struct ExecutionContext {
    pub cancel: tokio_util::sync::CancellationToken,
    pub deadline: std::time::Instant,
}

pub trait ToolExecutor: Send + Sync {
    fn execute(&self, call: &ToolCall, ctx: &ExecutionContext) -> ToolOutcome;
}
```

Shell runs in workspace, captures exit code/stdout/stderr, polls every ≤25 ms, kills/reaps process group on token/deadline, and returns `Interrupted`, `TimedOut`, or nonzero error. Add `tokio-util = "0.7"` and `[target.'cfg(unix)'.dependencies] libc = "0.2"` directly to `crates/darius-tools/Cargo.toml`. Windows shell is unavailable in this recovery. No child/process-group may remain.

Run RED: `cargo test -p darius-tools shell_ -- --nocapture`

Expected RED: long command ignores cancellation/nonzero returns Ok.

Implement; run GREEN: tests cover cwd, success, nonzero, stderr, timeout, 2-second cancellation, child reaping, large output spill. Then run `cargo check --workspace --all-targets`.

Commit: `git add crates/darius-tools Cargo.lock && git commit -m 'fix(tools): cancel and reap shell execution'`

## Task 2.4 — Enforce closed-world model tool allowlist

**Files**
- Create: `crates/darius-tools/src/model_tools.rs`
- Modify: `crates/darius-tools/src/lib.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Test: `crates/darius-tools/src/model_tools.rs`

Only these model-visible tools may be registered, each with explicit risk (registration has no default risk): ReadOnly `read_file`, `search_files`, `memory_search`, `memory_pack`, `task_list`, contained `spill_read`; Mutating `write_file`, `memory_remember`, `task_add`, `task_complete`; Shell `shell`. Remove model-controlled `approved` fields. Reject unknown/hidden calls before permission and return one correlated error. Explicit negative cases: `peer_send`, `mcp_*`, `subagent_*`, `worktree_*`, `rollback`, `cron`, legacy `glob`, `read_spill`, browser/A2A.

AllowSession keys: write = tool+canonical path; shell = tool+canonical workspace+exact command; memory/task = tool+canonical complete argument JSON. Document shell is approval-gated but not host-filesystem sandboxed.

Run RED: `cargo test -p darius-tools model_tool_allowlist_ -- --nocapture`

Expected RED: unsupported handlers/default-read-only registration remain reachable.

Implement; run GREEN with same command, then `cargo check --workspace --all-targets`.

Commit: `git add crates/darius-tools crates/darius-cli/src/runtime.rs && git commit -m 'fix(tools): close model execution to verified allowlist'`

---

# Work Unit 3 — Define valid, cancellable model protocol

## Task 3.1 — Add correlated conversation messages

**Files**
- Create: `crates/darius-cognitive/src/conversation.rs`
- Create: `crates/darius-cognitive/src/model.rs`
- Modify: `crates/darius-cognitive/src/lib.rs`
- Modify: `crates/darius-cognitive/Cargo.toml`
- Modify: `Cargo.lock`

Add `async-trait = "0.1"`, `tokio-util = "0.7"`. Introduce parallel `AsyncModel`; leave existing synchronous `Model` intact until Task 3.3. `LegacyModelAdapter` implements `AsyncModel`. Protocol:

```rust
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    System { content: String },
    User { content: String },
    Assistant { content: Option<String>, tool_calls: Vec<ToolCall> },
    Tool { tool_call_id: String, name: String, content: String },
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ToolCall { pub id: String, pub name: String, pub arguments: serde_json::Value }

#[async_trait::async_trait]
pub trait AsyncModel: Send {
    async fn complete(&mut self, messages: &[Message], tools: &[ToolSpec], ctx: &TurnContext)
        -> Result<ModelOutput, CognitiveError>;
}
```

`TurnContext` contains fresh cancellation token + 60-second request deadline. Reject empty/orphan/duplicate tool IDs. Every assistant tool-call message is appended before execution; every success, denial, error, timeout, and interruption receives one matching Tool result.

Keep existing `Model` and a temporary `LegacyModelAdapter` so current external impls/callers compile; remove both only in Task 3.3 after caller audit. Run `cargo check --workspace --all-targets` before commit.

Run RED: `cargo test -p darius-cognitive conversation_protocol_ -- --nocapture`

Expected RED: structured protocol absent.

Implement contract/tests; run GREEN: same command.

Commit: `git add crates/darius-cognitive Cargo.lock && git commit -m 'feat(model): define correlated cancellable conversation protocol'`

## Task 3.2 — Implement exact configured OpenAI-compatible adapter

**Files**
- Create: `crates/darius-daemon/src/model_router/openai.rs`
- Create: `crates/darius-daemon/src/model_router/wire.rs`
- Modify: `crates/darius-daemon/src/model_router.rs`
- Modify: `crates/darius-daemon/Cargo.toml`

Use async `reqwest::Client` (`json`, `rustls-tls`; remove blocking feature from coding path). `LiveModel` implements `AsyncModel` and owns one validated provider ID/config; no hard-coded default/rater fallback. Domain `Message`, `ToolCall`, and `ToolSpec` are never serialized directly. Dedicated DTOs encode tool definitions as `type:function` + `function.{name,description,parameters}`, assistant calls as `{id,type:"function",function:{name,arguments:"<json string>"}}`, and tool results as `{role:"tool",tool_call_id,content}`; decoder parses arguments string to JSON. Content+tool calls may coexist; execute calls before terminal text. URL is `<trimmed-base-url>/chat/completions`; headers include Bearer key and JSON content type. Use `tokio::select!` across `ctx.cancel.cancelled()`, request future, and deadline; connect timeout 10 s, total request deadline 60 s. Parse content or all tool calls with IDs/JSON arguments; reject mixed malformed/orphan/duplicate IDs. Sanitize errors.

RED wiremock tests:
- non-default provider/base URL/model selected;
- first request rejects missing/wrong Authorization and returns two exact nested tool calls;
- second request contains original assistant tool calls followed by exact correlated tool results;
- text response;
- invalid JSON/ID, 401, 429, 5xx;
- delayed response cancels and drops within 2 seconds;
- no secret in body/events/snapshots/logs/errors. Run `cargo check --workspace --all-targets` before commit.

Run RED: `cargo test -p darius-daemon openai_ -- --nocapture`

Expected RED: hard-coded provider and invalid protocol.

Implement; run GREEN: same command.

Commit: `git add crates/darius-daemon Cargo.lock && git commit -m 'fix(model): execute exact cancellable provider protocol'`

## Task 3.3 — Replace plan/react loop with one agent loop

**Files**
- Create: `crates/darius-cognitive/src/agent_loop.rs`
- Create: `crates/darius-cognitive/src/system_prompt.rs`
- Modify: `crates/darius-cognitive/src/lib.rs`
- Modify: `crates/darius-cli/src/lib.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cli/src/tui_runtime.rs`
- Modify: `crates/darius-daemon/src/model_router.rs`
- Modify: `tests/harness_e2e/src/lib.rs`

SessionRuntime now owns `Conversation`, shared `TaskBoard`, and model. Agent loop:
1. append/emit user message;
2. call model with stable coding prompt + bounded memory + real tool specs;
3. append/emit assistant text, or append assistant tool calls;
4. enforce execution policy and permission for each call;
5. execute with context, append one correlated Tool result, emit ToolStart/End/Diff;
6. continue until text terminal response, cancellation/error, or 12 rounds;
7. compact oldest tool results first at bounded character budget;
8. always emit exactly one terminal Error/Interrupted and one Done.

Migrate `crates/darius-cli/src/lib.rs`, `crates/darius-cli/src/runtime.rs`, `crates/darius-cli/src/tui_runtime.rs`, `tests/harness_e2e/src/lib.rs`, and every remaining `CognitiveLoop`, `run_loop`, `impl Model`, or `dyn Model` caller. Remove JSON plan wrapping, forced DONE, old synchronous trait/loop, legacy constructor, and `LegacyModelAdapter` only after `rg -n 'CognitiveLoop|run_loop\(|impl .*Model|dyn .*Model|ToolRegistry::new\(' crates tests` shows no unmigrated production caller. System prompt includes workspace, inspect-before-edit, test-before-complete, secret policy, and Plan no-mutation rule.

Run RED: `cargo test -p darius-cognitive agent_loop_ -- --nocapture`

Expected RED: no valid feedback/multi-turn loop.

Implement; run GREEN: same command. Then `cargo check --workspace --all-targets && cargo test -p darius-cognitive && cargo test -p darius-daemon && cargo test -p darius-cli --lib`.

Commit: `git add crates/darius-cognitive crates/darius-daemon crates/darius-cli && git commit -m 'feat(cognitive): run multi-turn coding agent loop'`

---

# Work Unit 4 — Responsive runtime ownership, permissions, policies

## Task 4.1 — Reproduce blocked controls

**Files**
- Add tests: `crates/darius-cli/src/tui_runtime.rs`

RED tests independently prove:
- PermissionRequired → ResolvePermission → Done under 2 s;
- delayed provider → Interrupt → Interrupted+Done under 2 s;
- long shell → Interrupt → killed/reaped+Done under 2 s;
- cancelled turn returns runtime; next turn succeeds;
- shutdown leaves no turn task.

Run RED: `cargo test -p darius-cli blocked_control_ -- --nocapture`

Expected RED: current worker cannot receive while turn runs.

Commit tests only: `git add crates/darius-cli/src/tui_runtime.rs && git commit -m 'test(runtime): reproduce blocked controls and poisoned turns'`

## Task 4.2 — Build async session actor

**Files**
- Create: `crates/darius-core/src/runtime_protocol.rs`
- Create: `crates/darius-cli/src/tui_runtime/actor.rs`
- Create: `crates/darius-cli/src/tui_runtime/turn.rs`
- Replace touched behavior in: `crates/darius-cli/src/tui_runtime.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-tui/src/controller.rs`
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-tui/src/commands.rs`
- Modify: `crates/darius-core/src/lib.rs`
- Modify: `crates/darius-tui/Cargo.toml`
- Modify: `Cargo.lock`

Move dependency-neutral `Mode::{Auto,Plan}`, `PermissionChoice`, `TurnId`, and neutral slash payload to `darius-core`; export from `crates/darius-core/src/lib.rs`. Define generic `RuntimeEvent<E> { turn_id, event }` so core never references cognitive/TUI. Add `darius-core` to `crates/darius-tui/Cargo.toml`, update `Cargo.lock`, and migrate TUI-owned command/permission types. Actor owns:

```rust
pub enum ActorState {
    Idle(SessionRuntime),
    Running { turn_id: TurnId, view: SessionView, permissions: Arc<SessionPermissions>, cancel: CancellationToken, control: Arc<TurnControl>, join: tokio::task::JoinHandle<TurnResult> },
    Stopped,
}

pub struct TurnResult { pub runtime: SessionRuntime, pub outcome: Result<(), String> }
```

Actor uses `tokio::select!` over command receive and running JoinHandle. Immutable `SessionView` supplies profile/model/mode/workspace/task counts while runtime is moved; shared permissions supplies `/permissions`. Turn token is new per submit. Completion joins and restores Idle. During Running: ResolvePermission, Interrupt, `/stop`, `/quit`, and read-only status accepted; new goal/mutating commands return typed Busy. Shutdown cancels; waits ≤2 s; aborts task only on invariant failure and emits fatal error. Tests assert no active task after normal cancellation/shutdown.

Run RED: `cargo test -p darius-cli blocked_control_ -- --nocapture`

Expected RED from Task 4.1.

Implement; run GREEN: same command, then `cargo check --workspace --all-targets`.

Commit: `git add crates/darius-core crates/darius-cli crates/darius-tui Cargo.lock && git commit -m 'fix(runtime): keep session controls responsive'`

## Task 4.3 — Persist and enforce session permissions

**Files**
- Create: `crates/darius-cli/src/permissions.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cli/src/tui_runtime/turn.rs`
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-tui/src/input.rs`

`SessionPermissions` lives in SessionRuntime and is reused by every turn control. Cache key includes tool + normalized target; resets on process restart. Semantics:
- AllowOnce executes once;
- AllowSession persists across turns only for exact key;
- Deny appends correlated tool result and loop continues;
- Escape resolves Deny—never drops chooser silently;
- Ctrl+C during chooser cancels turn and clears pending sender;
- EOF/shutdown resolve/cancel all pending requests;
- non-TTY `run` denies Mutating/Shell and exits with guidance, never blocks;
- setup/offline never requests mutation.

Run RED: `cargo test -p darius-cli permission_lifecycle_ -- --nocapture && cargo test -p darius-tui permission_escape_ -- --nocapture`

Expected RED: approval cache/control lifecycle incomplete.

Implement; run GREEN with same commands.

Commit: `git add crates/darius-cli crates/darius-tui && git commit -m 'fix(permissions): persist approvals and resolve every prompt'`

## Task 4.4 — Enforce only Auto and Plan modes

**Files**
- Create: `crates/darius-cognitive/src/execution_policy.rs`
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-tui/src/input.rs`
- Modify: `crates/darius-tui/src/render.rs`
- Modify: `README.md`

Hide Manual, AcceptEdits, and Effort. `ExecutionPolicy` is checked before permission:
- Plan: hard-deny Mutating and Shell; disk unchanged even if model requests them; read-only allowed;
- Auto: ReadOnly auto-runs; Mutating/Shell require permission.

Shift+Tab toggles Auto/Plan only. `/mode` accepts only auto/plan. Plan denial becomes correlated tool result so model can respond without mutation.

Run RED: `cargo test -p darius-cognitive execution_policy_ -- --nocapture && cargo test -p darius-tui mode_ -- --nocapture`

Expected RED: four cosmetic modes and no hard guard.

Implement; run GREEN with same commands.

Commit: `git add crates/darius-cognitive crates/darius-tui README.md && git commit -m 'fix(mode): enforce auto and mutation-free plan policies'`

---

# Work Unit 5 — Make visible slash commands real

## Task 5.1 — Create one availability/command registry

**Files**
- Create: `crates/darius-core/src/commands.rs`
- Modify: `crates/darius-core/src/lib.rs`
- Replace touched registry in: `crates/darius-tui/src/commands.rs`
- Create: `crates/darius-cli/src/commands.rs`

Visible commands only:

| Command | Real outcome |
|---|---|
| `/help` | generated visible registry |
| `/clear` | typed ClearTranscript event |
| `/compact` | real conversation before/after counts |
| `/model` | current provider/model; no unsupported mutation |
| `/mode [auto|plan]` | enforced policy change |
| `/permissions` | session approval keys, no secrets |
| `/memory [query]` | real search |
| `/pack` | real bounded pack |
| `/tasks` | retained shared task board |
| `/status` | profile/model/mode/workspace/running |
| `/config` | effective non-secret config |
| `/stop` | active turn cancellation |
| `/quit` | shutdown |

No `/effort`, `/serve`, `/a2a`, or `/skills`. One registry owns help, aliases, args, availability, and palette.

Run RED: `cargo test -p darius-core command_registry_ -- --nocapture`

Expected RED: old 18-command registry.

Implement; run GREEN: same command.

Commit: `git add crates/darius-core crates/darius-tui crates/darius-cli Cargo.lock && git commit -m 'refactor(commands): define one truthful visible registry'`

## Task 5.2 — Execute every registered command

**Files**
- Create: `crates/darius-cli/src/command_handler.rs`
- Modify: `crates/darius-cli/src/runtime.rs`
- Modify: `crates/darius-cli/src/tui_runtime/actor.rs`
- Modify: `crates/darius-cognitive/src/ui_events.rs`
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-cli/Cargo.toml`
- Modify: `Cargo.lock`

Add typed events for clear/mode/error/status; expose conversation compaction, task board, approval store, memory, and config through SessionRuntime. Handler returns events/control action. Running-state matrix: `/stop`, `/quit`, `/status`, `/permissions` accepted; other commands typed Busy. Remove generic `Command:` echo.

RED table test iterates every visible registry item and asserts semantic state/output, not merely an event. Include `/clear` TUI consumption and filtered palette behavior.

Run RED: `cargo test -p darius-cli slash_command_ -- --nocapture`

Expected RED: generic echo.

Implement; run GREEN: same command. Then `cargo test -p darius-tui command_palette_ -- --nocapture`.

Commit: `git add crates/darius-cli crates/darius-cognitive crates/darius-tui Cargo.lock && git commit -m 'feat(commands): execute every visible slash command'`

---

# Work Unit 6 — Finish TUI behavior and terminal safety

## Task 6.1 — Fix event ordering, lag, resize, and paste

**Files**
- Modify: `crates/darius-tui/src/terminal.rs`
- Modify: `crates/darius-tui/src/controller.rs`
- Modify: `crates/darius-tui/src/app.rs`

Drain runtime events before draw. Handle correlated turn events, broadcast Lagged with visible warning, Closed after worker terminal state, Resize via `terminal.autoresize()`, Paste as one normalized insertion, and key release/repeat correctly.

Run RED: `cargo test -p darius-tui terminal_event_ -- --nocapture`

Expected RED: paste/resize/lag/correlation behaviors absent.

Implement; run GREEN: same command.

Commit: `git add crates/darius-tui/src && git commit -m 'fix(tui): process runtime and terminal events reliably'`

## Task 6.2 — Complete composer, palette, scroll, and disclosures

**Files**
- Modify: `crates/darius-tui/src/app.rs`
- Modify: `crates/darius-tui/src/input.rs`
- Modify: `crates/darius-tui/src/render.rs`
- Modify: `crates/darius-tui/src/snapshots/`

Add left/right/home/end/delete, Unicode cursor boundaries, multiline paste normalization, query-filtered palette selection, Enter execute vs Tab complete, auto-tail unless user scrolled, End resumes tail, and actual tool expansion. Clamp narrow layout/cursor. Snapshots: 40×12, 80×24, 140×40 for setup, live text, tool, permission, denial, error, plan mode.

Run RED: `cargo test -p darius-tui composer_ -- --nocapture`

Expected RED: missing editing/scroll/disclosure behavior.

Implement; update snapshots intentionally; run GREEN: `cargo test -p darius-tui` and verify no `.snap.new` via `test -z "$(find crates/darius-tui -name '*.snap.new' -print -quit)"`.

Commit: `git add crates/darius-tui && git commit -m 'fix(tui): complete editing palette and transcript behavior'`

## Task 6.3 — Restore terminal on all exits

**Files**
- Modify: `crates/darius-tui/src/terminal.rs`
- Modify: `crates/darius-cli/tests/tui_pty.rs`

Follow Ratatui/Crossterm lifecycle: disable raw mode, leave alternate screen, show cursor, flush. Guard every partial initialization step. PTY cases: `/quit`, idle Ctrl+C, provider error, worker disconnect, EOF, panic fixture; each exits bounded and external shell can print afterward.

Run RED: `cargo test -p darius-cli --test tui_pty cleanup_path -- --nocapture --test-threads=1`

Expected RED: incomplete PTY proof.

Implement; run GREEN: same command, then `cargo test -p darius-tui terminal_guard_ -- --nocapture`.

Commit: `git add crates/darius-tui crates/darius-cli/tests/tui_pty.rs && git commit -m 'fix(tui): restore terminal on every exit path'`

---

# Work Unit 7 — Full startup and coding-agent proof

## Task 7.1 — Add deterministic fake provider

**Files**
- Create: `crates/darius-cli/tests/support/mod.rs`
- Create: `crates/darius-cli/tests/support/fake_provider.rs`
- Modify: `crates/darius-cli/tests/tui_pty.rs`

Scripted protocol:
1. text response;
2. two correlated read tool calls;
3. verify tool result IDs, then text;
4. write call, denial result, then recovery text;
5. write call, approval, then completion;
6. delayed HTTP request for cancellation;
7. shell call for cancellation;
8. next successful turn.

Fake endpoint rejects missing/wrong `Authorization: Bearer ...`. Record exact requests; assert URL `/chat/completions`, JSON content type, system/user/assistant-tool-calls/tool-results order, and no secret values outside Authorization.

Run RED: `cargo test -p darius-cli --test tui_pty fake_provider_protocol -- --nocapture --test-threads=1`

Expected RED: current provider protocol invalid.

Implement fixture; test remains RED until journey wiring. Commit fixture/tests: `git add crates/darius-cli/tests && git commit -m 'test(e2e): script real provider tool protocol'`.

## Task 7.2 — Prove clean first-run journey

**Files**
- Modify: `crates/darius-cli/tests/tui_pty.rs`

With empty temp DARIUS_HOME, no key, bare `darius`:
- setup screen appears, no fake completion/hang;
- goal submission gives actionable config guidance;
- `/config`, `/status`, `/help` work;
- `/quit` exits 0 and terminal restores;
- no files under real `~/.darius`.

Then binary `config init` in same temp home; restart without key; verify exact missing env guidance and secret not stored.

Run RED: `cargo test -p darius-cli --test tui_pty first_run_setup_journey -- --nocapture --test-threads=1`

Expected RED until startup/config/TUI complete.

Run GREEN after fixes: same command.

Commit: `git add crates/darius-cli/tests/tui_pty.rs && git commit -m 'test(e2e): prove honest clean first launch'`

## Task 7.3 — Prove complete live TTY journey

**Files**
- Modify: `crates/darius-cli/tests/tui_pty.rs`

From external temp workspace, bare `darius` with temp config/key env and fake provider:
- see exact profile/model/workspace;
- assistant text renders;
- two read calls return correlated results;
- write denial via Escape does not deadlock; disk unchanged; next turn succeeds;
- AllowSession persists across exact target on later turn but not a different target;
- Plan mode denies write/shell with unchanged disk;
- Auto write approval changes file and shows diff;
- delayed provider Ctrl+C returns Interrupted+Done under 2 s;
- long shell Ctrl+C kills/reaps under 2 s;
- next turn succeeds after each cancellation;
- all visible slash commands have semantic assertions;
- `/quit` exit 0 and terminal restored.

Run RED: `cargo test -p darius-cli --test tui_pty full_agent_journey -- --nocapture --test-threads=1`

Expected RED until all recovery work complete.

Run GREEN: same command; <45 s; no skip paths.

Commit: `git add crates/darius-cli/tests/tui_pty.rs && git commit -m 'test(e2e): prove complete interactive coding agent'`

## Task 7.4 — Prove retained noninteractive commands

**Files**
- Create: `crates/darius-cli/tests/run_e2e.rs`
- Modify: `crates/darius-cli/tests/cli_contract.rs`

Table-drive every retained CLI/nested operation and global flag. Fake-provider `run` proves assistant text/read tool. Mutating/shell call is denied without blocking. 401/429/5xx/timeout/invalid response exit 1 with sanitized actionable errors. Memory commands use temp home and verify profile isolation/import/export round-trip.

Run RED: `cargo test -p darius-cli --test run_e2e -- --nocapture && cargo test -p darius-cli --test cli_contract -- --nocapture`

Expected RED: incomplete dispatch/error behavior.

Implement fixes; run GREEN with same commands.

Commit: `git add crates/darius-cli/tests && git commit -m 'test(cli): prove retained command surface'`

---

# Work Unit 8 — Installer, release, truth, and closure

## Task 8.1 — Align installer and assets

**Files**
- Create: `scripts/release-target.sh`
- Modify: `install.sh`
- Modify: `.github/workflows/release.yml`
- Create: `tests/install_test.sh`

Canonical names:
- `darius-linux-x86_64.tar.gz`;
- `darius-macos-aarch64.tar.gz`;
- `darius-macos-x86_64.tar.gz`.

Installer maps uname exactly, fetches checksum, verifies before extract, validates binary version, installs atomically, and leaves old binary on failure. It also supports `bash install.sh --artifact-dir ./dist --version 1.2.0 --install-dir "$TMPDIR/bin"`, using identical archive/checksum validation for staged local artifacts. Workflow uploads `install.sh`. Test mocks release HTTP and proves success/checksum mismatch/missing asset/atomic replacement/PATH guidance.

Run RED: `bash tests/install_test.sh`

Expected RED: Darwin mismatch and missing release installer.

Implement; run GREEN: same command.

Commit: `git add scripts install.sh tests .github/workflows/release.yml && git commit -m 'fix(release): align installer and release assets'`

## Task 8.2 — Gate tag release on exact SHA evidence

**Files**
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/ci.yml`

Set workflow permissions `contents: read`; grant `contents: write` only to publish job. Pin/install `actionlint` and make it mandatory locally, in CI, and at tagged SHA. Release prerequisite job at tagged SHA:
- assert tag `v${workspace_version}`;
- fmt, clippy, workspace tests;
- PTY suite serially;
- installer tests;
- build matrix;
- verify all archives/checksums/install.sh and write `release-evidence.json` with baseline/WU commit SHAs, artifact SHA-256 values, version, and tag;
- host-compatible installed binary `--version` + no-arg non-TTY help.

Release job consumes only prerequisite artifacts and cannot publish partial set. No tag/publish during implementation.

Run RED syntax/test fixture: `actionlint .github/workflows/*.yml && bash tests/install_test.sh release_matrix`

Expected RED: missing mandatory actionlint/prerequisite/exact-version/evidence checks.

Implement; run GREEN with same command. If installed, also `actionlint .github/workflows/*.yml`.

Commit: `git add .github/workflows tests/install_test.sh && git commit -m 'ci(release): require exact tagged functional evidence'`

## Task 8.3 — Audit every machine-visible claim

**Files**
- Modify: `Cargo.toml`
- Modify: all touched `crates/*/Cargo.toml` descriptions
- Modify: `README.md`
- Modify: `CHANGELOG.md`
- Modify: `docs/CAPABILITIES.md`
- Create: `docs/TROUBLESHOOTING.md`
- Modify: `install.sh`
- Modify: `crates/darius-web/src/lib.rs`
- Create: `scripts/audit-public-claims.sh`

Audit manifests, generated help, README/changelog/docs, installer output, dashboard HTML, agent card, health/status output, and release workflow notes. Remove unsupported superlatives and web/A2A/live claims. Capability registry/matrix is source for visible support. Explain setup/live/offline-demo, Auto/Plan, permissions, Doctor, and retained commands. Record prior `v1.1.2` overclaims as corrected—not silently erased.

Create `scripts/audit-public-claims.sh` to audit only machine-visible outputs and authoritative content: generated top-level/nested CLI help; generated slash help/palette; every manifest description; README/CHANGELOG/capabilities/troubleshooting; installer output; dashboard HTML, agent card, health/status; release-note source. It must check v1.1.2 and unreleased v1.2.0 corrections plus hidden names (`cron`, `approval-check`, `peer_send`, MCP, subagent, worktree/rollback, A2A). Hidden Rust symbols/tests/comments do not fail merely for containing those words.

Run RED: `bash scripts/audit-public-claims.sh`

Expected RED: unsupported machine-visible claims found.

Implement; rerun. Expected GREEN: exit 0. Add CI/release execution plus a verified-capability-to-existing-test check.

Commit: `git add Cargo.toml crates README.md CHANGELOG.md docs install.sh && git commit -m 'docs: align every capability claim with executable proof'`

## Task 8.4 — Run full gates and adversarial review

Run exactly:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p darius-cli --test tui_pty -- --nocapture --test-threads=1
cargo test -p darius-cli --test cli_contract -- --nocapture
cargo test -p darius-cli --test run_e2e -- --nocapture
cargo build --release -p darius-cli
./target/release/darius --version
actionlint .github/workflows/*.yml
bash scripts/audit-public-claims.sh
bash tests/install_test.sh
shasum -a 256 ./target/release/darius
```

Expected: all exit 0; no SKIP; record actual test totals and release-binary SHA only now. Before any install decision, run both clean-home and live bare-launch PTY journeys against exact `./target/release/darius` from an external temp directory (PTY helper accepts `DARIUS_BIN_UNDER_TEST`). Verify `release-evidence.json`. Rehearse ordered rollback on a temporary branch/worktree and rerun full gates. Run three fresh implementation reviewers: concurrency/cancellation, security/tool boundaries, product truth/E2E. Every FAIL gets focused RED test, minimal fix, full rerun, fresh review.

## Task 8.5 — User-visible binary checkpoint

Default deliverable before approval is the SHA-recorded, PTY-verified `./target/release/darius`; do not imply user PATH changed. Present two explicit choices:

- approve local install into resolved PATH; first atomically preserve existing binary path/version/SHA, then run `bash install.sh --artifact-dir ./dist --version 1.2.0 --install-dir "<resolved-dir>"`, require installed SHA equals staged release SHA, and rerun bare PTY launch from an external temp directory. Installer test must restore preserved binary atomically;
- keep repository artifact only.

PATH install is state-changing and requires explicit approval. Git push/tag/release remains separately approval-gated.

---

## Tests / validation matrix

| Layer | Exact command | Blocking proof |
|---|---|---|
| CLI | `cargo test -p darius-cli --test cli_contract -- --nocapture` | all 17 baseline tokens disposed + exits |
| Paths/config | `cargo test -p darius-cli paths_ -- --nocapture && cargo test -p darius-cli config_ -- --nocapture` | isolated home/workspace; no silent fallback |
| Tools | `cargo test -p darius-tools` | containment, atomicity, exit, spill, cancellation |
| Provider | `cargo test -p darius-daemon openai_ -- --nocapture` | exact provider, correlated IDs, cancellation |
| Agent | `cargo test -p darius-cognitive agent_loop_ -- --nocapture` | feedback, multi-turn, policies, terminal events |
| Runtime | `cargo test -p darius-cli blocked_control_ -- --nocapture` | approval/HTTP/shell cancel + next turn |
| Commands | `cargo test -p darius-cli slash_command_ -- --nocapture` | every visible command semantic behavior |
| TUI | `cargo test -p darius-tui` | reducer/render/input/cleanup |
| First run | `cargo test -p darius-cli --test tui_pty first_run_setup_journey -- --nocapture --test-threads=1` | clean bare launch, setup, quit |
| Full TTY | `cargo test -p darius-cli --test tui_pty full_agent_journey -- --nocapture --test-threads=1` | real text/tools/deny/approve/plan/cancel/recovery |
| Installer | `bash tests/install_test.sh` | exact assets/checksum/atomic install |
| Workspace | full commands in Task 8.4 | all green, no skip |

## Risks and tradeoffs

- **Async migration:** model trait/provider/runtime change together. Compatibility adapter exists only through Task 3.2 and is removed in Task 3.3; each commit remains compile-green.
- **Cancellation:** cooperative cancellation is mandatory in HTTP and shell. Actor abort is fatal shutdown fallback only, not normal recovery.
- **Provider breadth:** only OpenAI-compatible verified. Configured provider ID is exact; no silent fallback.
- **Mode breadth:** Auto + Plan only. Hiding cosmetic modes is better than false behavior.
- **Server/A2A breadth:** removed from critical path and public registry. Separate future plan needed.
- **Non-TTY mutation:** deny by default; users needing approvals use TUI. No unattended unsafe flag in this recovery.
- **Existing oversized modules:** extract only cohesive touched responsibilities when needed; pre-existing untouched size debt is not this plan’s scope.
- **Publication:** workflow becomes ready, but no push/tag/release occurs without approval.

## Rollback manifest

Baseline: `de2e37f55038a8c01a47f552c217b70377035f89`. Record every completed WU/task commit SHA in `release-evidence.json`.

| Cluster | Units | Dependents | Reverse-revert order | Post-revert proof |
|---|---|---|---|---|
| release/docs | 8 | none | 8.3, 8.2, 8.1 | `cargo test --workspace && bash tests/install_test.sh` if script remains |
| E2E | 7 | 8 | 7.4, 7.3, 7.2, 7.1 | `cargo test --workspace` |
| TUI/commands | 5–6 | 7–8 | 6.3→5.1 | `cargo test -p darius-tui -p darius-cli` |
| actor/policy | 4 | 5–8 | 4.4→4.1 | `cargo test -p darius-cli -p darius-cognitive` |
| model protocol | 3 | 4–8 | 3.3→3.1 | `cargo test -p darius-daemon -p darius-cognitive` |
| tools | 2 | 3–8 | 2.3→2.1 | `cargo test -p darius-tools` |
| startup/config | 1 | 2–8 | 1.4→1.1 | `cargo test -p darius-cli` |
| RED evidence | 0 | all | revert last | baseline tests only |

Create a temporary rollback branch from final HEAD and run ordered `git revert --no-edit <recorded-sha>` from downstream to upstream; never revert an upstream cluster while dependent commits remain. Preserve user profile config and memory DB; schema is unchanged, so no data rollback. Test any changed config against both baseline binary and new parser before install. Existing v1.1.2 is not a valid asset rollback target because its matrix is incomplete. For a published v1.2.0 failure, hide/yank failed release and publish gated v1.2.1 forward fix; never overwrite immutable tag/assets. Before approved PATH install, preserve old binary path/version/SHA and test atomic restore. After local install rollback, reinstall previously verified asset and rerun `command -v darius`, `darius --version`, and non-TTY help.

## Completion conditions

- bare `darius` opens setup/live TUI in a TTY;
- clean first launch never hangs or fakes completion;
- first live prompt returns assistant text;
- valid OpenAI tool-call/result round trips preserve IDs;
- permissions, Escape denial, provider cancellation, shell cancellation respond under 2 seconds;
- cancelled/denied turns do not poison later turns;
- Auto/Plan behavior is runtime-enforced;
- every visible CLI/slash command has semantic binary tests;
- workspace writes are contained/atomic and shell failures truthful;
- terminal always restores;
- installer/workflow assets agree and release is exact-version gated;
- docs/manifests/help/cards contain no unsupported claims;
- exact SHA of repository release binary passes clean-home + live bare-launch PTY from external cwd; PATH install/publication await explicit approval.

## Next action

After all three fresh plan reviewers PASS and user approves: execute Work Units 0–8 task-by-task with two-stage review per commit.
