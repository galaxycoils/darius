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

pub fn encode_stream_request(
    model: &str,
    msgs: &[Message],
    tools: &[ToolSpec],
    max_tokens: u64,
) -> Value {
    let mut val = encode_request(model, msgs, tools, max_tokens);
    if let Some(map) = val.as_object_mut() {
        map.insert("stream".into(), json!(true));
        map.insert("stream_options".into(), json!({"include_usage": true}));
    }
    val
}
