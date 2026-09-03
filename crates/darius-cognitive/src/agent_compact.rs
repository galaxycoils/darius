//! Oldest-first, UTF-8-safe tool-result compaction.
pub use crate::agent_visible::transcript_chars;
use crate::{CognitiveError, Message};
const MARKER: &str = "\n[compacted]";

/// Shrink oldest tool results first and reject irreducible over-budget input.
pub fn compact_tool_results(msgs: &mut [Message], budget: usize) -> Result<(), CognitiveError> {
    for index in 0..msgs.len() {
        while transcript_chars(msgs) > budget {
            let excess = transcript_chars(msgs) - budget;
            let Message::Tool { content, .. } = &mut msgs[index] else {
                break;
            };
            if content.is_empty() {
                break;
            }
            let before = content.len();
            shrink(content, before.saturating_sub(excess));
            if content.len() == before {
                content.clear();
            }
        }
    }
    let required = transcript_chars(msgs);
    if required > budget {
        return Err(CognitiveError::ContextBudgetExceeded { required, budget });
    }
    Ok(())
}

fn shrink(content: &mut String, target: usize) {
    if content.len() <= target {
        return;
    }
    if target < MARKER.len() {
        content.clear();
        return;
    }
    let mut end = target - MARKER.len();
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content.truncate(end);
    content.push_str(MARKER);
}
