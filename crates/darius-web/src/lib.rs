//! HTTP execution transport; the CLI injects the policy-aware session executor.
mod dashboard;
mod execution;
mod handlers;
mod state;
mod stream;
mod types;
use axum::{
    Router,
    routing::{get, post},
};
pub use darius_cognitive::EventSink as CanonicalEventSink;
pub use darius_cognitive::UiEvent as CanonicalUiEvent;
pub use state::{GoalExecutor, ServerState};
pub use types::{A2aTask, AgentCard, CreateGoal, TaskEvent, TaskState, agent_card};
pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(dashboard::dashboard))
        .route("/api/goal", post(handlers::submit))
        .route("/api/events", get(stream::events))
        .route("/a2a/card", get(handlers::card))
        .route("/a2a/tasks", post(handlers::submit))
        .route("/a2a/tasks/:id", get(handlers::task))
        .with_state(state)
}
