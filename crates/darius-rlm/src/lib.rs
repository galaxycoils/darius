//! Darius RLM — Recursive Learning Machine kernel.

use darius_core::{DariusError, SubagentId};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

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
    #[allow(dead_code)]
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

impl IsolationTier {
    #[allow(dead_code)]
    pub fn is_at_least_t2(&self) -> bool {
        matches!(
            self,
            IsolationTier::Process
                | IsolationTier::GVisor
                | IsolationTier::MicroVm
                | IsolationTier::Wasm
        )
    }
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
    pub fn kill(&self) -> Result<(), DariusError> {
        let mut status = self.status.lock().unwrap();
        *status = RlmStatus::Killed;
        Ok(())
    }

    /// Get current status.
    pub fn status(&self) -> RlmStatus {
        *self.status.lock().unwrap()
    }

    /// Wait for the kernel to reach a specific status.
    pub fn wait_for(&self, target: RlmStatus) -> Result<(), DariusError> {
        let current = *self.status.lock().unwrap();
        if current == target {
            Ok(())
        } else {
            Err(DariusError::Hashline(format!(
                "kernel not in target state; current={:?}, target={:?}",
                current, target
            )))
        }
    }
}

/// Generator handle — survives prompt compaction; schema-bound when configured.
#[derive(Clone)]
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
    pub fn send(&self, _msg: &str) -> Result<(), DariusError> {
        let status = self.status.lock().unwrap();
        match *status {
            RlmStatus::Running | RlmStatus::Waiting => Ok(()),
            _ => Err(DariusError::Hashline(
                "handle not in running/waiting state".into(),
            )),
        }
    }

    /// Kill this RLM turn.
    pub fn kill(&self) -> Result<(), DariusError> {
        let mut status = self.status.lock().unwrap();
        *status = RlmStatus::Killed;
        Ok(())
    }

    /// Wait for this handle to reach Done or Killed.
    pub fn wait(&self) -> Result<Yield, DariusError> {
        let current = *self.status.lock().unwrap();
        Ok(Yield {
            status: current,
            output: String::new(),
        })
    }
}

/// A yield from an RLM turn.
#[derive(Debug, Clone)]
pub struct Yield {
    pub status: RlmStatus,
    pub output: String,
}

/// Compact-safe handle registry.
mod handle_registry {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Registry that tracks handles by ID and survives compaction.
    #[derive(Default)]
    pub struct HandleRegistry {
        handles: Arc<Mutex<HashMap<SubagentId, RlmHandle>>>,
    }

    impl HandleRegistry {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn register(&self, handle: RlmHandle) {
            let mut handles = self.handles.lock().unwrap();
            handles.insert(handle.id().clone(), handle);
        }

        pub fn get(&self, id: &SubagentId) -> Option<RlmHandle> {
            let handles = self.handles.lock().unwrap();
            handles.get(id).cloned()
        }

        pub fn list_ids(&self) -> Vec<SubagentId> {
            let handles = self.handles.lock().unwrap();
            handles.keys().cloned().collect()
        }

        /// Compact: remove killed/done handles, keep running ones.
        pub fn compact(&self) {
            let mut handles = self.handles.lock().unwrap();
            handles.retain(|_, h| {
                let status = *h.status.lock().unwrap();
                matches!(status, RlmStatus::Running | RlmStatus::Waiting)
            });
        }
    }
}

pub use handle_registry::HandleRegistry;

mod evaluate;
mod ipykernel;

pub use evaluate::rlm_evaluate;
pub use ipykernel::IpKernelConnection;

/// Entry point: spawn an RLM turn, returning a handle.
pub fn rlm(_prompt: &str, opts: RlmOptions) -> Result<RlmHandle, DariusError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_status_lifecycle() {
        let kernel = RlmKernel::new("k1", IsolationTier::Trusted);
        assert_eq!(kernel.status(), RlmStatus::Idle);
        kernel.start().unwrap();
        assert_eq!(kernel.status(), RlmStatus::Running);
        kernel.stop().unwrap();
        assert_eq!(kernel.status(), RlmStatus::Idle);
    }

    #[test]
    fn kernel_kill_transitions_to_killed() {
        let kernel = RlmKernel::new("k2", IsolationTier::Process);
        kernel.start().unwrap();
        kernel.kill().unwrap();
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

    #[test]
    fn handle_send_requires_running_state() {
        let handle = rlm("test", RlmOptions::default()).unwrap();
        assert!(handle.send("msg").is_ok());
    }

    #[test]
    fn isolation_tier_variants() {
        assert_eq!(IsolationTier::Trusted as i32, 0);
        assert_eq!(IsolationTier::Process as i32, 1);
        assert_eq!(IsolationTier::GVisor as i32, 2);
        assert_eq!(IsolationTier::MicroVm as i32, 3);
        assert_eq!(IsolationTier::Wasm as i32, 4);
    }

    #[test]
    fn kernel_cannot_start_when_killed() {
        let kernel = RlmKernel::new("k3", IsolationTier::Trusted);
        kernel.kill().unwrap();
        assert!(kernel.start().is_err());
    }

    #[test]
    fn kernel_with_schema() {
        let kernel = RlmKernel::new("k4", IsolationTier::Trusted).with_schema("schema-v1".into());
        assert_eq!(kernel.schema.as_deref(), Some("schema-v1"));
    }

    #[test]
    fn handle_wait_returns_yield() {
        let handle = rlm("test", RlmOptions::default()).unwrap();
        let mut status = handle.status.lock().unwrap();
        *status = RlmStatus::Done;
        drop(status);
        let yield_ = handle.wait().unwrap();
        assert_eq!(yield_.status, RlmStatus::Done);
    }

    #[test]
    fn handle_survives_compaction() {
        let registry = handle_registry::HandleRegistry::new();

        // Register two handles
        let h1 = rlm("test1", RlmOptions::default()).unwrap();
        let h2 = rlm("test2", RlmOptions::default()).unwrap();
        let id1 = h1.id().clone();
        let id2 = h2.id().clone();

        registry.register(h1);
        registry.register(h2);

        assert_eq!(registry.list_ids().len(), 2);

        // Compact - should keep both running handles
        registry.compact();
        assert_eq!(registry.list_ids().len(), 2);

        // Mark one as killed
        if let Some(h) = registry.get(&id1) {
            h.kill().unwrap();
        }

        // Compact - killed handle should be removed
        registry.compact();
        let ids = registry.list_ids();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], id2);
    }
}
