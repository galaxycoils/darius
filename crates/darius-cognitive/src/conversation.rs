//! Correlated conversation messages with tool-call/result validation.
use crate::CognitiveError;

mod validate;

/// Role-tagged message; assistant calls correlate to tool results by id.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<darius_tools::ToolCall>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
    },
}

/// Validated transcript: no empty/orphan/duplicate ids; every call resolved.
#[derive(Clone, Debug)]
pub struct Conversation {
    messages: Vec<Message>,
}

impl Conversation {
    pub fn from_messages(messages: Vec<Message>) -> Result<Self, CognitiveError> {
        validate::validate(&messages)?;
        Ok(Self { messages })
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn compact(&mut self, budget: usize) -> Result<(), CognitiveError> {
        crate::agent_compact::compact_tool_results(&mut self.messages, budget)
    }

    pub fn push(&mut self, msg: Message) -> Result<(), CognitiveError> {
        self.messages.push(msg);
        validate::validate(&self.messages)?;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }
}
