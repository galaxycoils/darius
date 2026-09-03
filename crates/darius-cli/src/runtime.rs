//! One reusable session runtime shared by CLI, TUI, web, and A2A.

use std::path::PathBuf;
use std::sync::Arc;

use darius_cognitive::{LoopPolicy, Model, RunMetadata, UiEvent};
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

pub struct SessionRuntime {
    pub config: RuntimeConfig,
    pub profile_config: ProfileConfig,
    pub memory: MemoryEngine,
    pub tools: ToolRegistry,
    pub task_board: Arc<parking_lot::Mutex<darius_tools::TaskBoard>>,
    pub model: Box<dyn Model>,
    pub metadata: RunMetadata,
    pub event_sender: broadcast::Sender<UiEvent>,
    pub cancellation: CancellationToken,
    pub policy: LoopPolicy,
    state: RuntimeState,
    diagnostics: Vec<String>,
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
        let model = model_for(&resolved.state);
        let metadata = RunMetadata {
            profile: profile.into(),
            model: model_label(&resolved.state),
            mode: resolved.state.label().into(),
        };
        let diagnostics = diagnostics::lines(
            paths,
            profile,
            &resolved.config_path,
            resolved.config_exists,
            &resolved.state,
            true,
        );
        let (event_sender, _) = broadcast::channel(256);
        Ok(Self {
            config: resolved.config,
            profile_config: resolved.profile_config,
            memory,
            tools,
            task_board: board,
            model,
            metadata,
            event_sender,
            cancellation: CancellationToken::new(),
            policy: LoopPolicy::default(),
            state: resolved.state,
            diagnostics,
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
        Ok(diagnostics::lines(
            paths,
            profile,
            &resolved.config_path,
            resolved.config_exists,
            &resolved.state,
            true,
        ))
    }

    pub fn is_setup(&self) -> bool {
        matches!(self.state, RuntimeState::Setup)
    }

    /// Model-facing dispatch through the verified allowlist. Unknown and
    /// hidden calls are rejected before permission with one correlated error.
    /// The cognitive loop migrates to this path in Task 3.3.
    pub fn execute_model_call(&self, call: &darius_tools::ToolCall) -> darius_tools::ToolOutcome {
        self.tools.execute_model(call)
    }

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
}

fn model_for(state: &RuntimeState) -> Box<dyn Model> {
    match state {
        RuntimeState::Live(provider) => {
            let cache = Arc::new(darius_daemon::CacheCoordinator::new());
            let router = darius_daemon::ModelRouter::new(cache);
            // ModelRouter::route resolves roles to the "default" entry, so the
            // configured provider must overwrite it; a custom-name-only entry
            // would register but never serve (Task 3.2 replaces this legacy
            // router with an exact configured adapter).
            router.register_provider(darius_daemon::Provider {
                name: "default".into(),
                model: provider.model.clone(),
                base_url: provider.base_url.clone(),
                enabled: true,
                api_key_env: provider.key_env.clone(),
            });
            Box::new(darius_daemon::LiveModel::new(
                router,
                darius_daemon::BudgetScope::Session,
            ))
        }
        RuntimeState::OfflineDemo | RuntimeState::Setup => Box::new(
            darius_cognitive::MockModel::new("{\"tasks\":[]}".into(), vec!["DONE".into()]),
        ),
        RuntimeState::MissingKey(_) => unreachable!("missing keys do not build runtimes"),
    }
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
}
