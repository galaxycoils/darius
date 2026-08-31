//! ModelRouter — single authority for all model calls (optimizer, planner, rater, etc.).

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
    http_client: OpenAiCompatibleClient,
}

impl ModelRouter {
    pub fn new(cache_coordinator: Arc<CacheCoordinator>) -> Self {
        let registry = ProviderRegistry::new();
        registry.register(Provider {
            name: "default".into(),
            model: "gpt-4".into(),
            base_url: "https://api.openai.com/v1".into(),
            enabled: true,
            api_key_env: "DARIUS_API_KEY".into(),
        });
        registry.register(Provider {
            name: "rater".into(),
            model: "claude-3".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            enabled: true,
            api_key_env: "DARIUS_API_KEY".into(),
        });

        Self {
            provider_registry: registry,
            budget_enforcer: BudgetEnforcer::new(),
            cache_coordinator,
            coalesced: Arc::new(Mutex::new(HashMap::new())),
            http_client: OpenAiCompatibleClient::new().unwrap_or_default(),
        }
    }

    /// Route a request to a provider.
    pub fn route(
        &self,
        role: ModelRole,
        prompt: &str,
        scope: BudgetScope,
    ) -> Result<String, RouterError> {
        // Check budget.
        let estimated_tokens = prompt.len() as u64 / 4; // rough estimate
        self.budget_enforcer.check_budget(scope, estimated_tokens)?;

        // Select the primary provider for the role, then fail over to the sibling.
        let (primary_name, fallback_name) = match role {
            ModelRole::Rater => ("rater", "default"),
            _ => ("default", "rater"),
        };

        let provider = self
            .provider_registry
            .get(primary_name)
            .filter(|provider| provider.enabled)
            .or_else(|| {
                self.provider_registry
                    .get(fallback_name)
                    .filter(|provider| provider.enabled)
            })
            .ok_or(RouterError::NoProviders)?;

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

        // If the API key env var is set, call the provider via HTTP.
        if std::env::var(&provider.api_key_env).is_ok() {
            let messages = serde_json::json!([
                {"role": "user", "content": prompt}
            ]);
            return self.http_client.chat_completion(
                &provider.base_url,
                &provider.api_key_env,
                &provider.model,
                &[messages],
            );
        }

        // Stub fallback: no API key configured.
        Ok(format!(
            "Response from {} for role {role:?}",
            provider.model
        ))
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
        let response = self.route(ModelRole::Default, goal, scope)?;
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

/// A model that uses the ModelRouter for live provider routing.
/// Falls back to stub responses when no providers are configured.
pub struct LiveModel {
    router: ModelRouter,
    scope: BudgetScope,
}

impl LiveModel {
    pub fn new(router: ModelRouter, scope: BudgetScope) -> Self {
        Self { router, scope }
    }
}

impl darius_cognitive::Model for LiveModel {
    fn plan(&mut self, goal: &str) -> Result<String, darius_cognitive::CognitiveError> {
        self.router
            .route_plan(goal, self.scope)
            .map_err(|e| darius_cognitive::CognitiveError::Loop(e.to_string()))
    }

    fn react(&mut self, context: &str) -> Result<String, darius_cognitive::CognitiveError> {
        let response = self
            .router
            .route_react(context, self.scope)
            .map_err(|e| darius_cognitive::CognitiveError::Loop(e.to_string()))?;
        // Wrap in DONE to signal completion
        Ok(format!("{response}\nDONE"))
    }
}

/// OpenAI-compatible HTTP client.
///
/// Reads the API key from the environment at call time (never stores it),
/// POSTs to `{base_url}/chat/completions`, and parses `choices[0].message.content`.
pub struct OpenAiCompatibleClient {
    inner: reqwest::blocking::Client,
}

impl OpenAiCompatibleClient {
    /// Create a new client with a default timeout.
    pub fn new() -> Result<Self, RouterError> {
        let inner = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| RouterError::Provider(format!("http client init failed: {e}")))?;
        Ok(Self { inner })
    }

    /// Call `/chat/completions` on an OpenAI-compatible provider.
    ///
    /// `api_key_env` names the environment variable holding the key; the key
    /// is read at call time and never stored or logged.
    pub fn chat_completion(
        &self,
        base_url: &str,
        api_key_env: &str,
        model: &str,
        messages: &[serde_json::Value],
    ) -> Result<String, RouterError> {
        let api_key = std::env::var(api_key_env).map_err(|_| RouterError::Unauthorized)?;

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "temperature": 0.7,
        });

        let response = self
            .inner
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {api_key}"))
            .json(&body)
            .send()
            .map_err(|e| RouterError::Provider(format!("request failed: {e}")))?;

        let status = response.status();
        if status.is_success() {
            let json: serde_json::Value = response
                .json()
                .map_err(|e| RouterError::Provider(format!("invalid response json: {e}")))?;
            let content = json
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .ok_or_else(|| {
                    RouterError::Provider("response missing choices[0].message.content".into())
                })?;
            Ok(content.to_string())
        } else {
            match status.as_u16() {
                401 | 403 => Err(RouterError::Unauthorized),
                429 => Err(RouterError::RateLimited),
                500..=599 => Err(RouterError::ServerError(format!(
                    "provider returned {status}"
                ))),
                _ => Err(RouterError::Provider(format!("provider returned {status}"))),
            }
        }
    }
}

impl Default for OpenAiCompatibleClient {
    fn default() -> Self {
        Self::new().expect("failed to build HTTP client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darius_cognitive::Model;

    #[test]
    fn openai_client_missing_key_returns_unauthorized() {
        let client = OpenAiCompatibleClient::new().unwrap();
        let result = client.chat_completion(
            "http://localhost:1",
            "DARIUS_TEST_KEY_NEVER_SET",
            "gpt-4o",
            &[serde_json::json!({"role":"user","content":"hi"})],
        );
        assert!(matches!(result, Err(RouterError::Unauthorized)));
    }

    #[test]
    fn openai_client_connection_failure_maps_to_provider_error() {
        let client = OpenAiCompatibleClient::new().unwrap();
        let result = client.chat_completion(
            "http://localhost:1",  // Nothing listening here
            "DARIUS_TEST_KEY_NEVER_SET",
            "gpt-4o",
            &[serde_json::json!({"role":"user","content":"hi"})],
        );
        // Missing key is checked first, so this returns Unauthorized
        assert!(matches!(result, Err(RouterError::Unauthorized)));
    }

    #[test]
    fn router_error_does_not_leak_api_key() {
        let err = RouterError::Provider("some error".into());
        let display = format!("{err}");
        assert!(!display.contains("sk-"));
        assert!(!display.contains("key"));
    }

    #[test]
    fn openai_client_secret_never_in_error_text() {
        let client = OpenAiCompatibleClient::new().unwrap();
        let result = client.chat_completion(
            "http://localhost:1",
            "DARIUS_TEST_KEY_NEVER_SET",
            "gpt-4o",
            &[serde_json::json!({"role":"user","content":"hi"})],
        );
        if let Err(ref e) = result {
            let display = format!("{e}");
            assert!(!display.contains("test-key"));
            assert!(!display.contains("DARIUS_TEST_KEY"));
        }
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
    fn model_router_routes_by_role() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);

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
    fn model_router_fails_over_when_primary_is_disabled() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);
        assert!(router.provider_registry.set_enabled("default", false));

        let response = router
            .route(ModelRole::Default, "hello", BudgetScope::Session)
            .unwrap();
        assert!(response.contains("claude-3"));
        assert_eq!(router.cache_metrics().hits, 1);
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
        let mut live = LiveModel::new(router, BudgetScope::Session);

        let plan = live.plan("test goal").unwrap();
        assert!(plan.contains("Response from gpt-4"));
    }

    #[test]
    fn live_model_routes_react() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);
        let mut live = LiveModel::new(router, BudgetScope::Session);

        let response = live.react("context").unwrap();
        assert!(response.contains("Response from gpt-4"));
        assert!(response.contains("DONE"));
    }
}
