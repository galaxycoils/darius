//! Unavailable web compatibility surface; no public listener is shipped.

use axum::{Json, Router, http::StatusCode, routing::get};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

use darius_cognitive::UiEvent;

// Re-export canonical UiEvent so downstream users can match the same type.
pub use darius_cognitive::UiEvent as CanonicalUiEvent;

/// A2A task state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
}

/// A2A task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTask {
    pub id: String,
    pub goal: String,
    pub state: TaskState,
    pub output: Option<String>,
}

/// A2A Agent Card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
}

/// Create the Agent Card.
pub fn agent_card() -> AgentCard {
    AgentCard {
        name: "darius".into(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Unavailable: web execution and A2A are not supported; use the CLI".into(),
        capabilities: Vec::new(),
    }
}

/// Peer message envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerMessageEnvelope {
    pub id: String,
    pub sender: String,
    pub recipient_handle: String,
    pub intent: String,
    pub payload: serde_json::Value,
    pub timestamp: u64,
    pub read: bool,
}

/// Request to send a peer message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerMessageRequest {
    pub sender: String,
    pub recipient_handle: String,
    pub intent: String,
    pub payload: serde_json::Value,
}

/// Shared server state.
#[derive(Clone)]
pub struct ServerState {
    pub event_sender: broadcast::Sender<UiEvent>,
    pub tasks: Arc<std::sync::Mutex<Vec<A2aTask>>>,
    pub peer_inbox: Arc<std::sync::Mutex<Vec<PeerMessageEnvelope>>>,
    pub sender_timestamps: Arc<std::sync::Mutex<HashMap<String, Vec<u64>>>>,
    pub rate_limit_per_min: usize,
}

impl ServerState {
    pub fn new() -> (Self, broadcast::Receiver<UiEvent>) {
        let (tx, rx) = broadcast::channel(100);
        let state = Self {
            event_sender: tx,
            tasks: Arc::new(std::sync::Mutex::new(Vec::new())),
            peer_inbox: Arc::new(std::sync::Mutex::new(Vec::new())),
            sender_timestamps: Arc::new(std::sync::Mutex::new(HashMap::new())),
            rate_limit_per_min: 60,
        };
        (state, rx)
    }

    pub fn with_rate_limit(mut self, limit: usize) -> Self {
        self.rate_limit_per_min = limit;
        self
    }
}

/// A compatibility router that never accepts work or advertises execution.
pub fn create_router(_state: ServerState) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/a2a/card", get(a2a_card))
        .fallback(unavailable)
}

async fn dashboard() -> axum::response::Html<&'static str> {
    axum::response::Html(
        "<!doctype html><html><head><title>Darius: unavailable</title></head><body><h1>Web dashboard unavailable</h1><p>Web execution, event streaming and A2A are not supported. Use darius tui or darius run with a configured provider; --offline is a demo.</p></body></html>",
    )
}

async fn a2a_card() -> Json<AgentCard> {
    Json(agent_card())
}

async fn unavailable() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(
            serde_json::json!({"status": "unavailable", "error": "Web execution and A2A are not supported"}),
        ),
    )
}
