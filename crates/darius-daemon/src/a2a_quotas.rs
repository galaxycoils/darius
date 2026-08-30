//! A2A Quotas & Backpressure — concurrency limits and queue policies aligned with Agent Cards.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QuotaError {
    #[error("quota exceeded: {0}")]
    Exceeded(String),
    #[error("agent not found: {0}")]
    AgentNotFound(String),
}

/// Quota configuration for an agent.
#[derive(Debug, Clone)]
pub struct AgentQuota {
    pub agent_id: String,
    pub max_concurrent_tasks: usize,
    pub max_queue_depth: usize,
    pub rate_limit_per_minute: u64,
}

impl Default for AgentQuota {
    fn default() -> Self {
        Self {
            agent_id: String::new(),
            max_concurrent_tasks: 5,
            max_queue_depth: 100,
            rate_limit_per_minute: 60,
        }
    }
}

/// Task queue policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePolicy {
    Drop,
    Block,
    DropOldest,
}

/// Tracks task execution state for an agent.
#[derive(Debug, Default, Clone)]
pub struct AgentState {
    pub active_tasks: usize,
    pub queued_tasks: usize,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub last_task_time: u64,
}

/// A2A Quota manager — enforces concurrency limits and queue policies.
pub struct QuotaManager {
    quotas: Arc<Mutex<HashMap<String, AgentQuota>>>,
    agent_states: Arc<Mutex<HashMap<String, AgentState>>>,
    default_policy: QueuePolicy,
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self {
            quotas: Arc::new(Mutex::new(HashMap::new())),
            agent_states: Arc::new(Mutex::new(HashMap::new())),
            default_policy: QueuePolicy::Drop,
        }
    }
}

impl QuotaManager {
    pub fn new() -> Self {
        Self {
            quotas: Arc::new(Mutex::new(HashMap::new())),
            agent_states: Arc::new(Mutex::new(HashMap::new())),
            default_policy: QueuePolicy::Drop,
        }
    }

    pub fn with_default_policy(mut self, policy: QueuePolicy) -> Self {
        self.default_policy = policy;
        self
    }

    pub fn register_agent(&self, quota: AgentQuota) {
        let mut quotas = self.quotas.lock();
        quotas.insert(quota.agent_id.clone(), quota);
    }

    pub fn get_quota(&self, agent_id: &str) -> Option<AgentQuota> {
        self.quotas.lock().get(agent_id).cloned()
    }

    pub fn get_state(&self, agent_id: &str) -> AgentState {
        self.agent_states
            .lock()
            .get(agent_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn can_accept(&self, agent_id: &str) -> Result<bool, QuotaError> {
        let quota = self
            .quotas
            .lock()
            .get(agent_id)
            .cloned()
            .ok_or_else(|| QuotaError::AgentNotFound(agent_id.into()))?;

        let state = self.get_state(agent_id);

        if state.active_tasks >= quota.max_concurrent_tasks {
            return Ok(false);
        }

        if state.queued_tasks >= quota.max_queue_depth {
            return Ok(false);
        }

        let now = current_timestamp();
        let elapsed = now.saturating_sub(state.last_task_time);
        if elapsed < 60 && state.tasks_completed > 0 {
            let rate = state.tasks_completed * 60 / elapsed.max(1);
            if rate >= quota.rate_limit_per_minute {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn start_task(&self, agent_id: &str) -> Result<(), QuotaError> {
        if !self.can_accept(agent_id)? {
            return Err(QuotaError::Exceeded(format!(
                "agent {agent_id} at capacity"
            )));
        }

        let mut states = self.agent_states.lock();
        let state = states.entry(agent_id.into()).or_default();
        state.active_tasks += 1;
        state.last_task_time = current_timestamp();

        Ok(())
    }

    pub fn complete_task(&self, agent_id: &str, success: bool) {
        let mut states = self.agent_states.lock();
        if let Some(state) = states.get_mut(agent_id) {
            state.active_tasks = state.active_tasks.saturating_sub(1);
            if success {
                state.tasks_completed += 1;
            } else {
                state.tasks_failed += 1;
            }
        }
    }

    pub fn queue_task(
        &self,
        agent_id: &str,
        policy: Option<QueuePolicy>,
    ) -> Result<(), QuotaError> {
        let policy = policy.unwrap_or(self.default_policy);
        let quota = self
            .quotas
            .lock()
            .get(agent_id)
            .cloned()
            .ok_or_else(|| QuotaError::AgentNotFound(agent_id.into()))?;

        let mut states = self.agent_states.lock();
        let state = states.entry(agent_id.into()).or_default();

        match policy {
            QueuePolicy::Drop => {
                if state.queued_tasks >= quota.max_queue_depth {
                    return Err(QuotaError::Exceeded("queue full".into()));
                }
                state.queued_tasks += 1;
            }
            QueuePolicy::Block => {
                if state.queued_tasks >= quota.max_queue_depth {
                    return Err(QuotaError::Exceeded("queue full, blocking".into()));
                }
                state.queued_tasks += 1;
            }
            QueuePolicy::DropOldest => {
                if state.queued_tasks >= quota.max_queue_depth {
                    state.queued_tasks = state.queued_tasks.saturating_sub(1);
                }
                state.queued_tasks += 1;
            }
        }

        Ok(())
    }

    pub fn dequeue_task(&self, agent_id: &str) {
        let mut states = self.agent_states.lock();
        if let Some(state) = states.get_mut(agent_id) {
            state.queued_tasks = state.queued_tasks.saturating_sub(1);
        }
    }

    pub fn all_states(&self) -> HashMap<String, AgentState> {
        let states = self.agent_states.lock();
        states.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
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
    fn register_and_get_quota() {
        let manager = QuotaManager::new();
        let quota = AgentQuota {
            agent_id: "agent1".into(),
            max_concurrent_tasks: 3,
            max_queue_depth: 50,
            rate_limit_per_minute: 30,
        };

        manager.register_agent(quota.clone());
        let fetched = manager.get_quota("agent1").unwrap();
        assert_eq!(fetched.max_concurrent_tasks, 3);
    }

    #[test]
    fn can_accept_within_limits() {
        let manager = QuotaManager::new();
        manager.register_agent(AgentQuota {
            agent_id: "agent1".into(),
            max_concurrent_tasks: 5,
            max_queue_depth: 10,
            rate_limit_per_minute: 60,
        });

        assert!(manager.can_accept("agent1").unwrap());
    }

    #[test]
    fn cannot_accept_unknown_agent() {
        let manager = QuotaManager::new();
        assert!(manager.can_accept("unknown").is_err());
    }

    #[test]
    fn start_task_increments_active() {
        let manager = QuotaManager::new();
        manager.register_agent(AgentQuota {
            agent_id: "agent1".into(),
            max_concurrent_tasks: 5,
            max_queue_depth: 10,
            rate_limit_per_minute: 60,
        });

        manager.start_task("agent1").unwrap();
        let state = manager.get_state("agent1");
        assert_eq!(state.active_tasks, 1);
    }

    #[test]
    fn complete_task_decrements_active() {
        let manager = QuotaManager::new();
        manager.register_agent(AgentQuota {
            agent_id: "agent1".into(),
            max_concurrent_tasks: 5,
            max_queue_depth: 10,
            rate_limit_per_minute: 60,
        });

        manager.start_task("agent1").unwrap();
        manager.complete_task("agent1", true);

        let state = manager.get_state("agent1");
        assert_eq!(state.active_tasks, 0);
        assert_eq!(state.tasks_completed, 1);
    }

    #[test]
    fn queue_policy_drop_rejects_when_full() {
        let manager = QuotaManager::new().with_default_policy(QueuePolicy::Drop);
        manager.register_agent(AgentQuota {
            agent_id: "agent1".into(),
            max_concurrent_tasks: 5,
            max_queue_depth: 1,
            rate_limit_per_minute: 60,
        });

        manager.queue_task("agent1", None).unwrap();
        assert!(manager.queue_task("agent1", None).is_err());
    }

    #[test]
    fn queue_policy_drop_oldest_makes_room() {
        let manager = QuotaManager::new().with_default_policy(QueuePolicy::DropOldest);
        manager.register_agent(AgentQuota {
            agent_id: "agent1".into(),
            max_concurrent_tasks: 5,
            max_queue_depth: 1,
            rate_limit_per_minute: 60,
        });

        manager.queue_task("agent1", None).unwrap();
        manager.queue_task("agent1", None).unwrap();

        let state = manager.get_state("agent1");
        assert_eq!(state.queued_tasks, 1);
    }

    #[test]
    fn concurrency_limit_enforced() {
        let manager = QuotaManager::new();
        manager.register_agent(AgentQuota {
            agent_id: "agent1".into(),
            max_concurrent_tasks: 2,
            max_queue_depth: 10,
            rate_limit_per_minute: 60,
        });

        manager.start_task("agent1").unwrap();
        manager.start_task("agent1").unwrap();

        // Third task should fail.
        assert!(manager.start_task("agent1").is_err());
    }
}
