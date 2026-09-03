//! OpenAI-compatible values used for model-visible context sizing.
mod values;
use crate::{Message, ToolSpec};
use serde_json::{Value, json};
use values::wire_message;

/// Serialized JSON character count for messages only.
pub fn transcript_chars(msgs: &[Message]) -> usize {
    serialized_chars(&json!(wire_messages(msgs)))
}

/// Serialized character count for the full model-visible request context.
pub fn model_request_chars(msgs: &[Message], tools: &[ToolSpec]) -> usize {
    serialized_chars(&json!({"messages": wire_messages(msgs), "tools": wire_tools(tools)}))
}

pub fn wire_messages(msgs: &[Message]) -> Vec<Value> {
    msgs.iter().map(wire_message).collect()
}

pub fn wire_tools(tools: &[ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({"type": "function", "function": {
                "name": tool.name, "description": tool.description, "parameters": tool.parameters
            }})
        })
        .collect()
}

fn serialized_chars(value: &Value) -> usize {
    serde_json::to_string(value).map_or(usize::MAX, |text| text.chars().count())
}
