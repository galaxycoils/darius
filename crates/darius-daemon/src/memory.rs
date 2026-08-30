//! Hindsight Memory — session compression, mental models, search.
//!
//! Backed by `darius-memory` for durable storage. The in-memory HashMap
//! is only a session-local cache; the SQLite database is the source of truth.

use darius_core::SessionHandoff;
use darius_memory::{MemoryEngine, MemoryError as EngineError, SearchQuery};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("memory not found: {0}")]
    NotFound(String),
    #[error("backend error: {0}")]
    Backend(#[from] EngineError),
}

/// A compressed session memory.
#[derive(Debug, Clone)]
pub struct SessionMemory {
    pub session_id: String,
    pub profile: String,
    pub summary: String,
    pub decisions: Vec<String>,
    pub timestamp: u64,
}

/// A mental model — aggregated understanding across sessions.
#[derive(Debug, Clone, Default)]
pub struct MentalModel {
    pub profile: String,
    pub total_sessions: usize,
    pub common_decisions: Vec<String>,
    pub patterns: Vec<String>,
}

/// Hindsight Memory service — profile-scoped session recall.
pub struct HindsightMemory {
    engine: Option<Arc<MemoryEngine>>,
    profile: String,
    // Session-local cache only — not the source of truth
    cache: Arc<Mutex<HashMap<String, Vec<SessionMemory>>>>,
}

impl HindsightMemory {
    /// Create an in-memory-only instance (for tests and backward compat).
    pub fn new() -> Self {
        Self {
            engine: None,
            profile: "default".into(),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a durable instance backed by `darius-memory`.
    pub fn with_profile(profile_dir: &Path, profile: &str) -> Result<Self, MemoryError> {
        let memory_dir = profile_dir.join("memory");
        let engine = MemoryEngine::open(&memory_dir)?;
        Ok(Self {
            engine: Some(Arc::new(engine)),
            profile: profile.into(),
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Compress a session into a memory.
    pub fn compress_session(
        &self,
        profile: &str,
        session_id: &str,
        handoff: &SessionHandoff,
    ) -> SessionMemory {
        let memory = SessionMemory {
            session_id: session_id.to_string(),
            profile: profile.into(),
            summary: handoff.goal.clone(),
            decisions: handoff
                .prior_decisions
                .iter()
                .map(|d| d.choice.clone())
                .collect(),
            timestamp: current_timestamp(),
        };

        // Write to backend if available
        if let Some(ref engine) = self.engine {
            let _ = engine.distill_handoff(handoff);
        }

        // Update session-local cache
        let mut cache = self.cache.lock();
        cache
            .entry(profile.into())
            .or_default()
            .push(memory.clone());

        memory
    }

    /// Recall memories for a profile.
    pub fn recall(&self, profile: &str) -> Vec<SessionMemory> {
        // Try backend first for durable recall
        if let Some(ref engine) = self.engine {
            if let Ok(records) = engine.search(&SearchQuery {
                text: None,
                kinds: vec![],
                limit: 100,
            }) {
                return records
                    .into_iter()
                    .map(|r| SessionMemory {
                        session_id: r.id,
                        profile: self.profile.clone(),
                        summary: r.body,
                        decisions: r.tags,
                        timestamp: r.created_at as u64,
                    })
                    .collect();
            }
        }

        // Fall back to session-local cache
        self.cache.lock().get(profile).cloned().unwrap_or_default()
    }

    /// Recall across all profiles (admin only).
    pub fn recall_all(&self) -> HashMap<String, Vec<SessionMemory>> {
        self.cache.lock().clone()
    }

    /// Build a mental model for a profile.
    pub fn build_mental_model(&self, profile: &str) -> MentalModel {
        let memories = self.recall(profile);
        let mut all_decisions: Vec<String> = Vec::new();

        for mem in &memories {
            all_decisions.extend(mem.decisions.clone());
        }

        let mut decision_counts: HashMap<String, usize> = HashMap::new();
        for decision in &all_decisions {
            *decision_counts.entry(decision.clone()).or_default() += 1;
        }

        let common_decisions: Vec<String> = decision_counts
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(decision, _)| decision.clone())
            .collect();

        MentalModel {
            profile: profile.to_string(),
            total_sessions: memories.len(),
            common_decisions: common_decisions.clone(),
            patterns: common_decisions,
        }
    }

    /// Search memories by keyword.
    pub fn search(&self, profile: &str, keyword: &str) -> Vec<SessionMemory> {
        // Try backend search first
        if let Some(ref engine) = self.engine {
            if let Ok(records) = engine.search(&SearchQuery {
                text: Some(keyword.into()),
                kinds: vec![],
                limit: 12,
            }) {
                return records
                    .into_iter()
                    .map(|r| SessionMemory {
                        session_id: r.id,
                        profile: self.profile.clone(),
                        summary: r.body,
                        decisions: r.tags,
                        timestamp: r.created_at as u64,
                    })
                    .collect();
            }
        }

        // Fall back to cache
        self.recall(profile)
            .into_iter()
            .filter(|mem| {
                mem.summary.contains(keyword) || mem.decisions.iter().any(|d| d.contains(keyword))
            })
            .collect()
    }

    /// Clear all memories for a profile.
    pub fn clear_profile(&self, profile: &str) {
        self.cache.lock().remove(profile);
    }
}

impl Default for HindsightMemory {
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
    use darius_core::{Decision, SessionHandoff};

    #[test]
    fn compress_and_recall_session() {
        let memory = HindsightMemory::new();
        let handoff = SessionHandoff {
            version: 1,
            goal: "test goal".to_string(),
            prior_decisions: vec![Decision {
                context: "ctx".to_string(),
                choice: "choice-a".to_string(),
                rationale: "because".to_string(),
            }],
            open_questions: Vec::new(),
            constraints: Vec::new(),
            artifact_refs: Vec::new(),
        };

        memory.compress_session("profile1", "sess1", &handoff);
        let recalled = memory.recall("profile1");

        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].summary, "test goal");
        assert_eq!(recalled[0].decisions.len(), 1);
    }

    #[test]
    fn cross_session_decision_recall() {
        let memory = HindsightMemory::new();

        for i in 0..3 {
            let handoff = SessionHandoff {
                version: 1,
                goal: format!("goal_{i}"),
                prior_decisions: vec![Decision {
                    context: "ctx".to_string(),
                    choice: "repeat-decision".to_string(),
                    rationale: "because".to_string(),
                }],
                open_questions: Vec::new(),
                constraints: Vec::new(),
                artifact_refs: Vec::new(),
            };
            memory.compress_session("profile1", &format!("sess{i}"), &handoff);
        }

        let model = memory.build_mental_model("profile1");
        assert_eq!(model.total_sessions, 3);
        assert!(
            model
                .common_decisions
                .contains(&"repeat-decision".to_string())
        );
    }

    #[test]
    fn no_cross_profile_leak() {
        let memory = HindsightMemory::new();

        let handoff1 = SessionHandoff {
            version: 1,
            goal: "secret goal".to_string(),
            prior_decisions: vec![Decision {
                context: "ctx".to_string(),
                choice: "secret".to_string(),
                rationale: "private".to_string(),
            }],
            open_questions: Vec::new(),
            constraints: Vec::new(),
            artifact_refs: Vec::new(),
        };

        memory.compress_session("profile_a", "sess_a", &handoff1);

        let recalled_b = memory.recall("profile_b");
        assert!(recalled_b.is_empty());

        let recalled_a = memory.recall("profile_a");
        assert_eq!(recalled_a.len(), 1);
    }

    #[test]
    fn search_memories() {
        let memory = HindsightMemory::new();

        let handoff = SessionHandoff {
            version: 1,
            goal: "searchable goal".to_string(),
            prior_decisions: vec![Decision {
                context: "ctx".to_string(),
                choice: "find-me".to_string(),
                rationale: "test".to_string(),
            }],
            open_questions: Vec::new(),
            constraints: Vec::new(),
            artifact_refs: Vec::new(),
        };

        memory.compress_session("profile1", "sess1", &handoff);

        let results = memory.search("profile1", "searchable");
        assert_eq!(results.len(), 1);

        let results = memory.search("profile1", "find-me");
        assert_eq!(results.len(), 1);

        let results = memory.search("profile1", "nonexistent");
        assert!(results.is_empty());
    }
}
