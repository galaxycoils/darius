//! Provider token usage parsing and conservative fallback estimates.
use darius_cognitive::{Message, ModelOutput};
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
pub fn estimate_input(messages: &[Message]) -> u64 {
    u64::try_from(messages.iter().map(message_bytes).sum::<usize>())
        .unwrap_or(u64::MAX)
        .div_ceil(4)
}
fn message_bytes(message: &Message) -> usize {
    match message {
        Message::System { content } | Message::User { content } => content.len(),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            content.as_ref().map_or(0, String::len)
                + tool_calls
                    .iter()
                    .map(|call| call.id.len() + call.name.len() + call.arguments.to_string().len())
                    .sum::<usize>()
        }
        Message::Tool {
            tool_call_id,
            name,
            content,
        } => tool_call_id.len() + name.len() + content.len(),
    }
}
pub fn estimate_output(output: &ModelOutput) -> u64 {
    let message = Message::Assistant {
        content: output.content.clone(),
        tool_calls: output.tool_calls.clone(),
    };
    estimate_input(&[message])
}
