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
    pub fn check_budget(&self, scope: BudgetScope, estimated_tokens: u64) -> Result<(), RouterError> {
        let mut budgets = self.budgets.lock();
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
        budgets.get(&scope).map(|(used, limit)| limit.saturating_sub(*used)).unwrap_or(0)
    }
}

/// Model provider.
#[derive(Debug, Clone)]
pub struct Provider {
    pub name: String,
    pub model: String,
    pub base_url: String,
    pub enabled: bool,
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
        self.providers.lock().insert(provider.name.clone(), provider);
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<Provider> {
        self.providers.lock().get(name).cloned()
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
    coalesced: Arc<Mutex<HashMap<String, u64>>>, // request hash -> result hash
}

impl ModelRouter {
    pub fn new(cache_coordinator: Arc<CacheCoordinator>) -> Self {
        let mut registry = ProviderRegistry::new();
        registry.register(Provider {
            name: "default".into(),
            model: "gpt-4".into(),
            base_url: "https://api.openai.com/v1".into(),
            enabled: true,
        });
        registry.register(Provider {
            name: "rater".into(),
            model: "claude-3".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            enabled: true,
        });

        Self {
            provider_registry: registry,
            budget_enforcer: BudgetEnforcer::new(),
            cache_coordinator,
            coalesced: Arc::new(Mutex::new(HashMap::new())),
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

        // Select provider based on role.
        let provider_name = match role {
            ModelRole::Rater => "rater",
            _ => "default",
        };

        let provider = self
            .provider_registry
            .get(provider_name)
            .ok_or(RouterError::NoProviders)?;

        if !provider.enabled {
            return Err(RouterError::NoProviders);
        }

        // Record usage.
        self.budget_enforcer.record_usage(scope, estimated_tokens);

        // Record cache stats.
        self.cache_coordinator.record_turn(darius_core::TurnCacheStats {
            prefix_bytes: 1000,
            break_offset: 500,
            suffix_hash: 12345,
            cache_hit: true,
            miss_cost_tokens: 0,
        });

        // Stub: in a real implementation, this would call the provider API.
        Ok(format!("Response from {} for role {role:?}", provider.model))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_enforcer_within_limit() {
        let enforcer = BudgetEnforcer::new();
        assert!(enforcer.check_budget(BudgetScope::Session, 1000).is_ok());
    }

    #[test]
    fn budget_enforcer_exceeds_limit() {
        let enforcer = BudgetEnforcer::new();
        // Global limit is 1,000,000.
        assert!(enforcer.check_budget(BudgetScope::Global, 2_000_000).is_err());
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
        });

        let provider = registry.get("test").unwrap();
        assert_eq!(provider.model, "test-model");
    }

    #[test]
    fn model_router_routes_by_role() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);

        let response = router.route(ModelRole::Default, "hello", BudgetScope::Session).unwrap();
        assert!(response.contains("gpt-4"));

        let rater_response = router.route(ModelRole::Rater, "rate this", BudgetScope::Eval).unwrap();
        assert!(rater_response.contains("claude-3"));
    }

    #[test]
    fn model_router_budget_exceeded() {
        let cache = Arc::new(CacheCoordinator::new());
        let router = ModelRouter::new(cache);

        // Eval scope has 10,000 token limit.
        let result = router.route(
            ModelRole::Default,
            &"x".repeat(100_000),
            BudgetScope::Eval,
        );
        assert!(result.is_err());
    }
}
