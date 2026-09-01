use serde::{Deserialize, Serialize};

/// A chat message representing a turn in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new("system", content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new("user", content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new("assistant", content)
    }
}

/// Options controlling lean-tail context compression.
#[derive(Debug, Clone)]
pub struct CompressOpts {
    pub max_chars: usize,  // Maximum allowed characters across all messages
    pub tail_chars: usize, // Characters reserved for recent tail messages
    pub head_chars: usize, // Characters reserved for system/goal head messages
}

impl Default for CompressOpts {
    fn default() -> Self {
        Self {
            max_chars: 48_000,
            tail_chars: 12_000,
            head_chars: 4_000,
        }
    }
}

/// Calculate the total character length of all message contents.
pub fn total_chars(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|m| m.content.len()).sum()
}

/// Compress a transcript using lean-tail strategy.
///
/// Keeps head messages (system prompt, goal) and recent tail messages intact,
/// replacing the middle with a structured compression marker when the total
/// character count exceeds `opts.max_chars`.
pub fn lean_tail_compress(messages: &[ChatMessage], opts: CompressOpts) -> Vec<ChatMessage> {
    if messages.is_empty() {
        return Vec::new();
    }

    let total = total_chars(messages);
    if total <= opts.max_chars {
        return messages.to_vec();
    }

    // 1. Select head messages up to head_chars (at least 1 message).
    let mut head_count = 0;
    let mut current_head_chars = 0;
    for msg in messages {
        if head_count > 0 && current_head_chars + msg.content.len() > opts.head_chars {
            break;
        }
        current_head_chars += msg.content.len();
        head_count += 1;
    }

    // 2. Select tail messages up to tail_chars (from end backwards, at least 1 message).
    let mut tail_start = messages.len();
    let mut current_tail_chars = 0;
    for (idx, msg) in messages.iter().enumerate().rev() {
        if tail_start < messages.len() && current_tail_chars + msg.content.len() > opts.tail_chars {
            break;
        }
        current_tail_chars += msg.content.len();
        tail_start = idx;
    }

    // 3. Prevent overlap between head and tail
    if head_count >= tail_start {
        // If they overlap or meet, keep messages up to max_chars from the end
        let mut result = Vec::new();
        let mut budget = opts.max_chars;
        // Keep the first message (system/head)
        if !messages.is_empty() {
            let first = &messages[0];
            budget = budget.saturating_sub(first.content.len());
            result.push(first.clone());
        }
        // Then fit as many from the tail as possible
        let mut tail_items = Vec::new();
        for msg in messages.iter().skip(1).rev() {
            if msg.content.len() <= budget {
                budget -= msg.content.len();
                tail_items.push(msg.clone());
            } else {
                break;
            }
        }
        tail_items.reverse();
        result.extend(tail_items);
        return result;
    }

    let dropped_count = tail_start - head_count;
    let mut result = Vec::with_capacity(head_count + 1 + (messages.len() - tail_start));

    // Append head
    result.extend_from_slice(&messages[..head_count]);

    // Append middle compression marker
    if dropped_count > 0 {
        let marker = ChatMessage {
            role: "system".into(),
            content: format!(
                "[... {} messages compressed and omitted for context budget ...]",
                dropped_count
            ),
        };
        result.push(marker);
    }

    // Append tail
    result.extend_from_slice(&messages[tail_start..]);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_n_messages(n: usize, char_len: usize) -> Vec<ChatMessage> {
        (0..n)
            .map(|i| {
                let role = if i == 0 {
                    "system"
                } else if i % 2 == 1 {
                    "user"
                } else {
                    "assistant"
                };
                let content = format!("msg_{i:04}: {}", "x".repeat(char_len.saturating_sub(10)));
                ChatMessage::new(role, content)
            })
            .collect()
    }

    #[test]
    fn lean_tail_keeps_head_and_tail_drops_middle() {
        let msgs = make_n_messages(100, 500); // 100 msgs ~500 chars each = ~50,000 chars
        let out = lean_tail_compress(
            &msgs,
            CompressOpts {
                max_chars: 20_000,
                tail_chars: 8_000,
                head_chars: 2_000,
            },
        );
        assert!(total_chars(&out) <= 20_000 + 500); // slack
        assert_eq!(
            out.first().map(|m| m.role.as_str()),
            msgs.first().map(|m| m.role.as_str())
        );
        assert_eq!(
            out.last().map(|m| m.content.as_str()),
            msgs.last().map(|m| m.content.as_str())
        );
        // Ensure middle was compressed with marker
        assert!(
            out.iter()
                .any(|m| m.content.contains("omitted for context budget"))
        );
    }

    #[test]
    fn lean_tail_under_budget_is_noop() {
        let msgs = make_n_messages(5, 100);
        let out = lean_tail_compress(
            &msgs,
            CompressOpts {
                max_chars: 20_000,
                tail_chars: 8_000,
                head_chars: 2_000,
            },
        );
        assert_eq!(out, msgs);
    }

    #[test]
    fn lean_tail_empty_messages() {
        let out = lean_tail_compress(&[], CompressOpts::default());
        assert!(out.is_empty());
    }
}
