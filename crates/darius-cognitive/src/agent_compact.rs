//! Oldest-first tool-result compaction at a bounded character budget.
use crate::conversation::Message;
const KEEP: usize = 256;
pub fn transcript_chars(msgs: &[Message]) -> usize {
    msgs.iter().map(msg_chars).sum()
}
fn msg_chars(msg: &Message) -> usize {
    match msg {
        Message::System { content } | Message::User { content } => content.len(),
        Message::Assistant { content, .. } => content.as_ref().map_or(0, |s| s.len()),
        Message::Tool { content, .. } => content.len(),
    }
}
/// Truncate the oldest tool results first until under budget; newest kept.
pub fn compact_tool_results(msgs: &mut [Message], budget: usize) {
    let mut pending: Vec<usize> = msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m, Message::Tool { .. }))
        .map(|(i, _)| i)
        .collect();
    while transcript_chars(msgs) > budget {
        let Some(idx) = pending.first().copied() else {
            break;
        };
        pending.remove(0);
        if let Message::Tool {
            content,
            tool_call_id,
            ..
        } = &mut msgs[idx]
        {
            if content.len() <= KEEP {
                continue;
            }
            let before = content.len();
            content.truncate(KEEP);
            content.push_str(&format!(
                "\n[compacted {before}->{KEEP} chars for {tool_call_id}]"
            ));
        }
    }
}
