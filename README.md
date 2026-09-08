# Darius v1.2.1

Terminal coding-agent harness with an OpenAI-compatible adapter, explicit offline demo, permission prompts, and SQLite memory. Local session storage does not mean inference stays local: configured providers receive prompts and tool results.

## Build and launch

```sh
cargo build --release -p darius-cli
./target/release/darius --help
./target/release/darius tui
```

An unconfigured launch shows setup guidance, not simulated analysis. Prebuilt assets and the source installer require separate release verification; this checkout does not establish that v1.2.0 has been published. See [capabilities](docs/CAPABILITIES.md).

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

The CLI exposes `tui`, `run <goal>`, `config show|init|preset`, and `memory search|pack|import|export|stats`. Inspect nested `--help` for required arguments. Explicit config/memory operations may initialize local storage.

## Corrected prior claims

Earlier v1.1.2 and v1.2.0 draft claims for MCP, subagent orchestration, cron, approval-check, peer_send, worktree rollback, web execution and A2A are retired and unavailable. The compatibility web router exposes no executable capabilities or active goal controls. Historical corrections are recorded in [CHANGELOG.md](CHANGELOG.md).

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
