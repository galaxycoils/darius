//! Validated construction for one exact live provider and shared budget.
use super::{
    BudgetEnforcer, BudgetScope, Provider, ProviderRouting, RouterError, anthropic::AnthropicModel,
    chat_completions::ChatCompletionsModel,
};
use darius_cognitive::{AsyncModel, CognitiveError, Message, ModelOutput, ToolSpec, TurnContext};

pub enum LiveBackend {
    ChatCompletions(ChatCompletionsModel),
    Anthropic(AnthropicModel),
}

pub struct LiveModel {
    pub model: String,
    pub base_url: String,
    pub key_env: String,
    pub client: reqwest::Client,
    pub budget: BudgetEnforcer,
    pub scope: BudgetScope,
    pub backend: LiveBackend,
}

impl LiveModel {
    pub fn for_provider(provider: Provider) -> Result<Self, RouterError> {
        Self::for_provider_with_budget(provider, BudgetEnforcer::new(), BudgetScope::Session)
    }

    pub fn for_provider_with_budget(
        provider: Provider,
        budget: BudgetEnforcer,
        scope: BudgetScope,
    ) -> Result<Self, RouterError> {
        provider.validate()?;
        let is_anthropic = provider.name == "anthropic";

        let mut base_url = provider.base_url.trim_end_matches('/').to_owned();
        // 3.5: NVIDIA local NIM override — a set `NVIDIA_BASE_URL` wins over
        // the profile default for nvidia providers only; empty values and
        // every other provider keep the configured base_url.
        if provider
            .profile()
            .is_some_and(|profile| profile.id == "nvidia")
            && let Ok(nim_url) = std::env::var("NVIDIA_BASE_URL")
            && !nim_url.trim().is_empty()
        {
            base_url = nim_url.trim_end_matches('/').to_owned();
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|_| RouterError::Provider("http client init failed".into()))?;

        let backend = if is_anthropic {
            LiveBackend::Anthropic(AnthropicModel {
                model: provider.model.clone(),
                base_url: base_url.clone(),
                key_env: provider.api_key_env.clone(),
                client: client.clone(),
                budget: budget.clone(),
                scope,
            })
        } else {
            LiveBackend::ChatCompletions(ChatCompletionsModel {
                profile: provider.profile(),
                model: provider.model.clone(),
                base_url: base_url.clone(),
                key_env: provider.api_key_env.clone(),
                client: client.clone(),
                budget: budget.clone(),
                scope,
                max_output_tokens: None,
                provider_routing: None,
            })
        };

        Ok(Self {
            model: provider.model,
            base_url,
            key_env: provider.api_key_env,
            client,
            budget,
            scope,
            backend,
        })
    }

    pub fn is_anthropic(&self) -> bool {
        matches!(self.backend, LiveBackend::Anthropic(_))
    }

    /// Attach OpenRouter routing prefs after construction. No-op for the
    /// Anthropic backend and for `None`; the body-build gate (3.6) additionally
    /// drops routing for non-OpenRouter profiles, so callers need no branching.
    pub fn with_provider_routing(mut self, routing: Option<ProviderRouting>) -> Self {
        if let LiveBackend::ChatCompletions(m) = &mut self.backend {
            m.provider_routing = routing;
        }
        self
    }
}

#[async_trait::async_trait]
impl AsyncModel for LiveModel {
    async fn complete(
        &mut self,
        messages: &[Message],
        tools: &[ToolSpec],
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        match &mut self.backend {
            LiveBackend::ChatCompletions(m) => m.complete(messages, tools, ctx).await,
            LiveBackend::Anthropic(m) => m.complete(messages, tools, ctx).await,
        }
    }

    async fn complete_stream(
        &mut self,
        messages: &[Message],
        tools: &[ToolSpec],
        sink: &dyn darius_cognitive::EventSink,
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        match &mut self.backend {
            LiveBackend::ChatCompletions(m) => m.complete_stream(messages, tools, sink, ctx).await,
            LiveBackend::Anthropic(m) => m.complete_stream(messages, tools, sink, ctx).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Provider;
    use super::LiveModel;

    /// 3.5 config tests: nvidia profile default base_url, `NVIDIA_BASE_URL`
    /// override (local NIM), empty override ignored, non-nvidia unaffected.
    /// Single test fn: the env var is process-global, so the cases run
    /// sequentially here instead of racing across parallel tests.
    #[test]
    fn nvidia_base_url_override() {
        unsafe { std::env::remove_var("NVIDIA_BASE_URL") };
        let nvidia = darius_core::providers::resolve("nvidia").unwrap();
        assert_eq!(nvidia.default_max_tokens, Some(16384));

        let live = LiveModel::for_provider(Provider::from_profile("nvidia").unwrap()).unwrap();
        assert_eq!(live.base_url, nvidia.base_url);

        unsafe { std::env::set_var("NVIDIA_BASE_URL", "http://localhost:8000/v1") };
        let live = LiveModel::for_provider(Provider::from_profile("nvidia").unwrap()).unwrap();
        assert_eq!(live.base_url, "http://localhost:8000/v1");

        // Alias resolves to the same nvidia profile, override still applies.
        let live = LiveModel::for_provider(Provider::from_profile("nvidia-nim").unwrap()).unwrap();
        assert_eq!(live.base_url, "http://localhost:8000/v1");

        // Empty override is ignored.
        unsafe { std::env::set_var("NVIDIA_BASE_URL", "   ") };
        let live = LiveModel::for_provider(Provider::from_profile("nvidia").unwrap()).unwrap();
        assert_eq!(live.base_url, nvidia.base_url);

        // Non-nvidia providers never see the override.
        unsafe { std::env::set_var("NVIDIA_BASE_URL", "http://localhost:8000/v1") };
        let mut other = Provider::from_profile("openrouter").unwrap();
        other.base_url = "https://openrouter.ai/api/v1".into();
        let live = LiveModel::for_provider(other).unwrap();
        assert_eq!(live.base_url, "https://openrouter.ai/api/v1");

        unsafe { std::env::remove_var("NVIDIA_BASE_URL") };
    }
}
