# Darius v1.0.0

**Open-source lean agent harness** — durable memory, tool ACI, plan–execute–accept loop. Local-first, provider-optional, zero API keys required to get started.

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

### 2. Use memory

```sh
# Memory is stored in ~/.darius/profiles/default/memory.db (SQLite with FTS5)
darius memory stats
```

### 3. Configure a live provider (optional)

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

### 4. Run with a real goal

```sh
darius run "analyze this codebase and summarize the architecture"
```

Without `DARIUS_API_KEY`, `darius run` uses the offline `MockModel` — useful for testing the loop without network.

## CLI Commands

| Command | Description |
|---------|-------------|
| `darius run "goal"` | Cognitive loop (Mock or live if configured) |
| `darius session-smoke` | Integrated daemon + session + handoff test |
| `darius memory search <q>` | FTS5 search |
| `darius memory pack` | Bounded MemoryPack (≤3500 chars) |
| `darius memory import <file>` | Deduped JSONL import |
| `darius memory export <file>` | JSONL export |
| `darius memory stats` | Record count + DB path |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `DARIUS_API_KEY` | API key for live provider |
| `DARIUS_PROFILE` | Profile name (default: `default`) |

## Architecture

```
darius-memory     → SQLite FTS5, MemoryPack, JSONL
darius-tools      → ToolRegistry, TOOL line protocol, spill
darius-cognitive  → Plan → TaskBoard → ReAct → Accept
darius-daemon     → Session, event log, handoff
darius-cli        → CLI surface
```

### Lean resource caps

| Resource | Cap |
|----------|-----|
| MemoryPack | 3500 chars |
| Tool preview | 32 KiB (+ spill to disk) |
| TaskBoard | 15 tasks |
| ReAct iters | 12 per task |
| Body size | 32 KiB per record |

## What's in v1

- ✅ Offline MockModel (no network)
- ✅ Live provider when configured
- ✅ Durable SQLite memory with FTS5 search
- ✅ Plan–execute–accept cognitive loop
- ✅ CLI with memory operations
- ✅ Session handoff + event replay

## What's NOT in v1

- Multi-platform messaging (Telegram/Discord)
- Jupyter/ZMQ backend
- gVisor/Firecracker isolation
- Embeddings/vector DB
- Cloud sync
- Training/fine-tuning

## Build & Test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p darius-cli
```

## License

MIT
