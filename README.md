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
| `Shift+Tab` | Cycle mode (auto → manual → accept-edits → plan) |
| `Esc` | Close palette / interrupt |
| `q` | Quit |

## TUI Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/clear` | Clear transcript |
| `/compact` | Compact session context (lean-tail compression) |
| `/model` | Show/set model |
| `/mode` | Cycle interaction mode |
| `/effort` | Set effort level |
| `/permissions` | View permission policy |
| `/memory` | Memory search & stats |
| `/pack` | Build bounded MemoryPack |
| `/tasks` | Show task board |
| `/plan` | Enter plan mode |
| `/status` | Session status & live cache/memory metrics |
| `/config` | Show effective profile config |
| `/skills` | List skills |
| `/a2a` | A2A card info & peer inbox |
| `/serve` | Start localhost server |
| `/stop` | Stop current operation |
| `/quit` | Exit TUI |

## CLI Commands

| Command | Description |
|---------|-------------|
| `darius run "goal"` | Cognitive loop (Mock or live if configured) |
| `darius tui` | Launch Claude-Code-style TUI |
| `darius serve` | Start web dashboard + A2A server |
| `darius session-smoke` | Integrated daemon + session + handoff test |
| `darius cron list\|add\|run\|notepad` | Cron jobs with memory continuity & notepads |
| `darius approval-check <tool> [args]` | Dry-run check tool execution approval requirements |
| `darius memory search <q>` | FTS5 search |
| `darius memory pack` | Bounded MemoryPack (≤3500 chars) |
| `darius memory import <file>` | Deduped JSONL import |
| `darius memory export <file>` | JSONL export |
| `darius memory stats` | Record count + DB path |
| `darius config show` | Show profile config |
| `darius a2a card` | Show A2A agent card |

## What's in v1.2.0

- ✅ **Lean-Tail Context Compression**: Keeps head and tail pinned while rolling middle context under budget.
- ✅ **Disk Spill Recall (`spill_read`)**: Large tool payloads (>32 KiB) spill to disk; paginated recall without RAM residency.
- ✅ **Live Subagent Steer / List / Stop & Schema Validation**: In-process subagent supervision with JSON Schema enforcement.
- ✅ **Cron Memory Continuity & Notepad**: Scheduled recurring jobs with persistent notepad and change-detection hashing.
- ✅ **Instruction-File Write Protection**: Gated writes for `AGENTS.md`, `SKILL.md`, `skills/`, `memory.db`, `.darius/`.
- ✅ **Secret Redaction**: Automatic scrubbing of `sk-` keys, Bearer tokens, and secrets across logs and UI events.
- ✅ **Prompt-Cache Coordinator**: Deterministic prefix hashing and hit/miss token tracking.
- ✅ **MCP Thin Registry & Health**: Model Context Protocol stdio/SSE client with ping health checks and step gating.
- ✅ **Peer A2A Messaging**: Direct agent-to-agent envelope delivery with recipient inbox and rate limit quotas.
- ✅ **Live Metrics & Fuzzy Palette**: Real-time cache ratio, memory size, and subagent counters in status bar with fuzzy matching.
- ✅ **Worktree Management & Rollback**: Isolated git worktrees with automatic session rollback and TTL pruning.
- ✅ **Approval Dry-Run CLI**: `darius approval-check` utility to inspect tool risk without executing.
- ✅ **Dynamic Role Model Overrides**: Profile config support for `planner`, `rater`, `smol`, and `advisor` roles.

## Build & Test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p darius-cli
```

## License

MIT
