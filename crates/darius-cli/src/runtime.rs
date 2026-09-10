//! One reusable session runtime shared by CLI, TUI, web, and A2A.

use std::path::PathBuf;
use std::sync::Arc;

use darius_cognitive::{AsyncModel, Conversation, LoopPolicy, RunMetadata, UiEvent};
use darius_memory::MemoryEngine;
use darius_tools::ToolRegistry;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::config::ProfileConfig;
use crate::config_error::ConfigError;
use crate::diagnostics;
use crate::paths::{DariusPaths, PathError};
use crate::runtime_selection::RuntimeState;
use crate::runtime_selector::{nonblank, select};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("path error: {0}")]
    Path(#[from] PathError),
    #[error("missing API key: set {0}")]
    MissingApiKey(String),
    #[error("tool error: {0}")]
    Tool(#[from] darius_tools::ToolError),
    #[error("model error: {0}")]
    Model(String),
    #[error("memory error: {0}")]
    Memory(#[from] darius_memory::MemoryError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub profile: String,
    pub profile_dir: PathBuf,
}

impl RuntimeConfig {
    pub fn from_profile(paths: &DariusPaths, profile: &str) -> Result<Self, PathError> {
        Ok(Self {
            profile: profile.into(),
            profile_dir: paths.profile(profile)?,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeOptions {
    pub offline: bool,
}

struct ResolvedProfile {
    config: RuntimeConfig,
    profile_config: ProfileConfig,
    config_path: PathBuf,
    config_exists: bool,
    state: RuntimeState,
}

fn resolve_profile(
    paths: &DariusPaths,
    profile: &str,
    options: RuntimeOptions,
) -> Result<ResolvedProfile, RuntimeError> {
    crate::runtime_selector::load_dotenv_if_present(Some(&paths.workspace));
    let config = RuntimeConfig::from_profile(paths, profile)?;
    let config_path = ProfileConfig::config_path(paths, profile)?;
    let config_exists = config_path.try_exists()?;
    let mut profile_config = if config_exists {
        ProfileConfig::load(paths, profile)?
    } else {
        ProfileConfig::default()
    };
    let state = if options.offline {
        RuntimeState::OfflineDemo
    } else {
        select(config_exists.then_some(&profile_config), nonblank)
    };
    if !config_exists && let RuntimeState::Live(provider) = &state {
        profile_config.model = Some(provider.config());
    }
    Ok(ResolvedProfile {
        config,
        profile_config,
        config_path,
        config_exists,
        state,
    })
}

pub use crate::permissions::SessionPermissions;

/// Session state; model tools must run through the policy-aware agent turn.
/// There is deliberately no direct model-call dispatch API on the session;
/// see `AgentLoop::run_turn` for the policy-checked path.
pub struct SessionRuntime {
    pub mode: darius_core::runtime_protocol::Mode,
    pub permissions: SessionPermissions,
    pub config: RuntimeConfig,
    pub profile_config: ProfileConfig,
    pub memory: MemoryEngine,
    pub tools: ToolRegistry,
    pub task_board: Arc<parking_lot::Mutex<darius_tools::TaskBoard>>,
    pub model: Box<dyn AsyncModel>,
    pub conversation: Conversation,
    pub workspace: PathBuf,
    pub metadata: RunMetadata,
    pub event_sender: broadcast::Sender<UiEvent>,
    pub cancellation: CancellationToken,
    pub policy: LoopPolicy,
    state: RuntimeState,
    diagnostics: Vec<String>,
    pub mcp_clients: Vec<Arc<darius_tools::StdioMcpClient>>,
    pub dynamic_tool_specs: Vec<darius_cognitive::ToolSpec>,
}
impl SessionRuntime {
    pub fn from_profile(paths: &DariusPaths, profile: &str) -> Result<Self, RuntimeError> {
        Self::from_options(paths, profile, RuntimeOptions::default())
    }

    pub fn from_options(
        paths: &DariusPaths,
        profile: &str,
        options: RuntimeOptions,
    ) -> Result<Self, RuntimeError> {
        let resolved = resolve_profile(paths, profile, options)?;
        if let RuntimeState::MissingKey(provider) = &resolved.state {
            return Err(RuntimeError::MissingApiKey(provider.key_env.clone()));
        }
        std::fs::create_dir_all(&resolved.config.profile_dir)?;
        let memory = MemoryEngine::open(&resolved.config.profile_dir)?;
        let spill_dir = resolved.config.profile_dir.join("tool_results");
        let mut tools = ToolRegistry::new_with_roots(&paths.workspace, &spill_dir)?;
        darius_tools::register_memory_builtins(&mut tools, &memory);
        let board = Arc::new(parking_lot::Mutex::new(darius_tools::TaskBoard::new(15)));
        darius_tools::register_task_builtins(&mut tools, board.clone());
        darius_tools::register_coding_builtins(&mut tools);
        darius_tools::register_spill_read(&mut tools);
        let model = model_for(&resolved.state).map_err(RuntimeError::Model)?;
        let metadata = RunMetadata {
            profile: profile.into(),
            model: model_label(&resolved.state),
            mode: resolved.state.label().into(),
        };
        let mut diagnostics = diagnostics::lines(
            paths,
            profile,
            &resolved.config_path,
            resolved.config_exists,
            &resolved.state,
            true,
        );
        let mut mcp_clients = Vec::new();
        let mut dynamic_tool_specs = Vec::new();
        for server in resolved.profile_config.mcp_servers() {
            let timeout = std::time::Duration::from_millis(server.timeout_ms.unwrap_or(30_000));
            match darius_tools::StdioMcpClient::connect(&server.transport, timeout) {
                Ok(client) => {
                    let client = Arc::new(client.with_spill_dir(spill_dir.clone()));
                    match darius_tools::register_mcp_tools_defs(
                        &mut tools,
                        &server.name,
                        client.clone(),
                    ) {
                        Ok(defs) => {
                            for (registered_name, def) in &defs {
                                dynamic_tool_specs.push(darius_cognitive::ToolSpec {
                                    name: registered_name.clone(),
                                    description: def.description.clone(),
                                    parameters: def.input_schema.clone(),
                                });
                            }
                            diagnostics.push(format!(
                                "mcp: {}={} tools={}",
                                server.name,
                                "ok",
                                defs.len()
                            ));
                            mcp_clients.push(client);
                        }
                        Err(e) => {
                            let _ = client.shutdown();
                            diagnostics.push(format!("mcp: {}={} ({})", server.name, "error", e));
                        }
                    }
                }
                Err(e) => {
                    diagnostics.push(format!("mcp: {}={} ({})", server.name, "error", e));
                }
            }
        }
        let (event_sender, _) = broadcast::channel(256);
        let conversation =
            Conversation::from_messages(vec![]).map_err(|e| RuntimeError::Model(e.to_string()))?;
        Ok(Self {
            mode: darius_core::runtime_protocol::Mode::Auto,
            permissions: Arc::default(),
            config: resolved.config,
            profile_config: resolved.profile_config,
            memory,
            tools,
            task_board: board,
            model,
            conversation,
            workspace: paths.workspace.clone(),
            metadata,
            event_sender,
            cancellation: CancellationToken::new(),
            policy: LoopPolicy::default(),
            state: resolved.state,
            diagnostics,
            mcp_clients,
            dynamic_tool_specs,
        })
    }

    pub fn diagnostics_for(
        paths: &DariusPaths,
        profile: &str,
        options: RuntimeOptions,
    ) -> Result<Vec<String>, RuntimeError> {
        let resolved = resolve_profile(paths, profile, options)?;
        std::fs::create_dir_all(&resolved.config.profile_dir)?;
        MemoryEngine::open(&resolved.config.profile_dir)?;
        let mut lines = diagnostics::lines(
            paths,
            profile,
            &resolved.config_path,
            resolved.config_exists,
            &resolved.state,
            true,
        );
        for server in resolved.profile_config.mcp_servers() {
            let timeout =
                std::time::Duration::from_millis(server.timeout_ms.unwrap_or(2_000).min(5_000));
            match darius_tools::StdioMcpClient::connect(&server.transport, timeout) {
                Ok(client) => {
                    let count = client.list_tools().map(|t| t.len()).unwrap_or(0);
                    let _ = client.shutdown();
                    lines.push(format!("mcp: {}={} tools={}", server.name, "ok", count));
                }
                Err(e) => {
                    lines.push(format!("mcp: {}={} ({})", server.name, "error", e));
                }
            }
        }
        Ok(lines)
    }

    pub fn is_setup(&self) -> bool {
        matches!(self.state, RuntimeState::Setup)
    }

    /// Model-facing tool execution must go through `AgentLoop::run_turn`,
    /// which checks mode and permission before dispatch.
    pub fn is_offline_demo(&self) -> bool {
        matches!(self.state, RuntimeState::OfflineDemo)
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
    pub fn subscribe_events(&self) -> broadcast::Receiver<UiEvent> {
        self.event_sender.subscribe()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn compact_conversation(&mut self) -> Result<(), darius_cognitive::CognitiveError> {
        self.conversation
            .compact(self.policy.compress_opts.max_chars)?;
        if !self.conversation.is_empty() {
            let summary = self
                .conversation
                .messages()
                .iter()
                .filter_map(|m| match m {
                    darius_cognitive::Message::User { content } => Some(format!("User: {content}")),
                    darius_cognitive::Message::Assistant {
                        content: Some(content),
                        ..
                    } => Some(format!("Assistant: {content}")),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !summary.is_empty() {
                let _ = self.memory.upsert(darius_memory::NewRecord {
                    kind: darius_memory::RecordKind::Episode,
                    title: Some("compacted conversation session".into()),
                    body: summary.chars().take(4000).collect(),
                    tags: vec!["compaction".into()],
                    importance: 0.8,
                    source: Some("compact_conversation".into()),
                });
            }
        }
        Ok(())
    }

    pub fn model_id(&self) -> &str {
        &self.metadata.model
    }

    pub fn apply_model_config(
        &mut self,
        cfg: &darius_core::config::ModelConfig,
    ) -> Result<Option<String>, String> {
        let _ = darius_core::config::save_model_config(&self.config.profile_dir, cfg);
        self.metadata.model = cfg.model.clone();

        if cfg.provider == "mock" || cfg.provider == "offline" {
            if !self.is_offline_demo() {
                return Err("mock provider is only allowed with explicit --offline flag".into());
            }
            self.model = Box::new(darius_cognitive::MockModel::new(vec![]));
            return Ok(None);
        }

        let is_local = cfg.base_url.contains("localhost")
            || cfg.base_url.contains("127.0.0.1")
            || cfg.api_key_env.eq_ignore_ascii_case("none");
        let has_key = std::env::var(&cfg.api_key_env).is_ok();

        if !is_local && !has_key {
            return Err(format!(
                "API key environment variable '{}' is not set; export it or run 'darius config init'",
                cfg.api_key_env
            ));
        }

        let model = darius_daemon::LiveModel::for_provider(darius_daemon::Provider {
            name: cfg.provider.clone(),
            model: cfg.model.clone(),
            base_url: cfg.base_url.clone(),
            enabled: true,
            api_key_env: cfg.api_key_env.clone(),
        })
        .map_err(|e| e.to_string())?;

        self.model = Box::new(model);
        Ok(None)
    }
}

impl Drop for SessionRuntime {
    fn drop(&mut self) {
        for client in &self.mcp_clients {
            let _ = client.shutdown();
        }
    }
}

fn model_for(state: &RuntimeState) -> Result<Box<dyn AsyncModel>, String> {
    match state {
        RuntimeState::Live(provider) => {
            darius_daemon::LiveModel::for_provider(darius_daemon::Provider {
                name: "default".into(),
                model: provider.model.clone(),
                base_url: provider.base_url.clone(),
                enabled: true,
                api_key_env: provider.key_env.clone(),
            })
            .map(|m| Box::new(m) as Box<dyn AsyncModel>)
            .map_err(|e| e.to_string())
        }
        RuntimeState::OfflineDemo | RuntimeState::Setup => {
            Ok(Box::new(darius_cognitive::MockModel::new(vec![])))
        }
        RuntimeState::MissingKey(_) => unreachable!("missing keys do not build runtimes"),
    }
}

/// Drive one async agent turn from sync callers on a fresh thread, so this
/// works with or without an enclosing async runtime.
pub(crate) fn block_on_turn<F, T>(future: F) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("local runtime")
        .block_on(future)
}

fn model_label(state: &RuntimeState) -> String {
    match state {
        RuntimeState::Live(provider) => format!("{}/{}", provider.provider, provider.model),
        RuntimeState::OfflineDemo => "offline-demo".into(),
        RuntimeState::Setup => "not-configured".into(),
        RuntimeState::MissingKey(_) => "not-configured".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths(temp: &TempDir) -> DariusPaths {
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        DariusPaths { home, workspace }
    }

    #[test]
    fn from_profile_enters_setup_when_config_is_missing() {
        let temp = TempDir::new().unwrap();
        let runtime = SessionRuntime::from_profile(&paths(&temp), "offline").unwrap();
        assert_eq!(runtime.metadata.model, "not-configured");
        assert!(runtime.is_setup());
    }

    #[test]
    fn from_profile_reports_missing_api_key() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let profile = paths.profile("missingkey").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("config.toml"), "[model]\nprovider = \"provider\"\nbase_url = \"https://api.example.test\"\nmodel = \"test\"\napi_key_env = \"DARIUS_TEST_MISSING_KEY_NEVER_SET\"\n").unwrap();
        assert!(matches!(
            SessionRuntime::from_profile(&paths, "missingkey"),
            Err(RuntimeError::MissingApiKey(_))
        ));
    }

    #[test]
    fn runtime_exposes_event_broadcaster_and_cancellation() {
        let temp = TempDir::new().unwrap();
        let runtime = SessionRuntime::from_profile(&paths(&temp), "metadata").unwrap();
        let _receiver = runtime.subscribe_events();
        assert!(!runtime.cancellation_token().is_cancelled());
    }

    #[test]
    fn runtime_retains_task_board() {
        let temp = TempDir::new().unwrap();
        let runtime = SessionRuntime::from_profile(&paths(&temp), "board").unwrap();
        runtime.task_board.lock().add("retained task").unwrap();
        let call = darius_tools::ToolCall {
            id: "board-1".into(),
            name: "task_list".into(),
            arguments: serde_json::Value::Null,
        };
        match runtime.tools.execute(&call) {
            darius_tools::ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("retained task"));
            }
            darius_tools::ToolOutcome::Err { message } => panic!("task_list failed: {message}"),
            darius_tools::ToolOutcome::Interrupted | darius_tools::ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
    }

    #[test]
    fn apply_mock_rejected_unless_offline_demo() {
        let temp = TempDir::new().unwrap();
        let mut runtime = SessionRuntime::from_profile(&paths(&temp), "model_test").unwrap();
        let cfg = darius_core::config::ModelConfig {
            provider: "mock".into(),
            base_url: "http://localhost:8080/v1".into(),
            model: "mock".into(),
            api_key_env: "NONE".into(),
        };
        let err = runtime.apply_model_config(&cfg).unwrap_err();
        assert!(err.contains("mock provider is only allowed with explicit --offline flag"));
    }

    #[test]
    fn apply_mock_allowed_with_offline_demo() {
        let temp = TempDir::new().unwrap();
        let mut runtime = SessionRuntime::from_options(
            &paths(&temp),
            "model_test_offline",
            RuntimeOptions { offline: true },
        )
        .unwrap();
        let cfg = darius_core::config::ModelConfig {
            provider: "mock".into(),
            base_url: "http://localhost:8080/v1".into(),
            model: "mock".into(),
            api_key_env: "NONE".into(),
        };
        let res = runtime.apply_model_config(&cfg).unwrap();
        assert_eq!(res, None);
        assert_eq!(runtime.model_id(), "mock");
    }

    #[test]
    fn apply_model_config_rejects_missing_key_without_mock_fallback() {
        let temp = TempDir::new().unwrap();
        let mut runtime = SessionRuntime::from_profile(&paths(&temp), "model_test2").unwrap();
        let cfg = darius_core::config::ModelConfig {
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            api_key_env: "DARIUS_UNSET_KEY_FOR_TEST_12345".into(),
        };
        let err = runtime.apply_model_config(&cfg).unwrap_err();
        assert!(err.contains("DARIUS_UNSET_KEY_FOR_TEST_12345"));
        assert!(err.contains("is not set"));
    }

    #[test]
    fn apply_openai_entry_with_key_sets_live_model() {
        let temp = TempDir::new().unwrap();
        let mut runtime = SessionRuntime::from_profile(&paths(&temp), "model_test3").unwrap();
        unsafe { std::env::set_var("DARIUS_TEST_LIVE_KEY_ENV", "test-live-key") };
        let cfg = darius_core::config::ModelConfig {
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            api_key_env: "DARIUS_TEST_LIVE_KEY_ENV".into(),
        };
        let res = runtime.apply_model_config(&cfg).unwrap();
        assert_eq!(res, None);
        assert_eq!(runtime.model_id(), "gpt-4o-mini");
        unsafe { std::env::remove_var("DARIUS_TEST_LIVE_KEY_ENV") };
    }

    #[test]
    #[ignore]
    fn live_key_smoke() {
        if let Ok(key) = std::env::var("OPENAI_API_KEY")
            && !key.is_empty()
        {
            let temp = TempDir::new().unwrap();
            let mut runtime = SessionRuntime::from_profile(&paths(&temp), "live_smoke").unwrap();
            let cfg = darius_core::config::ModelConfig {
                provider: "openai".into(),
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4o-mini".into(),
                api_key_env: "OPENAI_API_KEY".into(),
            };
            let res = runtime.apply_model_config(&cfg).unwrap();
            assert_eq!(res, None);
            assert_eq!(runtime.model_id(), "gpt-4o-mini");
        }
    }

    #[test]
    fn runtime_starts_and_registers_mcp_servers_from_profile() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let profile = paths.profile("mcp_prof").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../darius-tools/tests/fixtures/mock_mcp.py")
            .canonicalize()
            .unwrap();

        let toml_str = format!(
            r#"[model]
provider = "openai_compatible"
base_url = "http://127.0.0.1:8080"
model = "mock"
api_key_env = "NONE"

[[mcp.servers]]
name = "mock"
type = "stdio"
command = "python3"
args = ["{}"]
"#,
            script.to_string_lossy()
        );
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();

        let runtime = SessionRuntime::from_profile(&paths, "mcp_prof").unwrap();
        assert!(runtime.tools.has_tool("mcp_mock_echo"));
        assert_eq!(runtime.mcp_clients.len(), 1);
        assert!(
            runtime
                .diagnostics()
                .iter()
                .any(|d| d.contains("mcp: mock=ok tools=1"))
        );
    }

    #[test]
    fn runtime_soft_fails_on_bad_mcp_command_and_records_diagnostic() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let profile = paths.profile("mcp_bad").unwrap();
        std::fs::create_dir_all(&profile).unwrap();

        let toml_str = r#"[model]
provider = "openai_compatible"
base_url = "http://127.0.0.1:8080"
model = "mock"
api_key_env = "NONE"

[[mcp.servers]]
name = "bad_mock"
type = "stdio"
command = "/path/to/nonexistent/executable/for/test"
"#;
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();

        let runtime = SessionRuntime::from_profile(&paths, "mcp_bad").unwrap();
        assert_eq!(runtime.mcp_clients.len(), 0);
        assert!(
            runtime
                .diagnostics()
                .iter()
                .any(|d| d.contains("mcp: bad_mock=error"))
        );
        let diag =
            SessionRuntime::diagnostics_for(&paths, "mcp_bad", RuntimeOptions::default()).unwrap();
        assert!(diag.iter().any(|d| d.contains("mcp: bad_mock=error")));
    }

    #[test]
    fn dynamic_mcp_tools_included_in_turn_schemas() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let profile = paths.profile("mcp_turn_prof").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../darius-tools/tests/fixtures/mock_mcp.py")
            .canonicalize()
            .unwrap();

        let toml_str = format!(
            r#"[model]
provider = "openai_compatible"
base_url = "http://127.0.0.1:8080"
model = "mock"
api_key_env = "NONE"

[[mcp.servers]]
name = "mock"
type = "stdio"
command = "python3"
args = ["{}"]
"#,
            script.to_string_lossy()
        );
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();

        let mut runtime = SessionRuntime::from_profile(&paths, "mcp_turn_prof").unwrap();
        assert!(
            runtime
                .dynamic_tool_specs
                .iter()
                .any(|t| t.name == "mcp_mock_echo")
        );

        struct SpyModel {
            recorded_tools: std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
        }

        #[darius_cognitive::async_trait]
        impl darius_cognitive::AsyncModel for SpyModel {
            async fn complete(
                &mut self,
                _messages: &[darius_cognitive::Message],
                tools: &[darius_cognitive::ToolSpec],
                _ctx: &darius_cognitive::TurnContext,
            ) -> Result<darius_cognitive::ModelOutput, darius_cognitive::CognitiveError>
            {
                let mut list = self.recorded_tools.lock();
                for t in tools {
                    list.push(t.name.clone());
                }
                Ok(darius_cognitive::ModelOutput {
                    content: Some("done".into()),
                    tool_calls: vec![],
                })
            }
        }

        let recorded = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let mut spy = SpyModel {
            recorded_tools: recorded.clone(),
        };

        let (tx, _rx) = std::sync::mpsc::channel();
        let sink = std::sync::Arc::new(darius_cognitive::ChannelEventSink::new(tx));
        let control = std::sync::Arc::new(crate::permissions::HeadlessRunControl::default());
        let loopt = darius_cognitive::AgentLoop::new(sink, control);

        let res = block_on_turn(loopt.run_turn_with_extra_tools(
            &runtime.metadata,
            &runtime.policy,
            "test echo",
            &mut runtime.conversation,
            &mut spy,
            &runtime.tools,
            &runtime.memory,
            &runtime.workspace.to_string_lossy(),
            &runtime.dynamic_tool_specs,
        ));
        assert!(res.is_ok());

        let seen_tools = recorded.lock();
        assert!(
            seen_tools.contains(&"mcp_mock_echo".to_string()),
            "tools was {:?}",
            *seen_tools
        );
    }
}
