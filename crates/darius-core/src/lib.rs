//! Darius core types, IDs, and shared utilities.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubagentId {
    pub inner: uuid::Uuid,
}

impl SubagentId {
    pub fn new() -> Self {
        Self { inner: uuid::Uuid::new_v4() }
    }
}

impl Default for SubagentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Turn-level cache statistics for the CacheCoordinator.
///
/// Per turn: `prefix_bytes`, `break_offset`, `suffix_hash`,
/// `cache_hit`, `miss_cost_tokens`.
/// Invariant: no timestamps or session IDs in the prefix region.
#[derive(Debug, Clone, Default)]
pub struct TurnCacheStats {
    pub prefix_bytes: usize,
    pub break_offset: usize,
    pub suffix_hash: u64,
    pub cache_hit: bool,
    pub miss_cost_tokens: u64,
}

/// Versioned session handoff artifact.
///
/// Emitted on session end; loaded as initial context on next session.
/// Not raw chat logs — structured goal, decisions, constraints, artifacts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionHandoff {
    pub version: u32,
    pub goal: String,
    pub prior_decisions: Vec<Decision>,
    pub open_questions: Vec<String>,
    pub constraints: Vec<String>,
    pub artifact_refs: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Decision {
    pub context: String,
    pub choice: String,
    pub rationale: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactRef {
    pub id: String,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Error)]
pub enum DariusError {
    #[error("not implemented")]
    NotImplemented,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("hashline error: {0}")]
    Hashline(String),
}

/// Structured grade from the independent evaluator (AutoRater).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Grade {
    pub passed: bool,
    pub scores: Vec<RubricScore>,
    pub notes: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RubricScore {
    pub criterion: String,
    pub value: f32,
    pub max_value: f32,
}
