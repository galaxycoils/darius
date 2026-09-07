mod support;

use assert_cmd::Command as AssertCommand;
use support::DariusHomeSnapshot;
use support::fake_provider::FakeProvider;
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
            .env_remove("DARIUS_PROFILE")
            .current_dir(self.workspace.path())
            .arg("--cwd")
            .arg(self.workspace.path());
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
