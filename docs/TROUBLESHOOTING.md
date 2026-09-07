# Darius Troubleshooting and Operational Guide

This guide explains setup modes, provider configuration, runtime permissions, execution policies, diagnostics, and retained commands in Darius.

## 1. First-Run and Setup

### Clean-Home Bare Launch
When launched without an existing profile or configuration (e.g. clean `~/.darius`), Darius detects the unconfigured state:
- In non-TTY mode: prints a concise setup hint and exits 0 without writing files.
- In interactive TTY mode: presents the setup screen guiding you through profile initialization.

### Initializing a Profile
To initialize default configuration files:
```sh
darius config init
```
This generates the profile directory structure under `~/.darius/profiles/default/config.toml`.

## 2. Live Provider vs. Offline Mock

Darius supports two operational modes for model inference:

### Offline Mock Model (Default)
When no API key or provider is configured, Darius runs with a local `MockModel`. This allows testing the full agent loop, tool execution, memory, and permissions completely offline with zero API keys or network access.

### Configured OpenAI-Compatible Provider
To use a real model:
1. Configure `~/.darius/profiles/default/config.toml`:
```toml
[model]
provider = "openai_compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "DARIUS_API_KEY"
```
2. Set the corresponding environment variable:
```sh
export DARIUS_API_KEY="sk-..."
```
3. Test your configuration:
```sh
darius config show
```

Common issues:
- **Connection Refused or Timeout:** Ensure `base_url` is accessible and includes protocol (e.g., `https://` or `http://127.0.0.1:11434/v1`).
- **HTTP 401 Unauthorized:** Verify `DARIUS_API_KEY` is set and valid.
- **HTTP 429 Rate Limit:** The provider is throttling requests; wait or check quotas.

## 3. Execution Policies: Auto vs. Plan

Darius enforces two strict execution policies:

- **Auto Mode:**
  - Read-only tools (`read_file`, `search_files`, `memory_search`, etc.) execute automatically.
  - Mutating tools (`write_file`) and shell execution (`shell`) require user permission before execution.
- **Plan Mode:**
  - Read-only tools execute normally to gather context.
  - Mutating tools and shell commands are strictly denied at the runtime policy boundary. No disk modifications or shell commands can be executed in Plan mode.

### Toggling Modes
- In the TUI, press `Shift+Tab` to toggle between Auto and Plan modes.
- Alternatively, type `/mode auto` or `/mode plan` in the composer or command palette.

## 4. Permission Prompts

When an agent attempts a mutating tool or shell command in Auto mode:
1. **Interactive Prompt:** An in-terminal dialog presents the tool name, target/arguments, and three choices:
   - `[1] Allow Once`: Grants execution for this single invocation.
   - `[2] Allow for Session`: Caches approval for this exact tool and target for the rest of the session.
   - `[3] Deny`: Denies tool execution; the denial is returned as a correlated tool result to the agent so it can adapt.
2. **Dismissing Prompts:** Pressing `Esc` defaults to `Deny`. Pressing `Ctrl+C` cancels the entire active turn.
3. **Noninteractive Denials:** In noninteractive CLI mode (`darius run ...`), any mutating or shell tool request is automatically denied with exit code 1 and guidance to use `darius tui` for interactive authorization.

## 5. Doctor and Diagnostics

To diagnose environment or configuration issues:
- Check current active configuration:
  ```sh
  darius config show
  ```
- Check memory database statistics:
  ```sh
  darius memory stats
  ```
- Check terminal restoration:
  If the terminal state is corrupted on exit due to an unexpected process termination, run:
  ```sh
  reset
  ```
  Darius installs RAII `TerminalGuard` and panic hooks to restore raw mode and the alternate screen cleanly upon any exit or signal.

## 6. Retained Commands Summary

### CLI Subcommands (4 canonical subcommands):
- `darius tui [--cwd <path>]`: Launch interactive Claude-Code-style terminal session.
- `darius run <goal>`: Run agent loop noninteractively (denies mutations without TTY).
- `darius config [show|init]`: Inspect or initialize profile configuration.
- `darius memory [search|pack|import|export|stats]`: Manage persistent memory database.

Note: Removed/legacy commands (`daemon`, `status`, `start`, `stop`, `attach`, `eval`, `learn`, `session-smoke`, `serve`, `a2a`, `cron`, `approval-check`) have been retired and exit with code 2.

### TUI Slash Commands (13 canonical commands):
- `/help`: Display available commands.
- `/clear`: Clear transcript display.
- `/compact`: Compact session context window.
- `/model`: Display active model and provider information.
- `/mode [auto|plan]`: Switch between Auto and Plan execution modes.
- `/permissions`: Display session permissions.
- `/memory`: Inspect memory database.
- `/pack`: Build and preview bounded memory pack.
- `/tasks`: Inspect task board.
- `/status`: Display session status and metrics.
- `/config`: Display effective profile configuration.
- `/stop`: Interrupt active model generation or tool execution.
- `/quit`: Exit TUI session.
