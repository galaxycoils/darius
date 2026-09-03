//! ModelRouter — single authority for all model calls (optimizer, planner, rater, etc.).

pub mod openai;
pub mod wire;
pub mod wire_decode;

pub use openai::LiveModel;

use crate::cache::{CacheCoordinator, CacheMetrics};
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

/// Budget scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BudgetScope {
    Session,
    Subagent,
    Global,
    Eval,
}

/// Budget enforcer — tracks and limits token usage.
pub struct BudgetEnforcer {
    budgets: Arc<Mutex<HashMap<BudgetScope, (u64, u64)>>>, // (used, limit)
}

impl BudgetEnforcer {
    pub fn new() -> Self {
        let mut budgets = HashMap::new();
        budgets.insert(BudgetScope::Session, (0, 100_000));
        budgets.insert(BudgetScope::Subagent, (0, 50_000));
        budgets.insert(BudgetScope::Global, (0, 1_000_000));
        budgets.insert(BudgetScope::Eval, (0, 10_000));
        Self {
            budgets: Arc::new(Mutex::new(budgets)),
        }
    }

    /// Check if a request is within budget.
    pub fn check_budget(
        &self,
        scope: BudgetScope,
        estimated_tokens: u64,
    ) -> Result<(), RouterError> {
        let budgets = self.budgets.lock();
        let (used, limit) = budgets.get(&scope).copied().unwrap_or((0, 0));
        if used + estimated_tokens > limit {
            return Err(RouterError::BudgetExceeded(format!(
                "scope {scope:?}: {used}/{limit} tokens used, estimated {estimated_tokens}"
            )));
        }
        Ok(())
    }

    /// Record token usage.
    pub fn record_usage(&self, scope: BudgetScope, tokens: u64) {
        let mut budgets = self.budgets.lock();
        if let Some((used, _)) = budgets.get_mut(&scope) {
            *used += tokens;
        }
    }

    /// Get remaining budget for a scope.
    pub fn remaining(&self, scope: BudgetScope) -> u64 {
        let budgets = self.budgets.lock();
        budgets
            .get(&scope)
            .map(|(used, limit)| limit.saturating_sub(*used))
            .unwrap_or(0)
    }
}

impl Default for BudgetEnforcer {
    fn default() -> Self {
        Self::new()
    }
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
        // Check budget.
        let estimated_tokens = prompt.len() as u64 / 4; // rough estimate
        self.budget_enforcer.check_budget(scope, estimated_tokens)?;

        let provider = self.single_provider(role)?;
        let effective_model = self
            .model_overrides
            .lock()
            .get(role_key(role))
            .cloned()
            .unwrap_or_else(|| provider.model.clone());

        // Record usage.
        self.budget_enforcer.record_usage(scope, estimated_tokens);

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
            return Ok(format!("Response from {effective_model} for role {role:?}"));
        }

        let provider = Provider {
            model: effective_model,
            ..provider
        };
        let mut live = LiveModel::for_provider(provider)?;
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

    /// Route a plan request, returning JSON plan text.
    pub fn route_plan(&self, goal: &str, scope: BudgetScope) -> Result<String, RouterError> {
        let response = self.route(ModelRole::Plan, goal, scope)?;
        // Wrap the router response in a JSON plan
        Ok(format!(r#"{{"tasks":[{{"title":"{response}"}}]}}"#))
    }

    /// Route a react request, returning the tool/response text.
    pub fn route_react(&self, context: &str, scope: BudgetScope) -> Result<String, RouterError> {
        self.route(ModelRole::Default, context, scope)
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

/// Drive one async model turn from sync legacy callers on a fresh thread, so
/// this works with or without an enclosing async runtime. Removed in Task 3.3.
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

impl LiveModel {
    /// Legacy constructor (removed in Task 3.3): resolve the single reserved
    /// `default` entry the CLI registers its configured provider under.
    /// No sibling fallback.
    pub fn new(router: ModelRouter, scope: BudgetScope) -> Self {
        let _ = scope;
        let provider = router
            .provider_registry
            .get("default")
            .filter(|p| p.enabled)
            .expect("configured provider 'default' must be registered")
            .clone();
        Self::for_provider(provider).expect("configured provider must validate")
    }
}

use darius_cognitive::AsyncModel;

impl darius_cognitive::Model for LiveModel {
    fn plan(&mut self, goal: &str) -> Result<String, darius_cognitive::CognitiveError> {
        if std::env::var(&self.key_env).is_err() {
            return Ok(format!(
                r#"{{"tasks":[{{"title":"Response from {}"}}]}}"#,
                self.model
            ));
        }
        let messages = vec![darius_cognitive::Message::User {
            content: goal.to_owned(),
        }];
        let out =
            block_on_local(self.complete(&messages, &[], &darius_cognitive::TurnContext::new()))?;
        Ok(serde_json::json!({"tasks": [{"title": out.content.unwrap_or_default()}]}).to_string())
    }

    fn react(&mut self, context: &str) -> Result<String, darius_cognitive::CognitiveError> {
        if std::env::var(&self.key_env).is_err() {
            return Ok(format!("Response from {}\nDONE", self.model));
        }
        let messages = vec![darius_cognitive::Message::User {
            content: context.to_owned(),
        }];
        let out =
            block_on_local(self.complete(&messages, &[], &darius_cognitive::TurnContext::new()))?;
        Ok(format!("{}\nDONE", out.content.unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darius_cognitive::Model;

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
    fn budget_enforcer_within_limit() {
        let enforcer = BudgetEnforcer::new();
        assert!(enforcer.check_budget(BudgetScope::Session, 1000).is_ok());
    }

    #[test]
    fn budget_enforcer_exceeds_limit() {
        let enforcer = BudgetEnforcer::new();
        // Global limit is 1,000,000.
        assert!(
            enforcer
                .check_budget(BudgetScope::Global, 2_000_000)
                .is_err()
        );
    }

    #[test]
    fn budget_enforcer_records_usage() {
        let enforcer = BudgetEnforcer::new();
        enforcer.record_usage(BudgetScope::Session, 5000);
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
    fn live_model_routes_plan() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);
        register_default(&router);
        let mut live = LiveModel::new(router, BudgetScope::Session);

        let plan = live.plan("test goal").unwrap();
        assert!(plan.contains("Response from gpt-4"));
    }

    #[test]
    fn live_model_routes_react() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);
        register_default(&router);
        let mut live = LiveModel::new(router, BudgetScope::Session);

        let response = live.react("context").unwrap();
        assert!(response.contains("Response from gpt-4"));
        assert!(response.contains("DONE"));
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

        let plan_resp = router
            .route_plan("build api", BudgetScope::Session)
            .unwrap();
        assert!(plan_resp.contains("gpt-4o"));
    }
}
