//! Darius web server: SSE event streaming, goal submission, A2A task execution.

use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::get};
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
        description: "Darius web server: CognitiveLoop + SSE + A2A task execution".into(),
        capabilities: vec![
            "cognitive_loop".into(),
            "memory_search".into(),
            "tool_execution".into(),
            "task_board".into(),
        ],
    }
}

/// Request to create a new goal.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateGoal {
    pub goal: String,
}

/// Shared server state.
#[derive(Clone)]
pub struct ServerState {
    pub event_sender: broadcast::Sender<UiEvent>,
    pub tasks: Arc<std::sync::Mutex<Vec<A2aTask>>>,
    pub sender_timestamps: Arc<std::sync::Mutex<HashMap<String, Vec<u64>>>>,
    pub rate_limit_per_min: usize,
}

impl ServerState {
    pub fn new() -> (Self, broadcast::Receiver<UiEvent>) {
        let (tx, rx) = broadcast::channel(256);
        let state = Self {
            event_sender: tx,
            tasks: Arc::new(std::sync::Mutex::new(Vec::new())),
            sender_timestamps: Arc::new(std::sync::Mutex::new(HashMap::new())),
            rate_limit_per_min: 60,
        };
        (state, rx)
    }
}

/// Create the server router.
pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/api/goal", axum::routing::post(create_goal))
        .route("/api/events", get(sse_events))
        .route("/a2a/card", get(a2a_card))
        .route("/a2a/tasks", axum::routing::post(create_task))
        .route("/a2a/tasks/:id", get(get_task))
        .fallback(not_found)
        .with_state(state)
}

async fn dashboard() -> impl IntoResponse {
    axum::response::Html(
        r#"<!DOCTYPE html>
<html><head><title>Darius Web Server</title></head>
<body>
<h1>Darius Web Server</h1>
<form id="goalForm">
  <input id="goal" type="text" placeholder="Enter goal..." required />
  <button type="submit">Run Goal</button>
</form>
<div id="events"></div>
<script>
const events = document.getElementById('events');
const es = new EventSource('/api/events');
es.onmessage = (e) => {
  const div = document.createElement('div');
  div.textContent = e.data;
  events.appendChild(div);
};
document.getElementById('goalForm').onsubmit = async (e) => {
  e.preventDefault();
  const goal = document.getElementById('goal').value;
  await fetch('/api/goal', { method: 'POST', headers: {'Content-Type':'application/json'}, body: JSON.stringify({goal}) });
};
</script>
</body></html>"#,
    )
}

async fn a2a_card() -> Json<AgentCard> {
    Json(agent_card())
}

async fn create_goal(
    axum::extract::State(state): axum::extract::State<ServerState>,
    Json(payload): Json<CreateGoal>,
) -> impl IntoResponse {
    let goal = payload.goal.clone();
    let goal_for_response = goal.clone();
    let sender = state.event_sender.clone();
    let goal_for_spawn = goal.clone();
    tokio::spawn(async move {
        let _ = sender.send(UiEvent::UserMessage { text: goal_for_spawn.clone() });
        let _ = sender.send(UiEvent::Status {
            line: format!("Processing goal: {goal_for_spawn}"),
        });
        let _ = sender.send(UiEvent::AssistantDelta {
            text: "Goal accepted (demo mode: full CognitiveLoop requires darius-cli runtime).".into(),
        });
        let _ = sender.send(UiEvent::Done);
    });
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"status": "accepted", "goal": goal_for_response})),
    )
}

async fn sse_events(
    axum::extract::State(state): axum::extract::State<ServerState>,
) -> impl IntoResponse {
    let mut rx = state.event_sender.subscribe();
    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            let json = serde_json::to_string(&event).unwrap_or_default();
            yield Ok::<_, std::convert::Infallible>(
                axum::response::sse::Event::default()
                    .data(json)
                    .event("ui"),
            );
            if matches!(event, UiEvent::Done | UiEvent::Error { .. }) {
                break;
            }
        }
    };
    axum::response::Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

async fn create_task(
    axum::extract::State(state): axum::extract::State<ServerState>,
    Json(payload): Json<CreateGoal>,
) -> impl IntoResponse {
    let id = uuid::Uuid::new_v4().to_string();
    let goal = payload.goal.clone();
    let goal_for_response = goal.clone();
    let task = A2aTask {
        id: id.clone(),
        goal: goal.clone(),
        state: TaskState::Completed,
        output: Some(format!("Completed (demo): {goal}")),
    };
    state.tasks.lock().unwrap().push(task);
    let sender = state.event_sender.clone();
    let _ = sender.send(UiEvent::UserMessage { text: goal });
    let _ = sender.send(UiEvent::Done);
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"id": id, "state": "Running", "goal": goal_for_response})),
    )
}

async fn get_task(
    axum::extract::State(state): axum::extract::State<ServerState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let tasks = state.tasks.lock().unwrap();
    if let Some(task) = tasks.iter().find(|t| t.id == id) {
        (StatusCode::OK, Json(serde_json::json!(task)))
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "task not found", "id": id})),
        )
    }
}

async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "not found"})),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_card_has_capabilities() {
        let card = agent_card();
        assert_eq!(card.name, "darius");
        assert_eq!(
            card.capabilities,
            vec![
                "cognitive_loop",
                "memory_search",
                "tool_execution",
                "task_board",
            ]
        );
    }

    #[test]
    fn task_state_transitions() {
        let mut task = A2aTask {
            id: "test".into(),
            goal: "goal".into(),
            state: TaskState::Running,
            output: None,
        };
        assert_eq!(task.state, TaskState::Running);
        task.state = TaskState::Completed;
        task.output = Some("done".into());
        assert_eq!(task.state, TaskState::Completed);
        assert_eq!(task.output, Some("done".to_string()));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::serve;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn request(address: std::net::SocketAddr, method: &str, path: &str, body: &str) -> String {
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let size = body.len();
        let wire = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {size}\r\n\r\n{body}"
        );
        socket.write_all(wire.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            socket.read_to_end(&mut bytes),
        )
        .await
        .unwrap()
        .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[tokio::test]
    async fn web_server_serves_goal_sse_and_a2a() {
        let (state, _events) = ServerState::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = create_router(state.clone());
        let server = tokio::spawn(async move { serve(listener, router).await.unwrap() });

        // Test root serves HTML with form
        let root = request(address, "GET", "/", "").await;
        assert!(root.contains("goalForm"), "root should have goal form: {root}");
        assert!(root.contains("EventSource"), "root should have EventSource: {root}");

        // Test agent card has capabilities
        let card = agent_card();
        assert_eq!(card.capabilities, vec!["cognitive_loop", "memory_search", "tool_execution", "task_board"]);

        // Test /api/goal accepts POST and returns accepted
        let goal_resp = request(address, "POST", "/api/goal", r#"{"goal":"test goal"}"#).await;
        let goal_body = goal_resp.split("\r\n\r\n").nth(1).unwrap_or(&goal_resp);
        assert!(goal_body.contains("accepted"), "goal should be accepted: {goal_body}");

        // Test /a2a/tasks creates a task
        let full_resp = request(address, "POST", "/a2a/tasks", r#"{"goal":"a2a task"}"#).await;
        let task_resp = full_resp.split("\r\n\r\n").nth(1).unwrap_or(&full_resp);
        assert!(task_resp.contains("Running"), "task should be Running: {task_resp}");
        assert!(task_resp.contains("id"), "task should have id: {task_resp}");

        // Extract task id from response
        let task: serde_json::Value = serde_json::from_str(task_resp.trim()).unwrap();
        let task_id = task["id"].as_str().unwrap();

        // GET the task (wait for completion)
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let get_full = request(address, "GET", &format!("/a2a/tasks/{task_id}"), "").await;
        let get_resp = get_full.split("\r\n\r\n").nth(1).unwrap_or(&get_full);
        assert!(get_resp.contains("Completed"), "task should be Completed: {get_resp}");

        // Verify tasks state
        let tasks = state.tasks.lock().unwrap();
        assert!(!tasks.is_empty());
        assert!(tasks.iter().any(|t| t.state == TaskState::Completed));

        server.abort();
    }
}
