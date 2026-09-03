//! Oldest-first tool-result compaction for serialized context bounds.
mod shrink;
pub use crate::agent_visible::{model_request_chars, transcript_chars, wire_messages, wire_tools};
use crate::{CognitiveError, Message, ToolSpec};
use shrink::shrink;

/// Shrink messages to a serialized JSON character budget.
pub fn compact_tool_results(msgs: &mut [Message], budget: usize) -> Result<(), CognitiveError> {
    compact_to(msgs, budget, transcript_chars)
}

/// Bound the complete model-visible messages + tool schemas payload.
pub fn compact_model_request(
    msgs: &mut [Message],
    tools: &[ToolSpec],
    budget: usize,
) -> Result<(), CognitiveError> {
    compact_to(msgs, budget, |view| model_request_chars(view, tools))
}

fn compact_to(
    msgs: &mut [Message],
    budget: usize,
    size: impl Fn(&[Message]) -> usize,
) -> Result<(), CognitiveError> {
    for index in 0..msgs.len() {
        loop {
            let required = size(msgs);
            if required <= budget {
                return Ok(());
            }
            let Message::Tool { content, .. } = &mut msgs[index] else {
                break;
            };
            if content.is_empty() {
                break;
            }
            let before = content.len();
            shrink(content, before.saturating_sub(required - budget));
            if content.len() == before {
                content.clear();
            }
        }
    }
    let required = size(msgs);
    (required <= budget)
        .then_some(())
        .ok_or(CognitiveError::ContextBudgetExceeded { required, budget })
}
