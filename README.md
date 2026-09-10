# Darius v1.3.0

Terminal coding-agent harness with an OpenAI-compatible adapter, explicit offline demo, permission prompts, SQLite memory, and a loopback web server. Local session storage does not mean inference stays local: configured providers receive prompts and tool results.

## Build and launch

```sh
cargo build --release -p darius-cli
./target/release/darius --help
./target/release/darius tui
```

An unconfigured launch shows setup guidance, not simulated analysis. Prebuilt assets and the source installer require separate release verification; this checkout does not establish that a release has been published. See [capabilities](docs/CAPABILITIES.md).

## Configure a provider

```sh
./target/release/darius config init --provider openai_compatible \
  --base-url https://api.openai.com/v1 --model gpt-4o-mini --key-env DARIUS_API_KEY
export DARIUS_API_KEY="your-key"
./target/release/darius run "summarize this repository"
```

Initialization writes `~/.darius/profiles/default/config.toml` and refuses replacement unless `--force` is passed. `--profile`, `--cwd`, and `--offline` are global options. `DARIUS_HOME` relocates profile storage. `config preset <name>` writes example settings, not proof of provider availability. Model identifiers and endpoint compatibility must be checked with your provider.

Missing keys for credential-required configurations cause an error; there is no silent mock fallback. Local/no-key configurations can select live without a key, which is not proof of connectivity. With no configuration, a usable `DARIUS_API_KEY` or `OPENAI_API_KEY` selects the default OpenAI-compatible configuration. Only an explicit `--offline` selects the labelled `MockModel` demo:

```sh
./target/release/darius --offline run "demo"
```

Demo output is not repository analysis or completed work. Local fake-provider tests verify the wire contract; they do not verify a hosted service or every model.

## Controls and permissions

Ordinary text (including `q`) is typed into the composer. Enter submits; `/` or a leading `-` opens the palette; Shift+Tab switches Auto/Plan; Ctrl+C interrupts; `/quit` exits. Esc closes the palette or denies a permission prompt.

Auto runs read-only tools and asks before mutating tools or shell execution. Plan denies those tool classes. Noninteractive `run` denies requests needing approval. Shell approval is not an OS sandbox. Cleanup is tested for selected exit paths, not a timing SLA or protection against SIGKILL.

The supported slash commands are `/help`, `/clear`, `/compact`, `/model`, `/mode`, `/permissions`, `/memory`, `/pack`, `/tasks`, `/status`, `/config`, `/stop`, and `/quit`. `/model` opens an interactive picker or selects from catalog; `/mode auto` and `/mode plan` change policy. See [operating guidance](docs/TROUBLESHOOTING.md).

The CLI exposes `tui`, `run <goal>`, `serve`, `config show|init|preset`, and `memory search|pack|import|export|stats`. Inspect nested `--help` for required arguments. Explicit config/memory operations may initialize local storage.


## MCP server configuration

Darius supports local MCP (Model Context Protocol) servers over stdio and SSE transports. Configure servers in your profile's `config.toml`:

```toml
[[mcp.servers]]
name = "mock"
type = "stdio"
command = "python3"
args = ["/path/to/server.py"]
env = { "ENV_VAR" = "value" }
timeout_ms = 30000

[[mcp.servers]]
name = "remote"
type = "sse"
url = "https://example.com/mcp"
headers = { "Authorization" = "Bearer token" }
```

Discovered tools are registered with namespaced identifiers: `mcp_{server}_{tool}`. Tools default to `Mutating` risk and require user approval in Auto mode unless the tool metadata specifies a `read_only` hint. Model tool visibility is enforced via a session-scoped dynamic allowlist populated strictly from connected servers on startup, not a static wildcard.

## Write diff preview

Approved `write_file` operations on existing non-empty files generate unified diff previews in the TUI transcript with line-level addition and deletion markers (capped at 200 lines). First-time file creation produces a summary-only preview.
## Serve goals over HTTP

`darius serve` binds `127.0.0.1:7432` by default and runs the same policy-aware agent loop as `run`. It refuses to start without a live provider; `--offline` never executes goals. Headless web execution denies mutating tools, so approve writes and shell work in the TUI instead.

```sh
./target/release/darius serve &
curl -s http://127.0.0.1:7432/a2a/card
curl -s -X POST http://127.0.0.1:7432/api/goal \
  -H 'Content-Type: application/json' \
  -d '{"goal":"summarize this repository"}'
curl -N 'http://127.0.0.1:7432/api/events?task_id=<id-from-goal-response>'
curl -s -X POST http://127.0.0.1:7432/a2a/tasks \
  -H 'Content-Type: application/json' \
  -d '{"goal":"summarize this repository"}'
curl -s http://127.0.0.1:7432/a2a/tasks/<id>
```

Without a configured provider the server exits with an error instead of simulating work. Event streams replay the per-task journal, so a slow client still sees every event up to the terminal `Done` or `Error`.

## Corrected prior claims

Earlier v1.1.2 and v1.2.0 draft claims for MCP, subagent orchestration, cron, approval-check, peer_send, worktree rollback, and peer messaging fleets are retired and unavailable. The web router without an injected runtime exposes no executable capabilities. Historical corrections are recorded in [CHANGELOG.md](CHANGELOG.md).

## Verify this checkout

```sh
bash scripts/audit-public-claims.sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The audit checks claims and local contracts; it is not publication, cross-platform installation, or credentialed provider evidence.

## License

MIT
