//! Encode domain turns to exact OpenAI-compatible wire DTOs.
use darius_cognitive::{Message, ToolSpec, wire_messages, wire_tools};
use serde_json::{Value, json};

pub fn encode_request(model: &str, msgs: &[Message], tools: &[ToolSpec], max_tokens: u64) -> Value {
    json!({
        "model": model,
        "messages": wire_messages(msgs),
        "tools": wire_tools(tools),
        "max_tokens": max_tokens,
    })
}
