//! Provider token usage parsing and conservative request estimates.
use darius_cognitive::{Message, ToolSpec, model_request_chars};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct ProviderUsage {
    pub total_tokens: u64,
}

pub fn parse(body: &Value) -> Option<ProviderUsage> {
    let usage = body.get("usage")?;
    if let Some(total_tokens) = usage.get("total_tokens").and_then(Value::as_u64) {
        return Some(ProviderUsage { total_tokens });
    }
    let input = token(usage, "prompt_tokens", "input_tokens")?;
    let output = token(usage, "completion_tokens", "output_tokens")?;
    Some(ProviderUsage {
        total_tokens: input.saturating_add(output),
    })
}

fn token(usage: &Value, primary: &str, alternate: &str) -> Option<u64> {
    usage
        .get(primary)
        .or_else(|| usage.get(alternate))
        .and_then(Value::as_u64)
}

pub fn estimate_input(messages: &[Message], tools: &[ToolSpec]) -> u64 {
    u64::try_from(model_request_chars(messages, tools))
        .unwrap_or(u64::MAX)
        .div_ceil(4)
}
