use darius_cognitive::UiEvent;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTask {
    pub id: String,
    pub goal: String,
    pub state: TaskState,
    pub output: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub sequence: usize,
    pub event: UiEvent,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
}
/// Without an injected executor no execution capability is advertised.
pub fn agent_card() -> AgentCard {
    AgentCard {
        name: "darius".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        description: "Local headless goals, task lookup and correlated SSE; mutations denied"
            .into(),
        capabilities: vec![],
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct CreateGoal {
    pub goal: String,
}
