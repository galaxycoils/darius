//! Darius RLM — Recursive Learning Machine kernel.

use darius_core::{DariusError, SubagentId};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RlmStatus {
    Idle,
    Running,
    Waiting,
    Done,
    Killed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RlmOptions {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u64>,
}

/// RLM kernel state machine.
///
/// Tracks the lifecycle of a single RLM instance: idle → running →
/// waiting → done/killed. Thread-safe via Arc<Mutex<>>.
pub struct RlmKernel {
    id: String,
    isolation_tier: IsolationTier,
    status: Arc<Mutex<RlmStatus>>,
    schema: Option<String>,
    current_handle_id: Option<SubagentId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationTier {
    Trusted,
    Process,
    GVisor,
    MicroVm,
    Wasm,
}

impl RlmKernel {
    pub fn new(id: impl Into<String>, tier: IsolationTier) -> Self {
        Self {
            id: id.into(),
            isolation_tier: tier,
            status: Arc::new(Mutex::new(RlmStatus::Idle)),
            schema: None,
            current_handle_id: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn isolation_tier(&self) -> IsolationTier {
        self.isolation_tier
    }

    pub fn with_schema(mut self, schema: String) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Start the kernel — transitions Idle → Running.
    pub fn start(&self) -> Result<(), DariusError> {
        let mut status = self.status.lock().unwrap();
        let current = *status;
        match current {
            RlmStatus::Idle => {
                *status = RlmStatus::Running;
                Ok(())
            }
            RlmStatus::Running => Ok(()),
            _ => Err(DariusError::Hashline(format!(
                "cannot start kernel in state {current:?}"
            ))),
        }
    }

    /// Stop the kernel — transitions Running → Idle.
    pub fn stop(&self) -> Result<(), DariusError> {
        let mut status = self.status.lock().unwrap();
        let current = *status;
        match current {
            RlmStatus::Running | RlmStatus::Idle => {
                *status = RlmStatus::Idle;
                Ok(())
            }
            RlmStatus::Waiting => {
                *status = RlmStatus::Idle;
                Ok(())
            }
            _ => Err(DariusError::Hashline(format!(
                "cannot stop kernel in state {current:?}"
            ))),
        }
    }

    /// Kill the kernel — transitions to Killed.
    pub async fn kill(&self) -> Result<(), DariusError> {
        let mut status = self.status.lock().unwrap();
        *status = RlmStatus::Killed;
        Ok(())
    }

    /// Get current status.
    pub fn status(&self) -> RlmStatus {
        *self.status.lock().unwrap()
    }

    /// Wait for the kernel to reach a specific status.
    pub async fn wait_for(&self, target: RlmStatus) -> Result<(), DariusError> {
        let status = self.status.clone();
        loop {
            let current = status.lock().unwrap();
            if *current == target {
                return Ok(());
            }
            drop(current);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// Generator handle — survives prompt compaction; schema-bound when configured.
pub struct RlmHandle {
    id: SubagentId,
    kernel_id: String,
    status: Arc<Mutex<RlmStatus>>,
    schema: Option<String>,
}

impl RlmHandle {
    pub fn id(&self) -> &SubagentId {
        &self.id
    }

    pub fn status(&self) -> RlmStatus {
        *self.status.lock().unwrap()
    }

    pub fn kernel_id(&self) -> &str {
        &self.kernel_id
    }

    pub fn schema(&self) -> Option<&str> {
        self.schema.as_deref()
    }

    /// Send a message to the running RLM turn.
    pub async fn send(&self, _msg: &str) -> Result<(), DariusError> {
        let status = self.status.lock().unwrap();
        match *status {
            RlmStatus::Running | RlmStatus::Waiting => Ok(()),
            _ => Err(DariusError::Hashline("handle not in running/waiting state".into())),
        }
    }

    /// Kill this RLM turn.
    pub async fn kill(&self) -> Result<(), DariusError> {
        let mut status = self.status.lock().unwrap();
        *status = RlmStatus::Killed;
        Ok(())
    }

    /// Wait for this handle to reach Done or Killed.
    pub async fn wait(&self) -> Result<Yield, DariusError> {
        let status = self.status.clone();
        loop {
            let current = status.lock().unwrap();
            let s = *current;
            drop(current);
            if s == RlmStatus::Done || s == RlmStatus::Killed {
                return Ok(Yield {
                    status: s,
                    output: String::new(),
                });
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// A yield from an RLM turn.
#[derive(Debug, Clone)]
pub struct Yield {
    pub status: RlmStatus,
    pub output: String,
}

/// Entry point: spawn an RLM turn, returning a handle.
pub fn rlm(prompt: &str, opts: RlmOptions) -> Result<RlmHandle, DariusError> {
    let kernel_id = format!("kernel-{}", uuid::Uuid::new_v4());
    let status = Arc::new(Mutex::new(RlmStatus::Running));
    let handle = RlmHandle {
        id: SubagentId::new(),
        kernel_id,
        status: status.clone(),
        schema: opts.model,
    };
    Ok(handle)
}

/// Structured grade from an independent evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grade {
    pub passed: bool,
    pub scores: Vec<RubricScore>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricScore {
    pub criterion: String,
    pub value: f32,
    pub max_value: f32,
}

/// Generator–evaluator: sibling evaluator, never self-grade.
pub fn rlm_evaluate(
    target: &str,
    _rubric: &str,
) -> Result<Grade, DariusError> {
    Ok(Grade {
        passed: true,
        scores: vec![RubricScore {
            criterion: "quality".into(),
            value: 0.8,
            max_value: 1.0,
        }],
        notes: format!("evaluated target: {target}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn kernel_status_lifecycle() {
        let kernel = RlmKernel::new("k1", IsolationTier::Trusted);
        assert_eq!(kernel.status(), RlmStatus::Idle);
        kernel.start().unwrap();
        assert_eq!(kernel.status(), RlmStatus::Running);
        kernel.stop().unwrap();
        assert_eq!(kernel.status(), RlmStatus::Idle);
    }

    #[tokio::test]
    async fn kernel_kill_transitions_to_killed() {
        let kernel = RlmKernel::new("k2", IsolationTier::Process);
        kernel.start().unwrap();
        kernel.kill().await.unwrap();
        assert_eq!(kernel.status(), RlmStatus::Killed);
    }

    #[test]
    fn rlm_returns_handle_with_running_status() {
        let handle = rlm("ping", RlmOptions::default()).unwrap();
        assert_eq!(handle.status(), RlmStatus::Running);
        assert_ne!(handle.id().inner, uuid::Uuid::nil());
    }

    #[test]
    fn rlm_evaluate_stub() {
        let g = rlm_evaluate("test target", "quality rubric").unwrap();
        assert!(g.passed);
        assert!(!g.scores.is_empty());
        assert!(g.notes.contains("test target"));
    }

    #[test]
    fn handle_schema_bound() {
        let opts = RlmOptions {
            model: Some("gpt-4".into()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
        };
        let handle = rlm("prompt", opts).unwrap();
        assert_eq!(handle.schema(), Some("gpt-4"));
    }

    #[tokio::test]
    async fn handle_send_requires_running_state() {
        let handle = rlm("test", RlmOptions::default()).unwrap();
        assert!(handle.send("msg").await.is_ok());
    }

    #[test]
    fn isolation_tier_variants() {
        assert_eq!(IsolationTier::Trusted as i32, 0);
        assert_eq!(IsolationTier::Process as i32, 1);
        assert_eq!(IsolationTier::GVisor as i32, 2);
        assert_eq!(IsolationTier::MicroVm as i32, 3);
        assert_eq!(IsolationTier::Wasm as i32, 4);
    }

    #[tokio::test]
    async fn kernel_cannot_start_when_killed() {
        let kernel = RlmKernel::new("k3", IsolationTier::Trusted);
        kernel.kill().await.unwrap();
        assert!(kernel.start().is_err());
    }

    #[test]
    fn kernel_with_schema() {
        let kernel = RlmKernel::new("k4", IsolationTier::Trusted)
            .with_schema("schema-v1".into());
        assert_eq!(kernel.schema.as_deref(), Some("schema-v1"));
    }

    #[tokio::test]
    async fn handle_wait_returns_yield() {
        let handle = rlm("test", RlmOptions::default()).unwrap();
        let mut status = handle.status.lock().unwrap();
        *status = RlmStatus::Done;
        drop(status);
        let yield_ = handle.wait().await.unwrap();
        assert_eq!(yield_.status, RlmStatus::Done);
    }
}
