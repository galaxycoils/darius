# Setup and troubleshooting

## Setup, live, and offline-demo

A clean unconfigured launch enters setup. Bare non-TTY invocation prints a setup hint; bare TTY invocation restores the terminal after setup guidance. Neither is proof of analysis. `darius tui` starts the interactive surface.

Initialize a profile explicitly:

```sh
darius config init --provider openai_compatible --base-url https://api.openai.com/v1 \
  --model gpt-4o-mini --key-env DARIUS_API_KEY
export DARIUS_API_KEY="your-key"
darius run "inspect the repository"
```

`config init` requires all four fields and refuses overwriting unless `--force` is supplied. It stores a key environment-variable name, not the key. Default storage is `~/.darius/profiles/default`; `DARIUS_HOME` and global `--profile` change it. Use `--cwd` to select the workspace. `config preset openai|openrouter|ollama|groq` writes example settings; it does not test the service.

Without configuration, a usable `DARIUS_API_KEY` takes precedence over `OPENAI_API_KEY` for the default OpenAI-compatible configuration. A configured credential-required endpoint with a missing/blank key reports missing-key rather than choosing a mock. Local/no-key configurations may select live without a key; this is not authentication or connectivity verification. Without either, setup is required. Only explicit `darius --offline run "demo"` or `darius --offline tui` selects the labelled MockModel demo. Demo output is not successful analysis or completed work.

A `live` state means a provider configuration/key was selected; it does not establish network availability. Hosted credentials and model availability must be validated separately. HTTP 401 suggests key/auth problems; 429 suggests provider limits; connection errors require checking the endpoint. Native Anthropic requests and automatic provider failover are unavailable.

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
