//! Messaging utilities — shared types and helpers for platform adapters.
//!
//! Platform-specific adapters live in `platform_adapters.rs`. This module
//! provides shared error types and utility functions.

use crate::a2a::TaskState;
use thiserror::Error;

/// Map an A2A task state to a human-readable status string.
pub fn task_state_status(state: &TaskState) -> &'static str {
    match state {
        TaskState::Pending => "pending",
        TaskState::Running => "running",
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
        TaskState::Cancelled => "cancelled",
    }
}

/// A message received from a messaging platform.
#[derive(Debug, Clone)]
pub struct MessagingMessage {
    pub chat_id: String,
    pub text: String,
    pub message_id: u64,
}

#[derive(Debug, Error)]
pub enum MessagingError {
    #[error("not connected")]
    NotConnected,
    #[error("platform error: {0}")]
    Platform(String),
    #[error("task error: {0}")]
    Task(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_state_status_mapping() {
        use crate::a2a::TaskState;
        assert_eq!(task_state_status(&TaskState::Pending), "pending");
        assert_eq!(task_state_status(&TaskState::Running), "running");
        assert_eq!(task_state_status(&TaskState::Completed), "completed");
        assert_eq!(task_state_status(&TaskState::Failed), "failed");
        assert_eq!(task_state_status(&TaskState::Cancelled), "cancelled");
    }

    #[test]
    fn messaging_message_debug() {
        let msg = MessagingMessage {
            chat_id: "chat1".into(),
            text: "hello".into(),
            message_id: 42,
        };
        let debug = format!("{msg:?}");
        assert!(debug.contains("chat1"));
        assert!(debug.contains("hello"));
    }

    #[test]
    fn messaging_error_display() {
        let err = MessagingError::NotConnected;
        assert_eq!(err.to_string(), "not connected");
        let err = MessagingError::Platform("timeout".into());
        assert_eq!(err.to_string(), "platform error: timeout");
    }
}
