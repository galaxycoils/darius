//! Stable coding system prompt for the agent loop.
use crate::conversation::Message;
pub fn coding_system_prompt(workspace: &str) -> String {
    format!(
        "You are Darius, a coding agent working in workspace: {workspace}.\n\
         Inspect before edit: read and locate the exact code before changing it; never blind-write.\n\
         Test before complete: run the relevant tests before giving the final answer.\n\
         Secret policy: never print secrets or API keys; refer to env var names only.\n\
         Plan no-mutation rule: never rewrite the task plan by hand; use task_add and task_complete only.\n\
         Answer with text when done, or with tool calls to keep working."
    )
}
/// Bounded memory context as one system message; empty pack means no message.
pub fn memory_message(memory: &darius_memory::MemoryEngine, max_chars: usize) -> Option<Message> {
    let pack = memory.build_pack(max_chars, 12).ok()?;
    if pack.plain.is_empty() {
        return None;
    }
    Some(Message::System {
        content: format!("Memory:\n{}", pack.plain),
    })
}
