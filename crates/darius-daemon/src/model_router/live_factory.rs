//! Validated construction for one exact live provider and shared budget.
use super::{BudgetEnforcer, BudgetScope, LiveModel, Provider, RouterError};

impl LiveModel {
    pub fn for_provider(provider: Provider) -> Result<Self, RouterError> {
        Self::for_provider_with_budget(provider, BudgetEnforcer::new(), BudgetScope::Session)
    }

    pub fn for_provider_with_budget(
        provider: Provider,
        budget: BudgetEnforcer,
        scope: BudgetScope,
    ) -> Result<Self, RouterError> {
        let fields = [
            &provider.name,
            &provider.model,
            &provider.base_url,
            &provider.api_key_env,
        ];
        if fields.iter().any(|value| value.trim().is_empty()) {
            return Err(RouterError::Provider("provider config incomplete".into()));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|_| RouterError::Provider("http client init failed".into()))?;
        Ok(Self {
            model: provider.model,
            base_url: provider.base_url.trim_end_matches('/').to_owned(),
            key_env: provider.api_key_env,
            client,
            budget,
            scope,
        })
    }
}
