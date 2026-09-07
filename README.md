# Darius v1.2.0

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

### 1. View help and subcommands (no API key needed)

```sh
darius --help
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

[model_overrides]
planner = "gpt-4o"
rater = "claude-3-5-sonnet"
smol = "gpt-4o-mini"
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
| `/` | Open command palette (with fuzzy matching) |
| `-` at column zero | Also opens command palette |
| `Shift+Tab` | Cycle mode (auto → plan) |
| `Esc` | Close palette / interrupt |
| `q` | Quit |

## TUI Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/clear` | Clear transcript |
| `/compact` | Compact session context (lean-tail compression) |
| `/model` | Show current provider/model (read-only) |
| `/mode [auto|plan]` | Set Auto or Plan mode |
| `/permissions` | Show the current permission policy |
| `/memory` | Memory search & stats |
| `/pack` | Build bounded MemoryPack |
| `/tasks` | Show task board |
| `/status` | Session status & live cache/memory metrics |
| `/config` | Show effective profile config |
| `/stop` | Stop current operation |
| `/quit` | Exit TUI |

## CLI Commands

| Command | Description |
|---------|-------------|
| `darius run "goal"` | Cognitive loop (Mock or live if configured) |
| `darius tui` | Launch Claude-Code-style TUI |
| `darius config show` | Show profile config |
| `darius config init` | Initialize profile configuration |
| `darius memory search <q>` | FTS5 search |
| `darius memory pack` | Bounded MemoryPack (≤3500 chars) |
| `darius memory import <file>` | Deduped JSONL import |
| `darius memory export <file>` | JSONL export |
| `darius memory stats` | Record count + DB path |

## What's in v1.2.0

- ✅ **Correlated Multi-Turn Agent Loop**: Full OpenAI-compatible adapter supporting function tool calls with strict ID correlation.
- ✅ **Async Responsive TUI Runtime**: Decoupled session actor handling interrupts, permission dialogs, and clean terminal exits under 2 seconds.
- ✅ **Execution Policies & Permissions**: Hard-enforced Auto and Plan modes with interactive AllowOnce/AllowSession/Deny approval prompts.
- ✅ **Clean-Home Diagnostics & Honest Onboarding**: Detects unconfigured environment with truthful hints, zero home pollution.
- ✅ **Canonical Command Registry**: Closed-world set of 13 slash commands and 4 CLI subcommands with fuzzy matching and autocomplete.
- ✅ **Durable SQLite Memory Engine**: FTS5 full-text search, bounded pack generation, and JSONL import/export.
- ✅ **Safe Sandboxed Tools**: Strictly validated file read/write, file search, and truthfully cancellable shell tool execution.
- ℹ️ **Note on Prior Version Overclaims**: Unverified features advertised in earlier v1.1.2/v1.2.0 drafts (e.g. MCP thin client, subagent orchestration, A2A hub, cron scheduling, worktree rollback) have been formally retired from the public surface to ensure complete operational truth. See [CAPABILITIES.md](docs/CAPABILITIES.md) and [TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

## Build & Test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p darius-cli
```

## License

MIT
