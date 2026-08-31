# Darius v1.1.1

**Open-source lean agent harness** — Claude-Code-style TUI, durable memory, tool ACI, plan–execute–accept loop. Local-first, provider-optional, zero API keys required to get started.

## Install

### Option A: Pre-built binary

```sh
curl -sSL https://github.com/galaxycoils/darius/releases/latest/download/install.sh | bash
```

### Option B: From source

```sh
cargo install --git https://github.com/galaxycoils/darius darius-cli
```

## Quickstart

### 1. Run the smoke test (no API key needed)

```sh
darius session-smoke
```

### 2. Launch the TUI

```sh
darius tui
```

### 3. Use memory

```sh
darius memory stats
```

### 4. Configure a live provider (optional)

```sh
mkdir -p ~/.darius/profiles/default
cat > ~/.darius/profiles/default/config.toml << 'EOF'
[model]
provider = "openai_compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "DARIUS_API_KEY"
EOF

export DARIUS_API_KEY="sk-your-key-here"
```

### 5. Run with a real goal

```sh
darius run "analyze this codebase and summarize the architecture"
```

Without `DARIUS_API_KEY`, `darius run` uses the offline `MockModel` — useful for testing the loop without network.

## TUI Keyboard Reference

| Key | Action |
|-----|--------|
| `❯ text` + Enter | Send a message |
| `/` | Open command palette |
| `-` at column zero | Also opens palette |
| `Shift+Tab` | Cycle mode (auto → manual → accept-edits → plan) |
| `Esc` | Close palette / interrupt |
| `q` | Quit |

## TUI Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/clear` | Clear transcript |
| `/compact` | Compact context |
| `/model` | Show/set model |
| `/mode` | Cycle interaction mode |
| `/effort` | Set effort level |
| `/permissions` | View permission policy |
| `/memory` | Memory stats |
| `/pack` | Build MemoryPack |
| `/tasks` | Show task board |
| `/plan` | Show current plan |
| `/status` | Session status |
| `/config` | Show/set config |
| `/skills` | List skills |
| `/a2a` | A2A card info |
| `/serve` | Start localhost server |
| `/stop` | Stop current operation |
| `/quit` | Exit TUI |

## Modes

| Mode | Indicator | Behavior |
|------|-----------|----------|
| Auto | ⏵⏵ auto | Full autonomous execution |
| Manual | ⏸ manual | Pause after each step |
| Accept Edits | ⏵⏵ accept edits | Auto-accept file edits |
| Plan | ⏸ plan | Planning only, no execution |

## CLI Commands

| Command | Description |
|---------|-------------|
| `darius run "goal"` | Cognitive loop (Mock or live if configured) |
| `darius tui` | Launch Claude-Code-style TUI |
| `darius serve` | Start web dashboard + A2A server |
| `darius session-smoke` | Integrated daemon + session + handoff test |
| `darius memory search <q>` | FTS5 search |
| `darius memory pack` | Bounded MemoryPack (≤3500 chars) |
| `darius memory import <file>` | Deduped JSONL import |
| `darius memory export <file>` | JSONL export |
| `darius memory stats` | Record count + DB path |
| `darius config show` | Show profile config |
| `darius a2a card` | Show A2A agent card |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `DARIUS_API_KEY` | API key for live provider |
| `DARIUS_PROFILE` | Profile name (default: `default`) |
| `DARIUS_LIVE_MODEL` | Force live model for `darius run` |

## Architecture

```
darius-memory     → SQLite FTS5, MemoryPack, JSONL
darius-tools      → ToolRegistry, TOOL line protocol, spill
darius-cognitive  → Plan → TaskBoard → ReAct → Accept
darius-daemon     → Session, event log, handoff, model router
darius-tui        → Claude-Code-style terminal UI (ratatui)
darius-web        → Axum web dashboard + A2A server
darius-cli        → CLI surface + session runtime
```

### Lean resource caps

| Resource | Cap |
|----------|-----|
| MemoryPack | 3500 chars |
| Tool preview | 32 KiB (+ spill to disk) |
| TaskBoard | 15 tasks |
| ReAct iters | 12 per task |
| Body size | 32 KiB per record |

## What's in v1.1.1

- ✅ Claude-Code-style TUI with streaming turns, command palette, modes, effort, todos, diffs, permission chooser
- ✅ Offline MockModel (no network)
- ✅ Live OpenAI-compatible provider when configured
- ✅ Durable SQLite memory with FTS5 search
- ✅ Plan–execute–accept cognitive loop
- ✅ CLI with memory operations, TUI, serve, A2A
- ✅ Session handoff + event replay
- ✅ Web dashboard + A2A server (SSE)
- ✅ Unified UiEvent/runtime across CLI, TUI, web, A2A

## Design inspiration

Visual/interaction grammar inspired by [Brainless](https://brainless.swerdlow.dev/). No Brainless source code was copied or translated.

## Build & Test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p darius-cli
```

## License

MIT
