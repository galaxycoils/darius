//! Message wire values shared by preflight sizing and provider encoding.
use crate::Message;
use serde_json::{Value, json};

pub(super) fn wire_message(msg: &Message) -> Value {
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
            "role": "tool", "tool_call_id": tool_call_id, "name": name, "content": content
        }),
    }
}
