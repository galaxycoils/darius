mod support;

use assert_cmd::Command as AssertCommand;
use std::path::PathBuf;
use std::time::Duration;
use support::DariusHomeSnapshot;
use support::fake_provider::FakeProvider;
use tempfile::TempDir;

const KEY_ENV: &str = "DARIUS_RUN_KEY";

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
            .env("NO_PROXY", "127.0.0.1,localhost")
            .timeout(Duration::from_secs(15))
            .current_dir(self.workspace.path());
        cmd
    }

    fn write_profile_config_with_mcp(
        &self,
        profile: &str,
        provider_url: &str,
        key_env: &str,
        mcp_command: &str,
        mcp_args: &[&str],
        mcp_env: &[(&str, &str)],
    ) {
        let dir = self.home.path().join("profiles").join(profile);
        std::fs::create_dir_all(&dir).unwrap();

        let args_json = serde_json::to_string(mcp_args).unwrap();
        let mut env_toml = String::new();
        for (k, v) in mcp_env {
            if !env_toml.is_empty() {
                env_toml.push_str(", ");
            }
            env_toml.push_str(&format!("\"{k}\" = \"{v}\""));
        }

        let toml = format!(
            r#"[model]
provider = "custom-provider"
base_url = "{provider_url}/v1"
model = "custom-model"
api_key_env = "{key_env}"

[[mcp.servers]]
name = "mock"
type = "stdio"
command = "{mcp_command}"
args = {args_json}
env = {{ {env_toml} }}
"#
        );
        std::fs::write(dir.join("config.toml"), toml).unwrap();
    }

    fn assert_clean_home(&self, label: &str) {
        self.snapshot.assert_unchanged(label);
    }
}

#[test]
fn test_run_mcp_echo_tool_succeeds() {
    let ctx = TestContext::new();
    let provider = FakeProvider::start_strict("mcp-secret");

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../darius-tools/tests/fixtures/mock_mcp.py")
        .canonicalize()
        .expect("mock_mcp.py script must exist");

    ctx.write_profile_config_with_mcp(
        "default",
        provider.url(),
        KEY_ENV,
        "python3",
        &[&script.to_string_lossy()],
        &[("MOCK_MCP_READONLY", "1")],
    );

    provider.push_tool_call(
        "mcp-call-1",
        "mcp_mock_echo",
        serde_json::json!({"text": "MCP_E2E_MARKER"}),
    );
    provider.push_text("echoed successfully: MCP_E2E_MARKER");
    ctx.command()
        .env(KEY_ENV, "mcp-secret")
        .args(["run", "call mcp echo"])
        .assert()
        .success();
    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 2);
    let tools: Vec<_> = requests[1].body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .collect();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["tool_call_id"], "mcp-call-1");
    let content = tools[0]["content"].as_str().unwrap();
    assert!(content.contains("MCP_E2E_MARKER"), "content was: {content}");

    provider.assert_clean();
    ctx.assert_clean_home("test_run_mcp_echo_tool_succeeds");
}

#[test]
fn test_config_show_includes_mcp_health_without_leaking_secrets() {
    let ctx = TestContext::new();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../darius-tools/tests/fixtures/mock_mcp.py")
        .canonicalize()
        .expect("mock_mcp.py script must exist");

    ctx.write_profile_config_with_mcp(
        "default",
        "http://127.0.0.1:8080",
        KEY_ENV,
        "python3",
        &[&script.to_string_lossy()],
        &[("SECRET_MCP_KEY", "super-secret-token-12345")],
    );

    let assert = ctx.command().args(["config", "show"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("mcp: mock=ok tools=1"),
        "stdout was: {stdout}"
    );
    assert!(
        !stdout.contains("super-secret-token-12345"),
        "secret leaked in config show output"
    );
}
