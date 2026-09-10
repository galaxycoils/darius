//! Validated construction for one exact live provider and shared budget.
use super::{
    BudgetEnforcer, BudgetScope, Provider, RouterError, anthropic::AnthropicModel,
    openai::OpenAiModel,
};
use darius_cognitive::{AsyncModel, CognitiveError, Message, ModelOutput, ToolSpec, TurnContext};

pub enum LiveBackend {
    OpenAi(OpenAiModel),
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
        let fields = [
            &provider.name,
            &provider.model,
            &provider.base_url,
            &provider.api_key_env,
        ];
        if fields.iter().any(|value| value.trim().is_empty()) {
            return Err(RouterError::Provider("provider config incomplete".into()));
        }
        let is_anthropic = provider.name.eq_ignore_ascii_case("anthropic")
            || provider.base_url.contains("anthropic.com")
            || provider.model.starts_with("claude");

        let base_url = provider.base_url.trim_end_matches('/').to_owned();
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
            LiveBackend::OpenAi(OpenAiModel {
                model: provider.model.clone(),
                base_url: base_url.clone(),
                key_env: provider.api_key_env.clone(),
                client: client.clone(),
                budget: budget.clone(),
                scope,
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
            LiveBackend::OpenAi(m) => m.complete(messages, tools, ctx).await,
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
            LiveBackend::OpenAi(m) => m.complete_stream(messages, tools, sink, ctx).await,
            LiveBackend::Anthropic(m) => m.complete_stream(messages, tools, sink, ctx).await,
        }
    }
}
