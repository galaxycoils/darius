//! Encode domain turns to exact OpenAI-compatible wire DTOs.
use darius_cognitive::{Message, ToolSpec};
use serde_json::{Value, json};

pub fn encode_request(model: &str, msgs: &[Message], tools: &[ToolSpec]) -> Value {
    json!({"model": model,
        "messages": msgs.iter().map(encode_message).collect::<Vec<_>>(),
        "tools": tools.iter().map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}})).collect::<Vec<_>>()})
}

fn encode_message(m: &Message) -> Value {
    match m {
        Message::System { content } => json!({"role": "system", "content": content}),
        Message::User { content } => json!({"role": "user", "content": content}),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let mut v = json!({"role": "assistant", "content": content});
            if !tool_calls.is_empty() {
                v["tool_calls"] = tool_calls.iter().map(|c| json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.arguments.to_string()}})).collect();
            }
            v
        }
        Message::Tool {
            tool_call_id,
            content,
            ..
        } => json!({"role": "tool", "tool_call_id": tool_call_id, "content": content}),
    }
}
