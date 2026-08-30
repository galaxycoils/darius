//! A2A server — Agent Card discovery and stateful task lifecycle.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// A2A Agent Card — describes this agent's capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub version: String,
    pub description: String,
    pub url: Option<String>,
    pub capabilities: Vec<String>,
}

impl AgentCard {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            url: None,
            capabilities: vec![],
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn serve(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// A2A task state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A2A stateful task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub session_id: String,
    pub state: TaskState,
    pub input: String,
    pub output: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// A2A server — serves Agent Cards and manages stateful tasks.
pub struct A2aServer {
    agent_card: AgentCard,
    tasks: Arc<Mutex<HashMap<String, Task>>>,
}

impl A2aServer {
    pub fn new(agent_card: AgentCard) -> Self {
        Self {
            agent_card,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Serve the Agent Card.
    pub fn serve_card(&self) -> String {
        self.agent_card.serve()
    }

    /// Get the Agent Card.
    pub fn agent_card(&self) -> &AgentCard {
        &self.agent_card
    }

    /// Create a new task.
    pub fn create_task(&self, session_id: impl Into<String>, input: impl Into<String>) -> Task {
        let id = uuid::Uuid::new_v4().to_string();
        let ts = current_timestamp();
        let task = Task {
            id: id.clone(),
            session_id: session_id.into(),
            state: TaskState::Pending,
            input: input.into(),
            output: None,
            created_at: ts,
            updated_at: ts,
        };
        self.tasks.lock().insert(id.clone(), task.clone());
        task
    }

    /// Get a task by ID.
    pub fn get_task(&self, id: &str) -> Option<Task> {
        self.tasks.lock().get(id).cloned()
    }

    /// Update task state.
    pub fn update_task(
        &self,
        id: &str,
        state: TaskState,
        output: Option<String>,
    ) -> Result<(), String> {
        let mut tasks = self.tasks.lock();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| format!("task {id} not found"))?;
        task.state = state;
        task.output = output;
        task.updated_at = current_timestamp();
        Ok(())
    }

    /// List all tasks for a session.
    pub fn list_tasks(&self, session_id: &str) -> Vec<Task> {
        self.tasks
            .lock()
            .values()
            .filter(|t| t.session_id == session_id)
            .cloned()
            .collect()
    }

    /// List all tasks.
    pub fn list_all_tasks(&self) -> Vec<Task> {
        self.tasks.lock().values().cloned().collect()
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_card_serve_json() {
        let card = AgentCard::new("darius", "0.1.0", "agent harness")
            .with_url("http://localhost:8080")
            .with_capabilities(vec!["rlm".to_string(), "hashline".to_string()]);
        let json = card.serve();
        assert!(json.contains("darius"));
        assert!(json.contains("0.1.0"));
    }

    #[test]
    fn create_and_get_task() {
        let server = A2aServer::new(AgentCard::new("test", "0.1.0", "test"));
        let task = server.create_task("sess1", "hello world");
        assert_eq!(task.state, TaskState::Pending);

        let fetched = server.get_task(&task.id).unwrap();
        assert_eq!(fetched.id, task.id);
        assert_eq!(fetched.input, "hello world");
    }

    #[test]
    fn update_task_state() {
        let server = A2aServer::new(AgentCard::new("test", "0.1.0", "test"));
        let task = server.create_task("sess1", "input");

        server
            .update_task(&task.id, TaskState::Running, None)
            .unwrap();
        let t = server.get_task(&task.id).unwrap();
        assert_eq!(t.state, TaskState::Running);

        server
            .update_task(&task.id, TaskState::Completed, Some("output".to_string()))
            .unwrap();
        let t = server.get_task(&task.id).unwrap();
        assert_eq!(t.state, TaskState::Completed);
        assert_eq!(t.output, Some("output".to_string()));
    }

    #[test]
    fn list_tasks_by_session() {
        let server = A2aServer::new(AgentCard::new("test", "0.1.0", "test"));
        server.create_task("sess1", "a");
        server.create_task("sess1", "b");
        server.create_task("sess2", "c");

        let s1_tasks = server.list_tasks("sess1");
        assert_eq!(s1_tasks.len(), 2);
        let s2_tasks = server.list_tasks("sess2");
        assert_eq!(s2_tasks.len(), 1);
    }
}
