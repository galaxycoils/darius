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
}
