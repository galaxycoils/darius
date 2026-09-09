use crate::{A2aTask, TaskEvent};
use darius_cognitive::EventSink;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
/// Blocking callback: transport owns scheduling; CLI owns runtime and safety.
pub type GoalExecutor =
    Arc<dyn Fn(String, Arc<dyn EventSink>) -> Result<String, String> + Send + Sync>;
pub(crate) struct Job {
    pub task: A2aTask,
    pub events: Vec<TaskEvent>,
}
#[derive(Clone)]
pub struct ServerState {
    pub event_sender: broadcast::Sender<TaskEvent>,
    pub(crate) jobs: Arc<Mutex<HashMap<String, Job>>>,
    pub(crate) executor: Option<GoalExecutor>,
}
impl ServerState {
    pub fn new() -> (Self, broadcast::Receiver<TaskEvent>) {
        let (event_sender, rx) = broadcast::channel(256);
        (
            Self {
                event_sender,
                jobs: Arc::default(),
                executor: None,
            },
            rx,
        )
    }
    pub fn with_executor(executor: GoalExecutor) -> Self {
        let (mut state, _) = Self::new();
        state.executor = Some(executor);
        state
    }
}
