use crate::{A2aTask, AgentCard, CreateGoal, ServerState, TaskState, agent_card};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
pub(crate) async fn submit(
    State(state): State<ServerState>,
    Json(payload): Json<CreateGoal>,
) -> (StatusCode, Json<Value>) {
    if state.executor.is_none() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"execution unavailable: no runtime"})),
        );
    }
    if payload.goal.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"goal must not be blank"})),
        );
    }
    let id = uuid::Uuid::new_v4().to_string();
    let task = A2aTask {
        id: id.clone(),
        goal: payload.goal.clone(),
        state: TaskState::Pending,
        output: None,
    };
    let response = json!(task);
    state.jobs.lock().unwrap().insert(
        id.clone(),
        crate::state::Job {
            task,
            events: vec![],
        },
    );
    tokio::spawn(crate::execution::run(state, id, payload.goal));
    (StatusCode::ACCEPTED, Json(response))
}
pub(crate) async fn task(
    State(state): State<ServerState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match state.jobs.lock().unwrap().get(&id) {
        Some(job) => (StatusCode::OK, Json(json!(job.task))),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"task not found","id":id})),
        ),
    }
}
pub(crate) async fn card(State(state): State<ServerState>) -> Json<AgentCard> {
    let mut card = agent_card();
    if state.executor.is_some() {
        card.capabilities = vec![
            "goal_execution".into(),
            "task_lookup".into(),
            "task_sse".into(),
        ];
    }
    Json(card)
}
