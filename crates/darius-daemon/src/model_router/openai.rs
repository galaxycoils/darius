//! LiveModel owns one validated provider; exact cancellable protocol, no fallback.
use crate::model_router::{Provider, RouterError, wire, wire_decode};
use darius_cognitive::{AsyncModel, CognitiveError, Message, ModelOutput, ToolSpec, TurnContext};
use std::time::Duration;

/// Single configured provider; the API key is read per call, never stored.
pub struct LiveModel {
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) key_env: String,
    pub(crate) client: reqwest::Client,
}
impl LiveModel {
    /// Validate one provider config; rejects blanks, trims the base URL.
    pub fn for_provider(p: Provider) -> Result<Self, RouterError> {
        let fields = [&p.name, &p.model, &p.base_url, &p.api_key_env];
        if fields.iter().any(|s| s.trim().is_empty()) {
            return Err(RouterError::Provider("provider config incomplete".into()));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build();
        let client = client.map_err(|_| RouterError::Provider("http client init failed".into()))?;
        let base_url = p.base_url.trim_end_matches('/').to_owned();
        Ok(Self {
            model: p.model,
            base_url,
            key_env: p.api_key_env,
            client,
        })
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
        let key = std::env::var(&self.key_env);
        let key = key.map_err(|_| CognitiveError::Loop("authentication failed".into()))?;
        let url = format!("{}/chat/completions", self.base_url);
        let body = wire::encode_request(&self.model, messages, tools);
        let send = self.client.post(&url).bearer_auth(&key).json(&body).send();
        let cancel = ctx.token();
        let resp = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(CognitiveError::Cancelled),
            r = send => r.map_err(|_| CognitiveError::Loop("request failed".into()))?,
            _ = tokio::time::sleep(ctx.deadline_duration()) => {
                return Err(CognitiveError::Loop("deadline exceeded".into()));
            }
        };
        wire_decode::read_response(resp).await
    }
}
