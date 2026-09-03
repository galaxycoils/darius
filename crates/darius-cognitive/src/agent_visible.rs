//! Canonical model-visible message representation used for context sizing.
use crate::Message;
use serde_json::{Value, json};

pub fn transcript_chars(msgs: &[Message]) -> usize {
    serde_json::to_vec(&msgs.iter().map(wire_value).collect::<Vec<_>>())
        .map_or(usize::MAX, |bytes| bytes.len())
}

fn wire_value(msg: &Message) -> Value {
    match msg {
        Message::System { content } => json!({"role": "system", "content": content}),
        Message::User { content } => json!({"role": "user", "content": content}),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let mut value = json!({"role": "assistant", "content": content});
            if !tool_calls.is_empty() {
                value["tool_calls"] = json!(
                    tool_calls
                        .iter()
                        .map(|call| json!({
                            "id": call.id, "type": "function", "function": {
                                "name": call.name, "arguments": call.arguments.to_string()
                            }
                        }))
                        .collect::<Vec<_>>()
                );
            }
            value
        }
        Message::Tool {
            tool_call_id,
            name,
            content,
        } => json!({
            "role": "tool", "tool_call_id": tool_call_id,
            "name": name, "content": content
        }),
    }
}
