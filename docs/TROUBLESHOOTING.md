# Setup and troubleshooting

## Setup, live, and offline-demo

A clean unconfigured launch enters setup. Bare non-TTY invocation prints a setup hint; bare TTY invocation restores the terminal after setup guidance. Neither is proof of analysis. `darius tui` starts the interactive surface.

Initialize a profile explicitly (fastest: keyless `opencode-free`):

```sh
darius config preset opencode-free
darius models
export OPENROUTER_API_KEY="your-key"
darius config preset openrouter
darius run "inspect the repository"
```

`config init` requires all four fields and refuses overwriting unless `--force` is supplied. It stores a key environment-variable name, not the key. Default storage is `~/.darius/profiles/default`; `DARIUS_HOME` and global `--profile` change it. Use `--cwd` to select the workspace. `config preset kilocode|opencode-go|opencode-free|nous|openrouter|nvidia` writes example settings; it does not test the service. `config setup` walks through provider choice (recommending keyless `opencode-free`), writes keys to `~/.darius/.env`, and shows the live model list.

With no configuration, darius enters setup guidance: bare environment keys never auto-select a provider. A configured credential-required endpoint with a missing/blank key reports missing-key rather than choosing a mock. Local/no-key configurations may select live without a key; this is not authentication or connectivity verification. Without either, setup is required. Only explicit `darius --offline run "demo"` or `darius --offline tui` selects the labelled MockModel demo. Demo output is not successful analysis or completed work.

A `live` state means a provider configuration/key was selected; it does not establish network availability. Hosted credentials and model availability must be validated separately. HTTP 401 suggests key/auth problems; 429 suggests provider limits; connection errors require checking the endpoint. Native Anthropic requests and automatic provider failover are unavailable.

## Provider-specific failures

- **OpenCode Free 401:** the relay rejects *any* `Authorization: Bearer` value. Darius sends an explicit empty `Authorization` header for `opencode-free`. If a 401 names this provider, check that no proxy or wrapper injects a bearer token, and confirm the profile provider is exactly `opencode-free` (not a custom `openai_compatible` entry pointing at the same base URL, which sends `Bearer`).
- **OpenCode Go 400/422 on tool calls:** the relay requires string tool-result content. Darius flattens multipart tool content for `opencode-go` only. A 422 naming tool content on another provider is a provider-side schema rejection, not this quirk.
- **NVIDIA 400 on tool calls:** NVIDIA's ToolMessage schema rejects `name`/`tool_name` fields. Darius strips them for `nvidia` only (copy-on-write). NVIDIA defaults to 16384 max output tokens; set `NVIDIA_BASE_URL` to point at a local NIM (for example `http://localhost:8000/v1`).
- **Nous token refresh:** `darius auth status` shows token type, expiry, and quarantine state (never the token). A mid-session 401 triggers one single-flight refresh; a revoked refresh token is quarantined to stop replay loops — re-run `darius auth login nous`. The background keepalive refreshes ahead of expiry in long sessions; `[nous] keepalive_interval_seconds` overrides the 15-minute default (60s floor).
- **OpenRouter routing ignored:** `[openrouter]` `sort`/`only`/`ignore`/`order` map to the request `provider_routing` body for the `openrouter` profile only. The `nous` profile never forwards them (the Portal rejects them with 400). An empty `[openrouter]` section sends no routing key at all.
- **Stale model IDs:** OpenCode free-tier model names rotate. `darius models --provider opencode-free` shows the live list with its source (`live`, `cached`, or registry `fallback`); the picker falls back to the registry list offline.

## Auto, Plan and permissions

Auto executes read-only tools and prompts before mutating or shell tools. Plan denies mutating and shell tools at the tool-policy boundary; session/memory bookkeeping may still write local state. Approved shell commands are not OS-sandboxed.

Use Shift+Tab or `/mode auto` / `/mode plan`. Permission dialogs offer Allow Once, Allow for Session and Deny. Session approval is scoped to the tool and target, not every future action. Esc denies the prompt; Ctrl+C interrupts the active turn. Noninteractive `run` denies approval-requiring tools and returns failure with TUI guidance.

## Doctor workflow (not a standalone command)

```sh
darius config show
darius memory stats
```

`config show` reports runtime state, config parse/path, key presence, memory state and provider URL; it is not a remote health probe. Explicit config/memory operations may create the profile directory and SQLite database. In the TUI use `/status`, `/config`, `/permissions`, and read-only `/model`. Do not paste secret keys into diagnostic reports.

## Interrupt and terminal recovery

`/stop` or Ctrl+C interrupts an active turn; `/quit` exits. Ordinary `q` is text, not an exit shortcut. Esc closes the palette or denies a pending permission prompt.

Terminal guards attempt restoration on supported normal/error paths. Tests cover selected exit paths, not every signal, external process, or cleanup duration. SIGKILL cannot run cleanup hooks. If a terminal remains corrupted after an unexpected termination, run `reset` in that terminal.

## Supported commands and unavailable surfaces

CLI: `tui`, `run <goal>`, `config show|init|preset`, `memory search|pack|import|export|stats`. Nested `--help` describes required arguments; `config` and `memory` alone are not successful operations.

Slash commands: `/help`, `/clear`, `/compact`, `/model`, `/mode`, `/permissions`, `/memory`, `/pack`, `/tasks`, `/status`, `/config`, `/stop`, `/quit`.

Retired/unavailable: `daemon`, `status`, `start`, `stop`, `attach`, `eval`, `learn`, `session-smoke`, `serve`, `a2a`, `cron`, `approval-check`, `peer_send`, MCP, subagent orchestration and worktree rollback. Web goal/task/peer routes return unavailable, not synthetic success; the agent card advertises no executable capabilities. There is no public web listener.

## Evidence and release limits

See [CAPABILITIES.md](CAPABILITIES.md) for narrowly scoped named tests and [CHANGELOG.md](../CHANGELOG.md) for corrected v1.1.2 claims. A successful local claims audit does not mean a release has been published, installed on every platform, or tested against a credentialed provider. Run the separate release/installer/PTY gates before making those claims.
