//! One reusable session runtime shared by CLI, TUI, web, and A2A.
//!
//! Constructs profile/config/memory/tools/model once so every surface
//! runs the same cognitive loop against the same dependencies.

use std::path::PathBuf;
use std::sync::Arc;

use darius_cognitive::{LoopPolicy, Model, RunMetadata, UiEvent};
use darius_memory::MemoryEngine;
use darius_tools::ToolRegistry;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::config::ProfileConfig;

/// Errors that can occur when building a session runtime.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("missing API key: set {0}")]
    MissingApiKey(String),
    #[error("tool error: {0}")]
    Tool(#[from] darius_tools::ToolError),
    #[error("memory error: {0}")]
    Memory(#[from] darius_memory::MemoryError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration for building a session runtime.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub profile: String,
    pub profile_dir: PathBuf,
}

impl RuntimeConfig {
    pub fn from_profile(profile: &str) -> Self {
        let profile_dir = ProfileConfig::profile_dir(profile);
        Self {
            profile: profile.into(),
            profile_dir,
        }
    }
}

/// A fully-constructed session runtime ready to run cognitive loops.
pub struct SessionRuntime {
    pub config: RuntimeConfig,
    pub profile_config: ProfileConfig,
    pub memory: MemoryEngine,
    pub tools: ToolRegistry,
    pub model: Box<dyn Model>,
    pub metadata: RunMetadata,
    pub event_sender: broadcast::Sender<UiEvent>,
    pub cancellation: CancellationToken,
    pub policy: LoopPolicy,
}

impl SessionRuntime {
    /// Build a session runtime from a profile name.
    ///
    /// - Loads `ProfileConfig`.
    /// - Creates `MemoryEngine` and `ToolRegistry`.
    /// - Registers memory/task/coding tools.
    /// - Selects Mock only when no model config exists.
    /// - Returns `MissingApiKey` when config exists but key is absent.
    pub fn from_profile(profile: &str) -> Result<Self, RuntimeError> {
        let config = RuntimeConfig::from_profile(profile);
        std::fs::create_dir_all(&config.profile_dir)?;

        let profile_config = ProfileConfig::load(profile);
        let memory = MemoryEngine::open(&config.profile_dir)?;
        let mut tools = ToolRegistry::new(&config.profile_dir)?;

        // Register builtins.
        darius_tools::register_memory_builtins(&mut tools, &memory);
        let board = Arc::new(parking_lot::Mutex::new(darius_tools::TaskBoard::new(15)));
        darius_tools::register_task_builtins(&mut tools, board);
        darius_tools::register_coding_builtins(&mut tools);

        let (model, model_label) = if profile_config.model.is_none() {
            // No model config: use offline Mock.
            let plan_response =
                r#"{"tasks":[{"title":"Plan for the given goal"}]}"#.to_string();
            let react_responses = vec![
                r#"TOOL {"name":"memory_remember","arguments":{"body":"working on task"}}"#.to_string(),
                "DONE".to_string(),
            ];
            (
                Box::new(darius_cognitive::MockModel::new(plan_response, react_responses))
                    as Box<dyn Model>,
                "mock".to_string(),
            )
        } else {
            // Model config exists: check for API key.
            let env_name = profile_config
                .model
                .as_ref()
                .and_then(|m| m.api_key_env.as_deref())
                .unwrap_or("DARIUS_API_KEY");
            match std::env::var(env_name) {
                Ok(_) => {
                    // API key present: build a live model.
                    let cache = Arc::new(darius_daemon::CacheCoordinator::new());
                    let router = darius_daemon::ModelRouter::new(cache);
                    if let Some(ref model_config) = profile_config.model {
                        router.register_provider(darius_daemon::Provider {
                            name: model_config.provider.clone(),
                            model: model_config.model.clone(),
                            base_url: model_config.base_url.clone(),
                            enabled: true,
                            api_key_env: model_config
                                .api_key_env
                                .clone()
                                .unwrap_or_else(|| "DARIUS_API_KEY".into()),
                        });
                    }
                    (
                        Box::new(darius_daemon::LiveModel::new(
                            router,
                            darius_daemon::BudgetScope::Session,
                        )) as Box<dyn Model>,
                        profile_config
                            .model
                            .as_ref()
                            .map(|m| m.model.clone())
                            .unwrap_or_else(|| "live".into()),
                    )
                }
                Err(_) => {
                    return Err(RuntimeError::MissingApiKey(env_name.into()));
                }
            }
        };

        let metadata = RunMetadata {
            profile: profile.into(),
            model: model_label,
            mode: "auto".into(),
        };

        let (event_sender, _) = broadcast::channel(256);
        let cancellation = CancellationToken::new();
        let policy = LoopPolicy::default();

        Ok(Self {
            config,
            profile_config,
            memory,
            tools,
            model,
            metadata,
            event_sender,
            cancellation,
            policy,
        })
    }

    /// Subscribe to the event broadcast channel.
    pub fn subscribe_events(&self) -> broadcast::Receiver<UiEvent> {
        self.event_sender.subscribe()
    }

    /// Get a clone of the cancellation token.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_profile(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("darius_runtime_test_{}_{}", name, uuid::Uuid::new_v4()));
        dir.to_string_lossy().to_string()
    }

    #[test]
    fn from_profile_offline_mock_when_no_config() {
        let profile_dir = temp_profile("offline");
        std::fs::create_dir_all(&profile_dir).unwrap();
        let profile_name = format!("offline_{}", uuid::Uuid::new_v4());
        // Use a temp dir as the profile dir by creating a custom runtime config.
        // Since from_profile uses ProfileConfig::profile_dir, we test the real path.
        // Instead, test that a profile without config uses mock.
        let rt = SessionRuntime::from_profile(&profile_name).unwrap();
        assert_eq!(rt.metadata.model, "mock");
        assert_eq!(rt.metadata.profile, profile_name);
        // Cleanup
        let _ = std::fs::remove_dir_all(
            dirs::home_dir()
                .unwrap()
                .join(".darius")
                .join("profiles")
                .join(&profile_name),
        );
    }

    #[test]
    fn from_profile_missing_api_key_error() {
        let profile_name = format!("missingkey_{}", uuid::Uuid::new_v4());
        let profile_dir = dirs::home_dir()
            .unwrap()
            .join(".darius")
            .join("profiles")
            .join(&profile_name);
        std::fs::create_dir_all(&profile_dir).unwrap();
        // Write a config that references an env var that won't exist.
        let config = r#"
[model]
provider = "openai_compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "DARIUS_TEST_MISSING_KEY_NEVER_SET"
"#;
        std::fs::write(profile_dir.join("config.toml"), config).unwrap();

        let result = SessionRuntime::from_profile(&profile_name);
        assert!(matches!(result, Err(RuntimeError::MissingApiKey(_))));
        if let Err(RuntimeError::MissingApiKey(env)) = result {
            assert_eq!(env, "DARIUS_TEST_MISSING_KEY_NEVER_SET");
        }

        let _ = std::fs::remove_dir_all(&profile_dir);
    }

    #[test]
    fn runtime_exposes_event_broadcaster_and_cancellation() {
        let profile_name = format!("meta_{}", uuid::Uuid::new_v4());
        let rt = SessionRuntime::from_profile(&profile_name).unwrap();

        let _rx = rt.subscribe_events();
        let token = rt.cancellation_token();
        assert!(!token.is_cancelled());

        let _ = std::fs::remove_dir_all(
            dirs::home_dir()
                .unwrap()
                .join(".darius")
                .join("profiles")
                .join(&profile_name),
        );
    }
}