//! LiveModel owns one validated provider; exact cancellable protocol, no fallback.
use crate::model_router::{BudgetEnforcer, BudgetScope, wire, wire_decode};
use darius_cognitive::{AsyncModel, CognitiveError, Message, ModelOutput, ToolSpec, TurnContext};

/// Single configured provider; the API key is read per call, never stored.
pub struct LiveModel {
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) key_env: String,
    pub(crate) client: reqwest::Client,
    pub(crate) budget: BudgetEnforcer,
    pub(crate) scope: BudgetScope,
}

#[async_trait::async_trait]
impl AsyncModel for LiveModel {
    async fn complete(
        &mut self,
        messages: &[Message],
        tools: &[ToolSpec],
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        let key = std::env::var(&self.key_env)
            .map_err(|_| CognitiveError::Loop("authentication failed".into()))?;
        let url = format!("{}/chat/completions", self.base_url);
        let body = wire::encode_request(&self.model, messages, tools);
        // One guarded phase: send headers AND read the body under the
        // same cancel/deadline select, so a slow body cannot outlive the turn.
        let fetch = async {
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&key)
                .json(&body)
                .send()
                .await
                .map_err(|_| CognitiveError::Loop("request failed".into()))?;
            wire_decode::read_response(resp).await
        };
        let cancel = ctx.token();
        let sleep = tokio::time::sleep(ctx.deadline_duration());
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CognitiveError::Cancelled),
            out = fetch => out,
            _ = sleep => Err(CognitiveError::Loop("deadline exceeded".into())),
        }
    }
}
