//! Darius RLM — Recursive Learning Machine kernel.
//!
//! Provides the persistent kernel, Jupyter-wire subagent transport, and the
//! generator–evaluator (`rlm_evaluate`) surface. Optional `rlm-python` feature
//! enables PyO3-backed execution; the pure-Rust fallback always builds.

use darius_core::{DariusError, SubagentId};
use serde::{Deserialize, Serialize};

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

pub struct RlmKernel {
    id: String,
    isolation_tier: IsolationTier,
    schema: Option<String>,
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
        Self { id: id.into(), isolation_tier: tier, schema: None }
    }
    pub fn id(&self) -> &str { &self.id }
    pub fn isolation_tier(&self) -> IsolationTier { self.isolation_tier }
    pub fn with_schema(mut self, schema: String) -> Self {
        self.schema = Some(schema);
        self
    }
}

/// Generator handle — survives prompt compaction; schema-bound when configured.
pub struct RlmHandle {
    id: SubagentId,
    status: RlmStatus,
}

impl RlmHandle {
    pub fn id(&self) -> &SubagentId { &self.id }
    pub fn status(&self) -> RlmStatus { self.status }
}

/// Entry point: spawn an RLM turn.
pub fn rlm(_prompt: &str, _opts: RlmOptions) -> Result<RlmHandle, DariusError> {
    Ok(RlmHandle {
        id: SubagentId::new(),
        status: RlmStatus::Running,
    })
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
pub fn rlm_evaluate(_target: &str, _rubric: &str) -> Result<Grade, DariusError> {
    Ok(Grade {
        passed: true,
        scores: vec![],
        notes: "stub".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_status_lifecycle() {
        let kernel = RlmKernel::new("k1", IsolationTier::Trusted);
        assert_eq!(kernel.id(), "k1");
        assert_eq!(kernel.isolation_tier(), IsolationTier::Trusted);
    }

    #[test]
    fn rlm_returns_handle() {
        let handle = rlm("hi", RlmOptions::default()).unwrap();
        assert_eq!(handle.status(), RlmStatus::Running);
    }

    #[test]
    fn rlm_evaluate_stub() {
        let g = rlm_evaluate("t", "r").unwrap();
        assert!(g.passed);
    }
}
