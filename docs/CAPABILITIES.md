# Darius capability inventory

**Truth date:** 2026-09-07

**Scope:** repository recovery surface only. This document is deliberately stricter than source presence, unit tests, README copy, or a generated agent card.

## Status policy

| Status | Meaning |
| --- | --- |
| **Verified** | The public recovery behavior is covered by the linked executable test. A verified listing is not a claim that its underlying feature completes real work. |
| **Experimental** | End-to-end reachable in the recovery surface and has scoped test evidence, but does not yet meet the verified bar. |
| **Unavailable** | Not publicly proven by a recovery-surface test, not wired to a public entry point, explicitly hidden by recovery, or only represented by source/README/agent-card text. Do not advertise it as working. |

There are **no Experimental capabilities** at this revision.

## Verified public CLI contract

| Surface | Status | Exact proof |
| --- | --- | --- |
| `darius --help` / `darius -h` exposes only `tui`, `run`, `config`, and `memory` | **Verified** | [`public_help_matches_recovery_surface`](../crates/darius-cli/tests/cli_contract.rs#L31) |
| `darius --version` | **Verified** | [`version_flag_works`](../crates/darius-cli/tests/cli_contract.rs#L70) |
| `darius -V` | **Verified** | [`version_flag_short_works`](../crates/darius-cli/tests/cli_contract.rs#L78) |
| Global `--profile <name>` before or after a subcommand, when used with help | **Verified** | [`global_flags_before_subcommand`](../crates/darius-cli/tests/cli_contract.rs#L86); [`global_flags_after_subcommand`](../crates/darius-cli/tests/cli_contract.rs#L101) |
| Global `--session <id>` before or after a subcommand, when used with help | **Verified** | [`global_flags_before_subcommand`](../crates/darius-cli/tests/cli_contract.rs#L86); [`global_flags_after_subcommand`](../crates/darius-cli/tests/cli_contract.rs#L101) |
| Missing `--profile`/`--session` value and unknown global flag exit `2` | **Verified** | [`malformed_nested_args`](../crates/darius-cli/tests/cli_contract.rs#L116) |
| Unknown command exits `2` | **Verified** | [`unknown_command_exits_two`](../crates/darius-cli/tests/cli_contract.rs#L47) |
| Bare non-TTY invocation prints a setup hint and exits `0`, without full usage | **Verified** | [`no_arg_non_tty_help`](../crates/darius-cli/tests/cli_contract.rs#L131) |
| Bare clean-home PTY invocation prints a setup hint, restores the terminal, exits `0`, and does not modify `~/.darius` | **Verified** | [`clean_home_bare_launch`](../crates/darius-cli/tests/tui_pty.rs#L212) |

## CLI command and nested-command inventory

`darius` dispatches only the four recovery commands in [`crates/darius-cli/src/lib.rs`](../crates/darius-cli/src/lib.rs#L128-L191).

| Command / nested form | Source state | Status |
| --- | --- | --- |
| `tui` | Dispatched; accepts source-scanned `--cwd <path>` | **Verified** — PTY first-run and multi-turn live journey ([`first_run_setup_journey`](../crates/darius-cli/tests/tui_pty.rs#L212), [`full_agent_journey`](../crates/darius-cli/tests/tui_pty.rs#L340)). |
| `run <goal...>` | Dispatched; source selects mock or configured model | **Verified** — noninteractive execution and mutation denial ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `config` | Dispatched; no nested argument prints usage | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `config show` | Source implementation | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `config init` | Source implementation | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `config set` | Source prints a sample configuration; it does not write one | **Unavailable** — do not advertise as mutator. |
| `memory` | Dispatched; no nested argument prints usage | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `memory search <query>` | Source implementation | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `memory pack` | Source implementation | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `memory import <file>` | Source implementation | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `memory export <file>` | Source implementation | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `memory stats` | Source implementation | **Verified** ([`run_e2e.rs`](../crates/darius-cli/tests/run_e2e.rs)). |
| `--cwd <path>` | `tui`-only source scan, not a declared global flag | **Unavailable**. |
| Usage aliases `t`, `r`, `c`, `m` | Printed in help but not matched by dispatch | **Unavailable** — do not use as aliases. |

### Explicitly removed/hidden CLI tokens

The following tokens must exit `2`; that behavior is the only verified fact about them. Source functions, README rows, and historical tests do not make them available.

| Token / nested surface | Status | Exact proof / source note |
| --- | --- | --- |
| `daemon`, `status`, `start`, `stop`, `attach`, `eval`, `learn` | **Unavailable** | [`removed_tokens_all_exit_two`](../crates/darius-cli/tests/cli_contract.rs#L53) |
| `session-smoke`, `serve`, `a2a`, `cron`, `approval-check`, `help` | **Unavailable** | [`removed_tokens_all_exit_two`](../crates/darius-cli/tests/cli_contract.rs#L53) |
| `a2a card` | **Unavailable** | Parent `a2a` is explicitly removed. |
| `cron list`, `cron add`, `cron run`, `cron notepad` | **Unavailable** | Parent `cron` is explicitly removed. |
| `doctor` | **Unavailable** | Diagnostic workflow covered in [TROUBLESHOOTING.md](TROUBLESHOOTING.md). |

## TUI command palette and modes

The canonical command registry is defined in [`crates/darius-core/src/commands.rs`](../crates/darius-core/src/commands.rs). It provides strictly 13 canonical slash commands. All legacy commands have been retired.

| Slash command (and `-` alias) | Source-stated purpose | Status |
| --- | --- | --- |
| `/help`, `/clear`, `/compact` | help, clear transcript, compact context | **Verified** |
| `/model`, `/mode`, `/permissions` | model/mode/policy UI | **Verified** |
| `/effort` | effort selector | **Unavailable** — explicitly hidden in recovery. |
| `/memory`, `/pack`, `/tasks`, `/plan` | memory/task UI (`/plan` is legacy command; mode is toggled via `/mode`) | `/memory`, `/pack`, `/tasks` **Verified**; `/plan` **Unavailable**. |
| `/status`, `/config` | status/config UI | **Verified** |
| `/skills` | skill list/search UI | **Unavailable** — explicitly hidden in recovery. |
| `/a2a`, `/serve` | A2A/server UI | **Unavailable** — explicitly hidden in recovery. |
| `/stop`, `/quit` | interrupt / exit UI | **Verified** |

| Mode or selector value | Source state | Status |
| --- | --- | --- |
| `Auto` | Default TUI enum value | **Verified** — read-only auto-executes, mutating/shell gated by permission. |
| `Manual` | TUI enum value | **Unavailable** — explicitly hidden in recovery. |
| `AcceptEdits` | TUI enum value | **Unavailable** — explicitly hidden in recovery. |
| `Plan` | TUI enum value | **Verified** — strictly denies mutating tools and shell execution. |
| Effort `low`, `medium`, `high`, `xhigh`, `max`, `ultracode` | TUI enum values | **Unavailable** — `/effort` is hidden. |

## Providers and configuration

| Provider / configuration claim | Source state | Status |
| --- | --- | --- |
| Offline `MockModel` when no model config is present | Source fallback in [`runtime.rs`](../crates/darius-cli/src/runtime.rs#L82-L96) | **Unavailable** — no public `run` proof. |
| Configured `openai_compatible` endpoint | Config accepts arbitrary `provider`, `base_url`, `model`, and API-key environment variable; default example names `openai_compatible` | **Unavailable** — no live-provider recovery test. |
| OpenAI API | Only an OpenAI-compatible wire format is source-visible | **Unavailable** — do not claim a deployed/live integration. |
| Anthropic / Claude native provider | A router default contains an Anthropic URL, but the client is OpenAI-compatible | **Unavailable** — native Anthropic support is explicitly not available. |
| Any other named provider, model override, or automatic failover | Source-only configuration/router concepts | **Unavailable**. |

## Built-in tools and integrations

The session runtime registers memory, task, and coding tools in source ([`runtime.rs`](../crates/darius-cli/src/runtime.rs#L76-L80)). No recovery test proves that a user can invoke any tool through a successful public session, so every item below is **Unavailable**.

| Tool or integration | Source state | Status |
| --- | --- | --- |
| `memory_search`, `memory_pack`, `memory_remember` | Memory-tool registrations | **Unavailable** |
| `task_add`, `task_list`, `task_complete` | Task-tool registrations | **Unavailable** |
| `shell`, `read_file`, `search_files`, `write_file`, `spill_read` | Coding-tool registrations (`glob`, legacy `read_spill`, and `peer_send` removed; see Task 2.4) | **Unavailable** |
| `peer_send` | Registration removed (Task 2.4) | **Unavailable** — peer A2A is explicitly unavailable. |
| `subagent_spawn`, `subagent_list`, `subagent_steer`, `subagent_stop` | Source-only registration helper; not registered by the public runtime | **Unavailable** — subagents are explicitly unavailable. |
| MCP stdio/SSE server registry and dynamically discovered MCP tools | Source module only ([`mcp.rs`](../crates/darius-tools/src/mcp.rs#L25-L163)); no public runtime registration | **Unavailable** — MCP is explicitly unavailable. |
| Browser tool/integration | No public tool registration | **Unavailable**. |
| Skill discovery or skill mutation | Source registry concepts only; no public mutation interface | **Unavailable** — skill mutation is explicitly unavailable. |
| Worktree management, rollback, remote sandboxes | Source modules/README claims, not recovery entry points | **Unavailable** — all are explicitly unavailable. |
| Messaging beyond the in-process source peer stub | No recovery entry point | **Unavailable**. |

## Server, dashboard, and A2A routes

`darius serve` and `darius a2a` are removed CLI tokens. The router below is source-visible in [`crates/darius-web/src/lib.rs`](../crates/darius-web/src/lib.rs#L113-L125), but no recovered listener binds it. Every route and generated Agent Card capability is **Unavailable**.

| Method and route / generated claim | Status |
| --- | --- |
| `GET /` (dashboard) | **Unavailable** — dashboard goal is explicitly unavailable. |
| `GET /api/events` | **Unavailable** |
| `POST /api/goal` | **Unavailable** — dashboard goal is explicitly unavailable. |
| `GET /a2a/card` | **Unavailable** |
| `POST /a2a/tasks` | **Unavailable** |
| `GET /a2a/tasks/{id}` | **Unavailable** |
| `POST /a2a/peer` | **Unavailable** — peer A2A is explicitly unavailable. |
| `GET /a2a/inbox/{handle}` | **Unavailable** — peer A2A is explicitly unavailable. |
| Agent-card capabilities: `cognitive_loop`, `memory_search`, `tool_execution`, `task_board`, `peer_a2a` | **Unavailable** — generated metadata is not deployment or public-route proof. |

## Installer and platform claims

The shell installer constructs a release asset named `darius-${OS}-${ARCH}.tar.gz` ([`install.sh`](../install.sh#L13-L48)). It does not prove those assets exist, download, verify, or execute.

| Installer target / install path | Status |
| --- | --- |
| `darwin-x86_64` | **Unavailable** — source naming only; no release/install proof. |
| `darwin-aarch64` | **Unavailable** — source naming only; no release/install proof. |
| `linux-x86_64` | **Unavailable** — source naming only; no release/install proof. |
| `linux-aarch64` | **Unavailable** — source naming only; no release/install proof. |
| `cargo install --git https://github.com/galaxycoils/darius darius-cli` fallback | **Unavailable** — documentation/script claim, not an exercised install. |
| Windows binary, installer, or remote-sandbox support | **Unavailable** — explicitly unavailable. |

## Machine-visible product claims that are not capability proof

The following claims occur in [`README.md`](../README.md#L119-L133), the generated web Agent Card, or legacy source. They remain **Unavailable** unless and until a recovery-surface test proves the corresponding user path:

- Lean-tail context compression; disk spill recall; live metrics/fuzzy palette.
- Live subagent steer/list/stop and schema validation.
- Cron continuity/notepads; approval dry-run; instruction-file protection; secret redaction.
- Prompt-cache coordination; MCP health/step gating; peer A2A messaging.
- Worktree lifecycle and automatic rollback; dynamic role model overrides.
- “Local-first,” “provider-optional,” “zero API keys required,” “live provider,” “web dashboard,” “A2A server,” “deployed,” or “working” claims beyond the verified bare-launch and command-contract facts above.

## Drift guard

When changing the public surface, update this file in the same change. The guard is intentionally source-oriented: it fails if any recovery command, removed token, slash command, runtime-built-in tool, web route, or installer asset family is missing from this inventory. It does **not** promote an item to Verified.

```sh
python3 - <<'PY'
from pathlib import Path
import re

root = Path('.')
doc = (root / 'docs/CAPABILITIES.md').read_text()
checks = {
    'recovery CLI': (root / 'crates/darius-cli/src/lib.rs').read_text(),
    'slash registry': (root / 'crates/darius-tui/src/commands.rs').read_text(),
    'tool registrations': (root / 'crates/darius-tools/src/lib.rs').read_text(),
    'web routes': (root / 'crates/darius-web/src/lib.rs').read_text(),
}
needles = set(re.findall(r'"(tui|run|config|memory|daemon|status|start|stop|attach|eval|learn|session-smoke|serve|a2a|cron|approval-check|help)"', checks['recovery CLI']))
needles |= set(re.findall(r'name: "(/[^" ]+)"', checks['slash registry']))
needles |= set(re.findall(r'register_with_risk\("([a-z_]+)"', checks['tool registrations']))
needles |= set(re.findall(r'\.route\("([^"{]+(?:\{id\}|\{handle\})?)"', checks['web routes']))
needles |= {'darwin-x86_64', 'darwin-aarch64', 'linux-x86_64', 'linux-aarch64'}
missing = sorted(item for item in needles if item not in doc)
assert not missing, f'CAPABILITIES.md missing inventory entries: {missing}'
print(f'capability drift check: PASS ({len(needles)} source inventory needles)')
PY
```
