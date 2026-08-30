//! Kanban board — claim/reclaim/promote with failure circuit breaker.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KanbanError {
    #[error("task not found: {0}")]
    NotFound(String),
    #[error("task already exists: {0}")]
    AlreadyExists(String),
    #[error("task is claimed by another agent: {0}")]
    AlreadyClaimed(String),
}

/// Task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    InProgress,
    Done,
    Blocked,
}

/// A kanban task.
#[derive(Debug, Clone)]
pub struct KanbanTask {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<u64>,
    pub failure_count: u32,
}

/// Kanban board.
pub struct KanbanBoard {
    tasks: Arc<Mutex<HashMap<String, KanbanTask>>>,
    max_failures: u32,
    /// Stale claim threshold in seconds.
    stale_threshold: u64,
}

impl KanbanBoard {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            max_failures: 3,
            stale_threshold: 3600, // 1 hour
        }
    }

    /// Set max failures before auto-block.
    pub fn with_max_failures(mut self, max: u32) -> Self {
        self.max_failures = max;
        self
    }

    /// Set stale claim threshold.
    pub fn with_stale_threshold(mut self, threshold_secs: u64) -> Self {
        self.stale_threshold = threshold_secs;
        self
    }

    /// Add a task to the board.
    pub fn add_task(&self, task: KanbanTask) -> Result<(), KanbanError> {
        let mut tasks = self.tasks.lock();
        if tasks.contains_key(&task.id) {
            return Err(KanbanError::AlreadyExists(task.id.clone()));
        }
        tasks.insert(task.id.clone(), task);
        Ok(())
    }

    /// Get a task by ID.
    pub fn get_task(&self, id: &str) -> Option<KanbanTask> {
        self.tasks.lock().get(id).cloned()
    }

    /// List all tasks.
    pub fn list_tasks(&self) -> Vec<KanbanTask> {
        self.tasks.lock().values().cloned().collect()
    }

    /// List tasks by status.
    pub fn list_by_status(&self, status: TaskStatus) -> Vec<KanbanTask> {
        self.tasks
            .lock()
            .values()
            .filter(|t| t.status == status)
            .cloned()
            .collect()
    }

    /// Claim a task for execution.
    pub fn claim(&self, id: &str, agent: &str) -> Result<(), KanbanError> {
        let mut tasks = self.tasks.lock();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| KanbanError::NotFound(id.to_string()))?;

        if task.status != TaskStatus::Ready {
            return Err(KanbanError::AlreadyClaimed(id.to_string()));
        }

        task.status = TaskStatus::InProgress;
        task.claimed_by = Some(agent.to_string());
        task.claimed_at = Some(current_timestamp());
        Ok(())
    }

    /// Release a task back to ready.
    pub fn release(&self, id: &str) -> Result<(), KanbanError> {
        let mut tasks = self.tasks.lock();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| KanbanError::NotFound(id.to_string()))?;

        task.status = TaskStatus::Ready;
        task.claimed_by = None;
        task.claimed_at = None;
        Ok(())
    }

    /// Promote a task to done.
    pub fn promote(&self, id: &str) -> Result<(), KanbanError> {
        let mut tasks = self.tasks.lock();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| KanbanError::NotFound(id.to_string()))?;

        task.status = TaskStatus::Done;
        task.claimed_by = None;
        task.claimed_at = None;
        task.failure_count = 0;
        Ok(())
    }

    /// Record a failure for a task.
    pub fn record_failure(&self, id: &str) -> Result<(), KanbanError> {
        let mut tasks = self.tasks.lock();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| KanbanError::NotFound(id.to_string()))?;

        task.failure_count += 1;
        if task.failure_count >= self.max_failures {
            task.status = TaskStatus::Blocked;
        }
        Ok(())
    }

    /// Reclaim a stale claim (claim that's been held too long).
    pub fn reclaim_stale(&self, id: &str, agent: &str) -> Result<bool, KanbanError> {
        let mut tasks = self.tasks.lock();
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| KanbanError::NotFound(id.to_string()))?;

        if task.status != TaskStatus::InProgress {
            return Ok(false);
        }

        let is_stale = task
            .claimed_at
            .map(|t| current_timestamp() - t >= self.stale_threshold)
            .unwrap_or(false);

        if is_stale {
            task.claimed_by = Some(agent.to_string());
            task.claimed_at = Some(current_timestamp());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Check if a task is blocked.
    pub fn is_blocked(&self, id: &str) -> bool {
        self.tasks
            .lock()
            .get(id)
            .map(|t| t.status == TaskStatus::Blocked)
            .unwrap_or(false)
    }
}

impl Default for KanbanBoard {
    fn default() -> Self {
        Self::new()
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
    fn add_and_get_task() {
        let board = KanbanBoard::new();
        let task = KanbanTask {
            id: "t1".into(),
            title: "Test task".into(),
            status: TaskStatus::Ready,
            claimed_by: None,
            claimed_at: None,
            failure_count: 0,
        };

        board.add_task(task).unwrap();
        let fetched = board.get_task("t1").unwrap();
        assert_eq!(fetched.title, "Test task");
    }

    #[test]
    fn claim_and_promote() {
        let board = KanbanBoard::new();
        let task = KanbanTask {
            id: "t1".into(),
            title: "Test".into(),
            status: TaskStatus::Ready,
            claimed_by: None,
            claimed_at: None,
            failure_count: 0,
        };

        board.add_task(task).unwrap();
        board.claim("t1", "agent1").unwrap();

        let t = board.get_task("t1").unwrap();
        assert_eq!(t.status, TaskStatus::InProgress);
        assert_eq!(t.claimed_by, Some("agent1".to_string()));

        board.promote("t1").unwrap();
        let t = board.get_task("t1").unwrap();
        assert_eq!(t.status, TaskStatus::Done);
    }

    #[test]
    fn reclaim_stale_claim() {
        let board = KanbanBoard::new().with_stale_threshold(0); // immediate stale

        let task = KanbanTask {
            id: "t1".into(),
            title: "Test".into(),
            status: TaskStatus::Ready,
            claimed_by: None,
            claimed_at: None,
            failure_count: 0,
        };

        board.add_task(task).unwrap();
        board.claim("t1", "agent1").unwrap();

        // Reclaim by agent2 (should succeed because threshold is 0).
        let reclaimed = board.reclaim_stale("t1", "agent2").unwrap();
        assert!(reclaimed);

        let t = board.get_task("t1").unwrap();
        assert_eq!(t.claimed_by, Some("agent2".to_string()));
    }

    #[test]
    fn failure_circuit_breaker() {
        let board = KanbanBoard::new().with_max_failures(2);

        let task = KanbanTask {
            id: "t1".into(),
            title: "Test".into(),
            status: TaskStatus::Ready,
            claimed_by: None,
            claimed_at: None,
            failure_count: 0,
        };

        board.add_task(task).unwrap();
        board.claim("t1", "agent1").unwrap();

        // Record failures.
        board.record_failure("t1").unwrap();
        assert!(!board.is_blocked("t1"));

        board.record_failure("t1").unwrap();
        assert!(board.is_blocked("t1"));
    }
}
