#[path = "run_fixtures/http.rs"]
mod http_fixture;
mod support;

use assert_cmd::Command as AssertCommand;
use std::time::Duration;
use support::DariusHomeSnapshot;
use support::fake_provider::FakeProvider;

const KEY_ENV: &str = "DARIUS_RUN_KEY";
use tempfile::TempDir;

struct TestContext {
    home: TempDir,
    workspace: TempDir,
    snapshot: DariusHomeSnapshot,
}

impl TestContext {
    fn new() -> Self {
        let snapshot = DariusHomeSnapshot::capture();
        let home = TempDir::new().expect("create temp home");
        let workspace = TempDir::new().expect("create temp workspace");
        Self {
            home,
            workspace,
            snapshot,
        }
    }

    fn command(&self) -> AssertCommand {
        let mut cmd = AssertCommand::cargo_bin("darius").expect("darius binary not found");
        cmd.env("DARIUS_HOME", self.home.path())
            .env("HOME", self.home.path())
            .env("DARIUS_WORKSPACE", self.workspace.path())
            .env_remove("DARIUS_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env_remove("OPENROUTER_API_KEY")
            .env_remove("GROQ_API_KEY")
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("GOOGLE_API_KEY")
            .env_remove("GEMINI_API_KEY")
            .env_remove("MISTRAL_API_KEY")
            .env_remove("XAI_API_KEY")
            .env_remove("DEEPSEEK_API_KEY")
            .env_remove("NONE")
            .env_remove(KEY_ENV)
            .env_remove("DARIUS_PROFILE")
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("all_proxy")
            .env("NO_PROXY", "127.0.0.1,localhost")
            .timeout(Duration::from_secs(8))
            .current_dir(self.workspace.path());
        cmd
    }

    fn write_profile_config(&self, profile: &str, provider_url: &str, key_env: &str) {
        let profile_dir = self.home.path().join("profiles").join(profile);
        std::fs::create_dir_all(&profile_dir).expect("create profile dir");
        let config = format!(
            "[model]\nprovider = \"custom-provider\"\nbase_url = \"{provider_url}/v1\"\nmodel = \"custom-model\"\napi_key_env = \"{key_env}\"\n"
        );
        std::fs::write(profile_dir.join("config.toml"), config).expect("write config.toml");
    }

    fn assert_clean_home(&self, label: &str) {
        self.snapshot.assert_unchanged(label);
    }
}

#[test]
fn test_run_read_goal_succeeds() {
    let ctx = TestContext::new();
    let provider = FakeProvider::start();
    let key_env = "DARIUS_RUN_KEY";
    let key_val = "run-secret-123";

    ctx.write_profile_config("default", provider.url(), key_env);

    // Create a workspace file for read_file
    let readme_path = ctx.workspace.path().join("readme.txt");
    std::fs::write(&readme_path, "Darius documentation test\n").expect("write readme.txt");

    provider.push_tool_call(
        "call-read",
        "read_file",
        serde_json::json!({"path": "readme.txt"}),
    );
    provider.push_text("Successfully read readme: Darius documentation test");

    let mut cmd = ctx.command();
    cmd.env(key_env, key_val)
        .arg("run")
        .arg("read the readme file");

    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("Successfully read readme: Darius documentation test"),
        "expected model text in stdout, got: {stdout}"
    );

    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 2, "read must return to the model");
    for request in &requests {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/chat/completions");
        assert_eq!(request.body["model"], "custom-model");
        assert_eq!(
            request.headers["authorization"],
            format!("Bearer {key_val}")
        );
    }
    let messages = requests[1].body["messages"].as_array().unwrap();
    assert!(
        messages.iter().any(|message| {
            message["role"] == "tool"
                && message["tool_call_id"] == "call-read"
                && message["content"]
                    .as_str()
                    .is_some_and(|text| text.contains("Darius documentation test"))
        }),
        "actual read result missing from follow-up: {messages:?}"
    );
    assert_eq!(
        std::fs::read_to_string(readme_path).unwrap(),
        "Darius documentation test\n"
    );
    assert!(!stdout.contains(key_val));
    ctx.assert_clean_home("test_run_read_goal_succeeds");
}

#[test]
fn test_run_mutation_goal_denied_with_tui_guidance() {
    let ctx = TestContext::new();
    let provider = FakeProvider::start();
    let key_env = "DARIUS_RUN_KEY";
    let key_val = "run-secret-123";

    ctx.write_profile_config("default", provider.url(), key_env);

    // Mutation tool call
    provider.push_tool_call(
        "call-write",
        "write_file",
        serde_json::json!({
            "path": "unauthorized.txt",
            "content": "unauthorized write\n"
        }),
    );

    let mut cmd = ctx.command();
    cmd.env(key_env, key_val)
        .arg("run")
        .arg("create an unauthorized file");

    let assert = cmd.assert().failure().code(1);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains(
            "Mutation denied: run requires interactive approval; use the TUI (`darius tui`)"
        ),
        "expected mutation denial error in stderr, got: {stderr}"
    );

    let unauthorized_file = ctx.workspace.path().join("unauthorized.txt");
    assert!(
        !unauthorized_file.exists(),
        "unauthorized file should not have been created"
    );

    ctx.assert_clean_home("test_run_mutation_goal_denied_with_tui_guidance");
}

#[test]
fn test_config_show_and_init() {
    let ctx = TestContext::new();

    // 1. config init creates the profile
    let mut cmd = ctx.command();
    cmd.args([
        "config",
        "init",
        "--provider",
        "openai",
        "--base-url",
        "https://api.openai.com/v1",
        "--model",
        "gpt-4o",
        "--key-env",
        "OPENAI_API_KEY",
    ]);
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("Initialized profile 'default'"),
        "expected init message, got: {stdout}"
    );

    let config_file = ctx.home.path().join("profiles/default/config.toml");
    assert!(config_file.exists(), "config.toml should exist after init");
    let content = std::fs::read_to_string(&config_file).expect("read config.toml");
    assert!(content.contains("provider = \"openai\""));
    assert!(content.contains("gpt-4o"));

    // 2. config show displays diagnostics
    let mut cmd_show = ctx.command();
    cmd_show.args(["config", "show"]);
    let assert_show = cmd_show.assert().success();
    let stdout_show = String::from_utf8_lossy(&assert_show.get_output().stdout).to_string();
    assert!(
        stdout_show.contains("default") || stdout_show.contains("openai"),
        "expected profile diagnostics in config show, got: {stdout_show}"
    );

    ctx.assert_clean_home("test_config_show_and_init");
}

#[test]
fn test_memory_cli_lifecycle() {
    let ctx = TestContext::new();

    // Create profile dir so memory engine can locate profiles/default
    let profile_dir = ctx.home.path().join("profiles/default");
    std::fs::create_dir_all(&profile_dir).expect("create profile dir");

    // 1. Prepare JSONL file to import
    let import_path = ctx.workspace.path().join("memory_import.jsonl");
    let record1 = serde_json::json!({
        "kind": "Fact",
        "title": "Language",
        "body": "Darius is written in Rust.",
        "tags": [],
        "importance": 0.8,
        "source": null
    });
    let record2 = serde_json::json!({
        "kind": "Decision",
        "title": "Security",
        "body": "All mutations require approval.",
        "tags": [],
        "importance": 0.9,
        "source": null
    });
    let jsonl_content = format!("{record1}\n{record2}\n");
    std::fs::write(&import_path, jsonl_content).expect("write import file");

    // 2. Import
    let mut cmd_import = ctx.command();
    cmd_import.args(["memory", "import", import_path.to_str().unwrap()]);
    let assert_import = cmd_import.assert().success();
    let out_import = String::from_utf8_lossy(&assert_import.get_output().stdout).to_string();
    assert!(
        out_import.contains("Imported: 2, Skipped: 0"),
        "expected 2 imported records, got: {out_import}"
    );

    // 3. Stats
    let mut cmd_stats = ctx.command();
    cmd_stats.args(["memory", "stats"]);
    let assert_stats = cmd_stats.assert().success();
    let out_stats = String::from_utf8_lossy(&assert_stats.get_output().stdout).to_string();
    assert!(
        out_stats.contains("Records: 2"),
        "expected 2 records in stats, got: {out_stats}"
    );

    // 4. Search
    let mut cmd_search = ctx.command();
    cmd_search.args(["memory", "search", "Rust"]);
    let assert_search = cmd_search.assert().success();
    let out_search = String::from_utf8_lossy(&assert_search.get_output().stdout).to_string();
    assert!(
        out_search.contains("Found 1 results") && out_search.contains("Darius is written in Rust."),
        "expected search result for Rust, got: {out_search}"
    );

    // 5. Pack
    let mut cmd_pack = ctx.command();
    cmd_pack.args(["memory", "pack"]);
    let assert_pack = cmd_pack.assert().success();
    let out_pack = String::from_utf8_lossy(&assert_pack.get_output().stdout).to_string();
    assert!(
        out_pack.contains("Memory Pack") && out_pack.contains("records"),
        "expected memory pack output, got: {out_pack}"
    );

    // 6. Export
    let export_path = ctx.workspace.path().join("memory_export.jsonl");
    let mut cmd_export = ctx.command();
    cmd_export.args(["memory", "export", export_path.to_str().unwrap()]);
    let assert_export = cmd_export.assert().success();
    let out_export = String::from_utf8_lossy(&assert_export.get_output().stdout).to_string();
    assert!(
        out_export.contains("Exported 2 records"),
        "expected export message, got: {out_export}"
    );

    assert!(export_path.exists(), "exported jsonl file should exist");
    let exported_content = std::fs::read_to_string(&export_path).expect("read exported file");
    assert!(exported_content.contains("Darius is written in Rust."));
    assert!(exported_content.contains("All mutations require approval."));

    ctx.assert_clean_home("test_memory_cli_lifecycle");
}

#[test]
fn test_run_memory_tools_roundtrip() {
    let ctx = TestContext::new();
    let provider = FakeProvider::start();
    let key_env = "DARIUS_RUN_KEY";
    let key_val = "run-secret-123";

    ctx.write_profile_config("default", provider.url(), key_env);

    // Turn 1: memory_remember (mutating, denied in headless)
    provider.push_tool_call(
        "call-remember",
        "memory_remember",
        serde_json::json!({"body": "AGENT_MEM_UNIQUE_RS", "kind": "fact", "title": "Memory Test"}),
    );
    provider.push_text("remember attempted");

    // Turn 2: memory_search (read-only, should succeed)
    provider.push_tool_call(
        "call-search",
        "memory_search",
        serde_json::json!({"text": "AGENT_MEM_UNIQUE_RS"}),
    );
    provider.push_text("search complete");

    let mut cmd = ctx.command();
    cmd.env(key_env, key_val)
        .arg("run")
        .arg("remember and then search for AGENT_MEM_UNIQUE_RS");

    let assert = cmd.assert().failure().code(1);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("Mutation denied") || stderr.contains("denied"),
        "expected mutation denial, got: {stderr}"
    );

    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 2, "remember denied, no further turns");

    // Verify memory_remember was denied (mutating in headless)
    let messages = requests[1].body["messages"].as_array().unwrap();
    assert!(
        messages.iter().any(|message| {
            message["role"] == "tool"
                && message["tool_call_id"] == "call-remember"
                && message["content"]
                    .as_str()
                    .is_some_and(|text| text.contains("denied") || text.contains("approval"))
        }),
        "memory_remember should be denied in headless: {messages:?}"
    );

    // Verify memory.db was NOT written to (remember was denied)
    let db_path = ctx.home.path().join("profiles/default/memory.db");
    if db_path.exists() {
        let content = std::fs::read_to_string(&db_path).unwrap_or_default();
        assert!(!content.contains("AGENT_MEM_UNIQUE_RS"), "memory_remember was denied, unique body should not be in db");
    }

    ctx.assert_clean_home("test_run_memory_tools_roundtrip");
}

fn assert_provider_failure(response: Option<(u16, String)>, category: &[&str], action: &[&str]) {
    let ctx = TestContext::new();
    let provider = http_fixture::HttpFixture::start(response);
    ctx.write_profile_config("default", &provider.url, KEY_ENV);
    let start = std::time::Instant::now();
    let output = ctx
        .command()
        .env(KEY_ENV, "fixture-secret-never-display")
        .args(["run", "inspect"])
        .timeout(Duration::from_secs(70))
        .assert()
        .get_output()
        .clone();
    assert_eq!(
        output.status.code(),
        Some(1),
        "must exit itself with code 1"
    );
    assert!(start.elapsed() < Duration::from_secs(70));
    assert!(provider.requests.load(std::sync::atomic::Ordering::SeqCst) > 0);
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    for marker in [
        "fixture-secret-never-display",
        "private-provider-body",
        "\u{1b}",
    ] {
        assert!(
            !stderr.contains(marker) && !stdout.contains(marker),
            "provider data leaked: {marker}"
        );
    }
    assert!(
        category.iter().any(|word| stderr.contains(word)),
        "missing error category: {stderr}"
    );
    assert!(
        action.iter().any(|word| stderr.contains(word)),
        "missing recovery action: {stderr}"
    );
    ctx.assert_clean_home("binary provider failure");
}

fn hostile_body() -> String {
    serde_json::json!({"error":{"message":"private-provider-body fixture-secret-never-display \u{1b}[31m"}}).to_string()
}

#[test]
fn binary_401_is_sanitized_and_actionable() {
    assert_provider_failure(
        Some((401, hostile_body())),
        &["auth", "401"],
        &["check", "configure", "set ", "config init"],
    );
}
#[test]
fn binary_429_is_sanitized_and_actionable() {
    assert_provider_failure(
        Some((429, hostile_body())),
        &["rate", "429"],
        &["retry", "wait", "try again"],
    );
}
#[test]
fn binary_500_is_sanitized_and_actionable() {
    assert_provider_failure(
        Some((500, hostile_body())),
        &["unavailable", "server", "500"],
        &["retry", "try again"],
    );
}
#[test]
fn binary_503_is_sanitized_and_actionable() {
    assert_provider_failure(
        Some((503, hostile_body())),
        &["unavailable", "server", "503"],
        &["retry", "try again"],
    );
}
#[test]
fn binary_invalid_response_is_actionable() {
    assert_provider_failure(
        Some((
            200,
            "invalid-json private-provider-body fixture-secret-never-display".into(),
        )),
        &["invalid", "response"],
        &["check", "retry", "compatible"],
    );
}
#[test]
fn binary_timeout_is_actionable_and_exits_itself() {
    // Exercise the real binary's 60-second turn deadline, not adapter injection or watchdog kill.
    assert_provider_failure(
        None,
        &["timeout", "timed out", "deadline"],
        &["retry", "check", "try again"],
    );
}

#[test]
fn binary_shell_denied_without_execution_or_prompt() {
    let ctx = TestContext::new();
    let provider = FakeProvider::start();
    ctx.write_profile_config("default", provider.url(), KEY_ENV);
    provider.push_tool_call(
        "shell-denied",
        "shell",
        serde_json::json!({"command":"touch shell-ran"}),
    );
    provider.push_text("denied safely");
    let output = ctx
        .command()
        .env(KEY_ENV, "fixture-key")
        .args(["run", "execute shell"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(!ctx.workspace.path().join("shell-ran").exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("denied") && stderr.contains("darius tui"));
    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1].body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "tool"
                && m["tool_call_id"] == "shell-denied"
                && m["content"].as_str().is_some_and(|s| s.contains("denied")))
    );
    ctx.assert_clean_home("binary shell denial");
}

#[test]
fn binary_memory_roundtrip_and_profile_isolation() {
    let ctx = TestContext::new();
    let input = ctx.workspace.path().join("input.jsonl");
    let first = ctx.workspace.path().join("first.jsonl");
    let second = ctx.workspace.path().join("second.jsonl");
    let record = serde_json::json!({"kind":"Fact","title":"Roundtrip","body":"unique persisted fact","tags":["proof"],"importance":0.7,"source":null});
    std::fs::write(&input, format!("{record}\n")).unwrap();
    for profile in ["alpha", "beta", "default"] {
        std::fs::create_dir_all(ctx.home.path().join("profiles").join(profile)).unwrap();
    }
    for (args, expected) in [
        (
            vec![
                "--profile",
                "alpha",
                "memory",
                "import",
                input.to_str().unwrap(),
            ],
            "Imported: 1, Skipped: 0",
        ),
        (
            vec![
                "memory",
                "export",
                first.to_str().unwrap(),
                "--profile",
                "alpha",
            ],
            "Exported 1 records",
        ),
        (vec!["memory", "stats", "--profile", "beta"], "Records: 0"),
        (
            vec!["memory", "search", "unique", "--profile", "beta"],
            "Found 0 results",
        ),
        (
            vec![
                "memory",
                "import",
                first.to_str().unwrap(),
                "--profile",
                "beta",
            ],
            "Imported: 1, Skipped: 0",
        ),
        (
            vec![
                "--profile",
                "beta",
                "memory",
                "export",
                second.to_str().unwrap(),
            ],
            "Exported 1 records",
        ),
        (
            vec!["memory", "search", "unique", "--profile", "beta"],
            "unique persisted fact",
        ),
        (
            vec!["memory", "pack", "--profile", "beta"],
            "unique persisted fact",
        ),
        (vec!["memory", "stats", "--profile", "alpha"], "Records: 1"),
        (vec!["memory", "stats"], "Records: 0"),
    ] {
        let output = ctx
            .command()
            .args(&args)
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(expected),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    let a: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(first).unwrap().trim()).unwrap();
    let b: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(second).unwrap().trim()).unwrap();
    for field in ["kind", "title", "body", "tags", "importance", "source"] {
        assert_eq!(a[field], record[field], "initial export {field}");
        assert_eq!(b[field], a[field], "roundtrip {field}");
    }
    ctx.assert_clean_home("binary memory profile roundtrip");
}

#[test]
fn binary_globals_apply_before_and_after_nested_operations() {
    let ctx = TestContext::new();
    let provider = FakeProvider::start();
    let selected = TempDir::new().unwrap();
    ctx.write_profile_config("chosen", provider.url(), KEY_ENV);
    for after in [false, true] {
        let globals = [
            "--profile",
            "chosen",
            "--cwd",
            selected.path().to_str().unwrap(),
        ];
        let mut args = vec!["config", "show"];
        if after {
            args.extend(globals);
        } else {
            args.splice(0..0, globals);
        }
        let output = ctx
            .command()
            .env(KEY_ENV, "fixture-key")
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(ctx.home.path().join("profiles/chosen").to_str().unwrap()));
        assert!(
            stdout.contains(&format!(
                "Workspace: {}",
                selected.path().canonicalize().unwrap().display()
            )),
            "{stdout}"
        );
        let mut args = vec!["run", "inspect"];
        if after {
            args.extend(globals);
            args.push("--offline");
        } else {
            args.splice(0..0, globals);
            args.insert(0, "--offline");
        }
        let output = ctx
            .command()
            .env(KEY_ENV, "fixture-key")
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(String::from_utf8_lossy(&output.stdout).contains("Runtime state: offline-demo"));
    }
    assert!(
        provider.recorded_requests().is_empty(),
        "offline must not contact configured provider"
    );
    for args in [
        vec![],
        vec!["--help"],
        vec!["-h"],
        vec!["run", "--help"],
        vec!["config", "--help"],
        vec!["memory", "--help"],
        vec!["tui", "--help"],
    ] {
        let output = ctx
            .command()
            .args(args)
            .assert()
            .success()
            .get_output()
            .clone();
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
    }
    for flag in ["--version", "-V"] {
        let output = ctx
            .command()
            .arg(flag)
            .assert()
            .success()
            .get_output()
            .clone();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("darius {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
    ctx.assert_clean_home("binary globals");
}

#[test]
fn binary_config_presets_and_force_preserve_other_profiles() {
    let ctx = TestContext::new();
    for preset in ["openai", "openrouter", "ollama", "groq"] {
        ctx.command()
            .args(["--profile", preset, "config", "preset", preset])
            .assert()
            .success();
        let path = ctx
            .home
            .path()
            .join("profiles")
            .join(preset)
            .join("config.toml");
        let original = std::fs::read(&path).unwrap();
        ctx.command()
            .args(["config", "preset", "openai", "--profile", preset])
            .assert()
            .code(1);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        ctx.command()
            .args(["config", "preset", "openai", "--profile", preset, "--force"])
            .assert()
            .success();
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains("api.openai.com")
        );
    }
    assert!(
        !ctx.home
            .path()
            .join("profiles/default/config.toml")
            .exists()
    );
    ctx.command()
        .args([
            "config",
            "init",
            "--profile",
            "custom",
            "--provider",
            "fixture",
            "--base-url",
            "http://127.0.0.1:1/v1",
            "--model",
            "original",
            "--key-env",
            KEY_ENV,
        ])
        .assert()
        .success();
    let path = ctx.home.path().join("profiles/custom/config.toml");
    let original = std::fs::read(&path).unwrap();
    let args = [
        "config",
        "init",
        "--profile",
        "custom",
        "--provider",
        "fixture",
        "--base-url",
        "http://127.0.0.1:1/v1",
        "--model",
        "replacement",
        "--key-env",
        KEY_ENV,
    ];
    ctx.command().args(args).assert().code(1);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    ctx.command().args(args).arg("--force").assert().success();
    assert!(
        std::fs::read_to_string(path)
            .unwrap()
            .contains("replacement")
    );
    ctx.assert_clean_home("binary config presets and force");
}
