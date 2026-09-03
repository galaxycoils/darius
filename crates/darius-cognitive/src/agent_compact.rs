//! Oldest-first, UTF-8-safe tool-result compaction to a byte budget.
use crate::conversation::Message;
const MARKER: &str = "\n[compacted]";

pub fn transcript_chars(msgs: &[Message]) -> usize {
    msgs.iter().map(msg_bytes).sum()
}

fn msg_bytes(msg: &Message) -> usize {
    match msg {
        Message::System { content } | Message::User { content } => content.len(),
        Message::Assistant { content, .. } => content.as_ref().map_or(0, String::len),
        Message::Tool { content, .. } => content.len(),
    }
}

/// Shrink oldest tool results first until the transcript reaches `budget`.
pub fn compact_tool_results(msgs: &mut [Message], budget: usize) {
    let mut excess = transcript_chars(msgs).saturating_sub(budget);
    for msg in msgs {
        if excess == 0 {
            break;
        }
        if let Message::Tool { content, .. } = msg {
            let before = content.len();
            shrink(content, before.saturating_sub(excess));
            excess = excess.saturating_sub(before - content.len());
        }
    }
}

fn shrink(content: &mut String, target: usize) {
    if content.len() <= target {
        return;
    }
    if target < MARKER.len() {
        content.clear();
        return;
    }
    let mut prefix = target - MARKER.len();
    while !content.is_char_boundary(prefix) {
        prefix -= 1;
    }
    content.truncate(prefix);
    content.push_str(MARKER);
}
