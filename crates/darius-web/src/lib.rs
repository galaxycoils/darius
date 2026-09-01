//! Darius web dashboard + A2A server.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post},
};
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
        description: "Darius — open-source lean agent harness".into(),
        capabilities: vec![
            "cognitive_loop".into(),
            "memory_search".into(),
            "tool_execution".into(),
            "task_board".into(),
            "peer_a2a".into(),
        ],
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

/// Create the web + A2A router.
pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/api/events", get(sse_handler))
        .route("/api/goal", post(submit_goal))
        .route("/a2a/card", get(a2a_card))
        .route("/a2a/tasks", post(a2a_create_task))
        .route("/a2a/tasks/{id}", get(a2a_get_task))
        .route("/a2a/peer", post(a2a_peer_send))
        .route("/a2a/inbox/{handle}", get(a2a_peer_inbox))
        .with_state(state)
}

/// Dashboard HTML.
async fn dashboard() -> String {
    r#"<!DOCTYPE html>
<html><head><title>Darius</title><style>
:root{--bg:#0c0e12;--surface:#141820;--border:#2a3140;--text:#e8eaef;--muted:#8b93a7;--accent:#e8a54b;--ok:#3dd68c;--warn:#f0c14a;--err:#f07178}
body{background:var(--bg);color:var(--text);font-family:monospace;margin:0;padding:1rem}
header{border-bottom:1px solid var(--border);padding-bottom:1rem;margin-bottom:1rem}
h1{color:var(--accent);margin:0}
.panel{border:1px solid var(--border);background:var(--surface);padding:1rem;margin:1rem 0;border-radius:4px}
.stream{height:300px;overflow-y:auto}
.task{color:var(--ok)}
input{background:var(--surface);border:1px solid var(--border);color:var(--text);padding:0.5rem;width:80%}
button{background:var(--accent);border:none;padding:0.5rem 1rem;cursor:pointer}
</style></head><body>
<header><h1>darius</h1><span style="color:var(--muted)">v1.2.0</span></header>
<div class="panel"><h3>Stream</h3><div class="stream" id="stream"></div></div>
<div class="panel"><h3>Goal</h3>
<input type="text" id="goal" placeholder="Enter goal..."/>
<button onclick="submitGoal()">Run</button></div>
<div class="panel"><h3>Tasks</h3><div id="tasks"></div></div>
<script>
const stream=document.getElementById('stream');
const tasks=document.getElementById('tasks');
const es=new EventSource('/api/events');
es.onmessage=e=>{const ev=JSON.parse(e.data);const d=document.createElement('div');d.textContent=JSON.stringify(ev);stream.appendChild(d);stream.scrollTop=stream.scrollHeight};
function submitGoal(){const g=document.getElementById('goal').value;fetch('/api/goal',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({goal:g})})}
</script></body></html>"#.into()
}

/// SSE handler.
async fn sse_handler(
    State(state): State<ServerState>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.event_sender.subscribe();
    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok(Event::default().data(json));
        }
    };
    Sse::new(stream)
}

/// Submit goal.
#[derive(Deserialize)]
pub struct GoalRequest {
    pub goal: String,
}

async fn submit_goal(
    State(state): State<ServerState>,
    Json(req): Json<GoalRequest>,
) -> Json<serde_json::Value> {
    let _ = state.event_sender.send(UiEvent::Header {
        profile: "default".into(),
        model: "mock".into(),
        goal: req.goal.clone(),
    });
    let _ = state.event_sender.send(UiEvent::Status {
        line: format!("Goal submitted: {}", req.goal),
    });
    let _ = state.event_sender.send(UiEvent::Done);
    Json(serde_json::json!({"status": "ok"}))
}

/// A2A card.
async fn a2a_card() -> Json<AgentCard> {
    Json(agent_card())
}

/// Create A2A task.
async fn a2a_create_task(
    State(state): State<ServerState>,
    Json(req): Json<GoalRequest>,
) -> Json<serde_json::Value> {
    let task_id = uuid::Uuid::new_v4().to_string();
    let task = A2aTask {
        id: task_id.clone(),
        goal: req.goal.clone(),
        state: TaskState::Pending,
        output: None,
    };
    state.tasks.lock().unwrap().push(task.clone());
    let _ = state.event_sender.send(UiEvent::A2aTask {
        task_id: task_id.clone(),
        state: "pending".into(),
    });
    Json(serde_json::json!({"task_id": task_id, "state": "pending"}))
}

/// Get A2A task.
async fn a2a_get_task(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let tasks = state.tasks.lock().unwrap();
    match tasks.iter().find(|t| t.id == id) {
        Some(task) => Json(serde_json::to_value(task).unwrap_or_default()),
        None => Json(serde_json::json!({"error": "not found"})),
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// POST /a2a/peer: Peer A2A messaging with quota enforcement.
async fn a2a_peer_send(
    State(state): State<ServerState>,
    Json(req): Json<PeerMessageRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let now = current_timestamp();

    // Check rate limit (quota per minute)
    {
        let mut timestamps = state.sender_timestamps.lock().unwrap();
        let list = timestamps.entry(req.sender.clone()).or_default();
        // Evict older than 60s
        list.retain(|&t| now.saturating_sub(t) < 60);

        if list.len() >= state.rate_limit_per_min {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": format!("quota exceeded for sender '{}': max {} messages/min", req.sender, state.rate_limit_per_min)
                })),
            );
        }

        list.push(now);
    }

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PeerMessageEnvelope {
        id: msg_id.clone(),
        sender: req.sender,
        recipient_handle: req.recipient_handle,
        intent: req.intent,
        payload: req.payload,
        timestamp: now,
        read: false,
    };

    state.peer_inbox.lock().unwrap().push(envelope);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "delivered",
            "message_id": msg_id
        })),
    )
}

/// GET /a2a/inbox/{handle}: Pull unread messages and mark as read.
async fn a2a_peer_inbox(
    State(state): State<ServerState>,
    Path(handle): Path<String>,
) -> Json<Vec<PeerMessageEnvelope>> {
    let mut inbox = state.peer_inbox.lock().unwrap();
    let mut unread = Vec::new();

    for msg in inbox.iter_mut() {
        if msg.recipient_handle == handle && !msg.read {
            msg.read = true;
            unread.push(msg.clone());
        }
    }

    Json(unread)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_card_returns_darius() {
        let card = agent_card();
        assert_eq!(card.name, "darius");
        assert!(card.capabilities.contains(&"cognitive_loop".into()));
        assert!(card.capabilities.contains(&"peer_a2a".into()));
    }

    #[test]
    fn task_state_serializes() {
        let state = TaskState::Pending;
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("Pending"));
    }

    #[tokio::test]
    async fn peer_messaging_send_and_pull_inbox() {
        let (state, _) = ServerState::new();

        let req = PeerMessageRequest {
            sender: "agent-alice".into(),
            recipient_handle: "agent-bob".into(),
            intent: "code_review".into(),
            payload: serde_json::json!({"pr": 42, "files": ["src/main.rs"]}),
        };

        // 1. Send peer message
        let (status, res) = a2a_peer_send(State(state.clone()), Json(req)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(res["status"], "delivered");

        // 2. Pull inbox for recipient (bob)
        let unread = a2a_peer_inbox(State(state.clone()), Path("agent-bob".into())).await;
        assert_eq!(unread.0.len(), 1);
        assert_eq!(unread.0[0].sender, "agent-alice");
        assert_eq!(unread.0[0].intent, "code_review");

        // 3. Pull inbox again -> now empty because already marked read
        let unread_again = a2a_peer_inbox(State(state.clone()), Path("agent-bob".into())).await;
        assert!(unread_again.0.is_empty());
    }

    #[tokio::test]
    async fn peer_messaging_quota_enforcement() {
        let (state, _) = ServerState::new();
        let state = state.with_rate_limit(2); // limit to 2 per min

        let req = PeerMessageRequest {
            sender: "spammer".into(),
            recipient_handle: "target".into(),
            intent: "ping".into(),
            payload: serde_json::json!({}),
        };

        // Message 1: OK
        let (s1, _) = a2a_peer_send(State(state.clone()), Json(req.clone())).await;
        assert_eq!(s1, StatusCode::OK);

        // Message 2: OK
        let (s2, _) = a2a_peer_send(State(state.clone()), Json(req.clone())).await;
        assert_eq!(s2, StatusCode::OK);

        // Message 3: 429 Too Many Requests
        let (s3, res3) = a2a_peer_send(State(state.clone()), Json(req.clone())).await;
        assert_eq!(s3, StatusCode::TOO_MANY_REQUESTS);
        assert!(res3["error"].as_str().unwrap().contains("quota exceeded"));
    }
}
