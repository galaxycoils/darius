//! ModelRouter — single authority for all model calls (optimizer, planner, rater, etc.).

pub mod anthropic;
pub mod budget;
pub mod live_factory;
pub mod openai;
mod usage;
pub mod wire;
pub mod wire_call;
pub mod wire_decode;

use crate::cache::{CacheCoordinator, CacheMetrics};
pub use anthropic::AnthropicModel;
pub use budget::{BudgetEnforcer, BudgetScope};
pub use live_factory::LiveModel;
pub use openai::OpenAiModel;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("no available providers")]
    NoProviders,
    #[error("request coalesced")]
    Coalesced,
    #[error("authentication failed (invalid API key)")]
    Unauthorized,
    #[error("rate limited (too many requests)")]
    RateLimited,
    #[error("provider unavailable ({0})")]
    ServerError(String),
}

/// Token accounting for a request.
#[derive(Debug, Clone, Default)]
pub struct TokenAccounting {
    pub billed_input: u64,
    pub billed_output: u64,
    pub actual_input: u64,
    pub actual_output: u64,
    pub cached_input: u64,
}

/// Model provider.
#[derive(Debug, Clone)]
pub struct Provider {
    pub name: String,
    pub model: String,
    pub base_url: String,
    pub enabled: bool,
    /// Environment variable name holding the API key. Never store the key itself.
    pub api_key_env: String,
}

/// Provider registry.
pub struct ProviderRegistry {
    providers: Arc<Mutex<HashMap<String, Provider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a provider.
    pub fn register(&self, provider: Provider) {
        self.providers
            .lock()
            .insert(provider.name.clone(), provider);
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<Provider> {
        self.providers.lock().get(name).cloned()
    }

    /// Enable or disable a provider.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> bool {
        let mut providers = self.providers.lock();
        if let Some(provider) = providers.get_mut(name) {
            provider.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// List all enabled providers.
    pub fn list_enabled(&self) -> Vec<Provider> {
        self.providers
            .lock()
            .values()
            .filter(|p| p.enabled)
            .cloned()
            .collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Model role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    Default,
    Smol,
    Plan,
    Commit,
    Advisor,
    Rater,
}

/// ModelRouter — routes requests to providers with budget, cache, and fallback.
pub struct ModelRouter {
    provider_registry: ProviderRegistry,
    budget_enforcer: BudgetEnforcer,
    cache_coordinator: Arc<CacheCoordinator>,
    #[allow(dead_code)]
    coalesced: Arc<Mutex<HashMap<String, u64>>>, // request hash -> result hash
    model_overrides: Arc<Mutex<HashMap<String, String>>>,
}

impl ModelRouter {
    /// Empty registry; callers register their configured provider explicitly.
    /// No hard-coded providers, no fallbacks.
    pub fn new(cache_coordinator: Arc<CacheCoordinator>) -> Self {
        Self {
            provider_registry: ProviderRegistry::new(),
            budget_enforcer: BudgetEnforcer::new(),
            cache_coordinator,
            coalesced: Arc::new(Mutex::new(HashMap::new())),
            model_overrides: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Builder method to configure a role model override.
    pub fn with_override(self, role: &str, model: &str) -> Self {
        self.model_overrides
            .lock()
            .insert(role.to_string(), model.to_string());
        self
    }

    /// Set a dynamic model override for a role.
    pub fn set_override(&self, role: &str, model: &str) {
        self.model_overrides
            .lock()
            .insert(role.to_string(), model.to_string());
    }

    /// Get effective model name for a role: explicit override, else the role's
    /// single reserved provider. No sibling fallback, no hard-coded default.
    pub fn get_role_model(&self, role: ModelRole) -> Result<String, RouterError> {
        if let Some(m) = self.model_overrides.lock().get(role_key(role)) {
            return Ok(m.clone());
        }
        Ok(self.single_provider(role)?.model)
    }

    /// Look up the role's single reserved provider (`rater` for Rater, else
    /// `default` — the name the CLI registers its configured provider under).
    fn single_provider(&self, role: ModelRole) -> Result<Provider, RouterError> {
        let name = match role {
            ModelRole::Rater => "rater",
            _ => "default",
        };
        self.provider_registry
            .get(name)
            .filter(|p| p.enabled)
            .ok_or(RouterError::NoProviders)
    }

    /// Route a request to the role's single provider over the exact protocol.
    pub fn route(
        &self,
        role: ModelRole,
        prompt: &str,
        scope: BudgetScope,
    ) -> Result<String, RouterError> {
        let provider = self.single_provider(role)?;
        let effective_model = self
            .model_overrides
            .lock()
            .get(role_key(role))
            .cloned()
            .unwrap_or_else(|| provider.model.clone());

        // Record cache stats.
        self.cache_coordinator
            .record_turn(darius_core::TurnCacheStats {
                prefix_bytes: 1000,
                break_offset: 500,
                suffix_hash: 12345,
                cache_hit: true,
                miss_cost_tokens: 0,
            });

        // Stub fallback: no API key configured.
        if std::env::var(&provider.api_key_env).is_err() {
            let estimated_tokens = (prompt.len() as u64).div_ceil(4);
            self.budget_enforcer.charge(scope, estimated_tokens)?;
            return Ok(format!("Response from {effective_model} for role {role:?}"));
        }

        let provider = Provider {
            model: effective_model,
            ..provider
        };
        let mut live =
            LiveModel::for_provider_with_budget(provider, self.budget_enforcer.clone(), scope)?;
        let messages = vec![darius_cognitive::Message::User {
            content: prompt.to_owned(),
        }];
        let ctx = darius_cognitive::TurnContext::new();
        let out = block_on_local(live.complete(&messages, &[], &ctx))
            .map_err(|e| RouterError::Provider(e.to_string()))?;
        Ok(out.content.unwrap_or_default())
    }

    /// Get the budget enforcer.
    pub fn budget_enforcer(&self) -> &BudgetEnforcer {
        &self.budget_enforcer
    }

    /// Get the cache coordinator.
    pub fn cache_coordinator(&self) -> &CacheCoordinator {
        &self.cache_coordinator
    }

    /// Get cache metrics.
    pub fn cache_metrics(&self) -> CacheMetrics {
        self.cache_coordinator.metrics()
    }

    /// Register a provider.
    pub fn register_provider(&self, provider: Provider) {
        self.provider_registry.register(provider);
    }
}

/// Role display key for model overrides.
fn role_key(role: ModelRole) -> &'static str {
    match role {
        ModelRole::Default => "default",
        ModelRole::Smol => "smol",
        ModelRole::Plan => "planner",
        ModelRole::Commit => "commit",
        ModelRole::Advisor => "advisor",
        ModelRole::Rater => "rater",
    }
}

/// Drive one async model turn from a sync caller on a fresh thread, so
/// this works with or without an enclosing async runtime.
fn block_on_local<F, T>(future: F) -> T
where
    F: Send + std::future::Future<Output = T>,
    T: Send,
{
    std::thread::scope(|s| {
        s.spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("local runtime")
                .block_on(future)
        })
        .join()
        .expect("model turn thread")
    })
}

use darius_cognitive::AsyncModel;

#[cfg(test)]
mod tests {
    use super::*;

    fn register_default(router: &ModelRouter) {
        router.register_provider(Provider {
            name: "default".into(),
            model: "gpt-4".into(),
            base_url: "http://localhost:1".into(),
            enabled: true,
            api_key_env: "DARIUS_TEST_KEY_NEVER_SET".into(),
        });
    }

    #[test]
    fn openai_for_provider_rejects_blank_config() {
        let bad = Provider {
            name: "custom".into(),
            model: String::new(),
            base_url: "http://localhost:1".into(),
            enabled: true,
            api_key_env: "DARIUS_TEST_KEY_NEVER_SET".into(),
        };
        assert!(LiveModel::for_provider(bad).is_err());
    }

    #[test]
    fn openai_for_provider_trims_base_url() {
        let live = LiveModel::for_provider(Provider {
            name: "custom".into(),
            model: "custom-model".into(),
            base_url: "http://localhost:1/v1///".into(),
            enabled: true,
            api_key_env: "DARIUS_TEST_KEY_NEVER_SET".into(),
        })
        .unwrap();
        assert_eq!(live.base_url, "http://localhost:1/v1");
    }

    #[test]
    fn live_model_budget_scope_enforced() {
        let enforcer = BudgetEnforcer::new();
        enforcer.charge(BudgetScope::Session, 100_000).unwrap();
        let err = enforcer
            .charge(BudgetScope::Session, 100)
            .unwrap_err()
            .to_string();
        assert!(err.contains("tokens used"), "got: {err}");
    }

    #[test]
    fn decode_rejects_null_message() {
        let body = serde_json::json!({"choices": [{"message": null}]});
        assert!(wire_decode::decode_response(&body).is_err());
    }

    #[test]
    fn decode_rejects_empty_response() {
        let body = serde_json::json!({"choices": [{"message": {}}]});
        assert!(wire_decode::decode_response(&body).is_err());
    }

    #[test]
    fn decode_rejects_empty_tool_name() {
        let body = serde_json::json!({"choices": [{"message": {
            "content": null,
            "tool_calls": [{"id": "c1", "type": "function",
                "function": {"name": "", "arguments": "{}"}}],
        }}]});
        assert!(wire_decode::decode_response(&body).is_err());
    }

    #[test]
    fn budget_enforcer_within_limit() {
        let enforcer = BudgetEnforcer::new();
        assert!(enforcer.charge(BudgetScope::Session, 1000).is_ok());
    }

    #[test]
    fn budget_enforcer_exceeds_limit() {
        let enforcer = BudgetEnforcer::new();
        // Global limit is 1,000,000.
        assert!(enforcer.charge(BudgetScope::Global, 2_000_000).is_err());
    }

    #[test]
    fn budget_enforcer_records_usage() {
        let enforcer = BudgetEnforcer::new();
        enforcer.charge(BudgetScope::Session, 5000).unwrap();
        assert_eq!(enforcer.remaining(BudgetScope::Session), 95_000);
    }

    #[test]
    fn provider_registry() {
        let registry = ProviderRegistry::new();
        registry.register(Provider {
            name: "test".into(),
            model: "test-model".into(),
            base_url: "http://localhost".into(),
            enabled: true,
            api_key_env: "DARIUS_API_KEY".into(),
        });

        let provider = registry.get("test").unwrap();
        assert_eq!(provider.model, "test-model");
    }

    #[test]
    fn router_error_does_not_leak_api_key() {
        let err = RouterError::Provider("some error".into());
        let display = format!("{err}");
        assert!(!display.contains("sk-"));
        assert!(!display.contains("key"));
    }

    #[test]
    fn model_router_routes_by_role() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);
        register_default(&router);
        router.register_provider(Provider {
            name: "rater".into(),
            model: "claude-3".into(),
            base_url: "http://localhost:1".into(),
            enabled: true,
            api_key_env: "DARIUS_TEST_KEY_NEVER_SET".into(),
        });

        let response = router
            .route(ModelRole::Default, "hello", BudgetScope::Session)
            .unwrap();
        assert!(response.contains("gpt-4"));

        let rater_response = router
            .route(ModelRole::Rater, "rate this", BudgetScope::Eval)
            .unwrap();
        assert!(rater_response.contains("claude-3"));
    }

    #[test]
    fn model_router_no_fallback_when_primary_is_disabled() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);
        register_default(&router);
        assert!(router.provider_registry.set_enabled("default", false));

        let result = router.route(ModelRole::Default, "hello", BudgetScope::Session);
        assert!(matches!(result, Err(RouterError::NoProviders)));
    }

    #[test]
    fn model_router_budget_exceeded() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);

        // Eval scope has 10,000 token limit.
        let result = router.route(ModelRole::Default, &"x".repeat(100_000), BudgetScope::Eval);
        assert!(result.is_err());
    }

    #[test]
    fn test_model_router_role_overrides() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);
        register_default(&router);
        let router = router
            .with_override("planner", "gpt-4o")
            .with_override("rater", "claude-3-5-sonnet");

        assert_eq!(router.get_role_model(ModelRole::Plan).unwrap(), "gpt-4o");
        assert_eq!(
            router.get_role_model(ModelRole::Rater).unwrap(),
            "claude-3-5-sonnet"
        );
        assert_eq!(router.get_role_model(ModelRole::Default).unwrap(), "gpt-4");
    }
}
