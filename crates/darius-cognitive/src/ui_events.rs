use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;

/// UiEvent bus — sole producer of agent progress events.
/// Consumers: TUI, Web (SSE), A2A task status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    Header {
        profile: String,
        model: String,
        goal: String,
    },
    UserMessage {
        text: String,
    },
    AssistantDelta {
        text: String,
    },
    Thinking {
        text: String,
        elapsed_ms: u64,
    },
    ToolStart {
        id: String,
        name: String,
        args_preview: String,
    },
    ToolEnd {
        id: String,
        ok: bool,
        preview: String,
        spilled: Option<String>,
    },
    Diff {
        file: String,
        summary: String,
        lines: Vec<DiffLine>,
    },
    TaskBoard(Vec<TaskSnapshot>),
    PermissionRequired {
        id: String,
        title: String,
        command: String,
        reason: String,
    },
    PermissionResolved {
        id: String,
        choice: PermissionChoice,
    },
    Accept {
        passed: bool,
        notes: String,
    },
    Status {
        line: String,
    },
    A2aTask {
        task_id: String,
        state: String,
    },
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSnapshot {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Context,
    Add,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChoice {
    AllowOnce,
    AllowSession,
    Deny,
}

/// Sink for live UiEvent progress. Implementations forward events to
/// the TUI, a web SSE stream, or a test collector.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: UiEvent);
}

/// Channel-backed EventSink — sends events through a std mpsc channel.
pub struct ChannelEventSink {
    sender: Sender<UiEvent>,
}

impl ChannelEventSink {
    pub fn new(sender: Sender<UiEvent>) -> Self {
        Self { sender }
    }
}

impl EventSink for ChannelEventSink {
    fn emit(&self, event: UiEvent) {
        let _ = self.sender.send(event);
    }
}
