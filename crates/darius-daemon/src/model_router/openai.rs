//! LiveModel owns one validated provider; exact cancellable protocol, no fallback.
use crate::model_router::{BudgetEnforcer, BudgetScope, usage, wire, wire_decode};
use darius_cognitive::{AsyncModel, CognitiveError, Message, ModelOutput, ToolSpec, TurnContext};
const MAX_OUTPUT_TOKENS: u64 = 4096;

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
        let is_local = self.base_url.contains("localhost")
            || self.base_url.contains("127.0.0.1")
            || self.base_url.contains("0.0.0.0")
            || self.key_env.eq_ignore_ascii_case("NONE");
        let key = match std::env::var(&self.key_env) {
            Ok(k) if !k.trim().is_empty() => k,
            _ if is_local => "ollama".to_string(),
            _ => return Err(CognitiveError::Loop("authentication failed".into())),
        };
        let input = usage::estimate_input(messages, tools).max(1);
        let mut reservation = self
            .budget
            .reserve(self.scope, input, MAX_OUTPUT_TOKENS)
            .map_err(|error| CognitiveError::Loop(error.to_string()))?;
        let body = wire::encode_request(&self.model, messages, tools, reservation.output_limit());
        let url = format!("{}/chat/completions", self.base_url);
        let fetch = async {
            reservation.mark_dispatched();
            let response = self
                .client
                .post(url)
                .bearer_auth(key)
                .json(&body)
                .send()
                .await
                .map_err(|_| CognitiveError::Loop("request failed".into()))?;
            wire_decode::read_response(response).await
        };
        let cancel = ctx.token();
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CognitiveError::Cancelled),
            output = fetch => output,
            _ = tokio::time::sleep(ctx.deadline_duration()) => {
                Err(CognitiveError::Loop("deadline exceeded".into()))
            },
        };
        if let Ok((_, Some(reported))) = &result {
            reservation.reconcile(reported.total_tokens);
        } else if reservation.was_dispatched() {
            reservation.conservative_charge();
        }
        result.map(|(output, _)| output)
    }
}
