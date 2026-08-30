//! SessionHandoff persistence — re-exports core types, adds disk-backed store.

// Re-export the canonical core types so consumers use one source of truth.
pub use darius_core::{ArtifactRef, Decision, SessionHandoff};

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Handoff store error types.
#[derive(Debug, Error)]
pub enum HandoffError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("handoff not found: {0}")]
    NotFound(String),
}

/// Disk-backed store for versioned session handoffs.
pub struct HandoffStore {
    base_dir: PathBuf,
}

impl HandoffStore {
    /// Create a new store rooted at the given directory.
    pub fn new(base_dir: impl AsRef<Path>) -> Result<Self, HandoffError> {
        let base = PathBuf::from(base_dir.as_ref());
        std::fs::create_dir_all(&base)?;
        Ok(Self { base_dir: base })
    }

    /// Path to a session's handoff file.
    fn handoff_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{session_id}.json"))
    }

    /// Save a handoff for a session.
    pub fn save(&self, session_id: &str, handoff: &SessionHandoff) -> Result<(), HandoffError> {
        let json = serde_json::to_string_pretty(handoff)?;
        std::fs::write(self.handoff_path(session_id), json)?;
        Ok(())
    }

    /// Load a handoff for a session.
    pub fn load(&self, session_id: &str) -> Result<SessionHandoff, HandoffError> {
        let path = self.handoff_path(session_id);
        if !path.exists() {
            return Err(HandoffError::NotFound(session_id.to_string()));
        }
        let json = std::fs::read_to_string(&path)?;
        let handoff = serde_json::from_str(&json)?;
        Ok(handoff)
    }

    /// Load a handoff, returning a new default if not found.
    pub fn load_or_default(&self, session_id: &str, goal: String) -> SessionHandoff {
        self.load(session_id).unwrap_or_else(|_| SessionHandoff {
            version: 1,
            goal,
            prior_decisions: Vec::new(),
            open_questions: Vec::new(),
            constraints: Vec::new(),
            artifact_refs: Vec::new(),
        })
    }

    /// List all session IDs that have handoffs.
    pub fn list_sessions(&self) -> Result<Vec<String>, HandoffError> {
        let mut sessions = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || !path.extension().is_some_and(|e| e == "json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            sessions.push(stem.to_string());
        }
        Ok(sessions)
    }

    /// Delete a handoff.
    pub fn delete(&self, session_id: &str) -> Result<(), HandoffError> {
        let path = self.handoff_path(session_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("darius_handoff_test_{}", uuid::Uuid::new_v4()));
        if path.exists() {
            std::fs::remove_dir_all(&path).ok();
        }
        path
    }

    #[test]
    fn store_save_and_load() {
        let dir = temp_dir();
        let store = HandoffStore::new(&dir).unwrap();

        let h = SessionHandoff {
            version: 1,
            goal: "test goal".to_string(),
            prior_decisions: vec![Decision {
                context: "ctx".to_string(),
                choice: "choice".to_string(),
                rationale: "because".to_string(),
            }],
            open_questions: vec!["what?".to_string()],
            constraints: vec!["safe".to_string()],
            artifact_refs: vec![ArtifactRef {
                id: "art1".to_string(),
                path: "/p".to_string(),
                description: "desc".to_string(),
            }],
        };

        store.save("s1", &h).unwrap();
        let loaded = store.load("s1").unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.goal, "test goal");
        assert_eq!(loaded.prior_decisions.len(), 1);
        assert_eq!(loaded.prior_decisions[0].choice, "choice");
        assert_eq!(loaded.open_questions[0], "what?");
        assert_eq!(loaded.constraints[0], "safe");
        assert_eq!(loaded.artifact_refs[0].id, "art1");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn store_load_or_default_returns_new_when_missing() {
        let dir = temp_dir();
        let store = HandoffStore::new(&dir).unwrap();

        let loaded = store.load_or_default("missing", "default goal".to_string());
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.goal, "default goal");
        assert!(loaded.prior_decisions.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn store_list_sessions() {
        let dir = temp_dir();
        let store = HandoffStore::new(&dir).unwrap();

        let h = SessionHandoff {
            version: 1,
            goal: "g".to_string(),
            prior_decisions: Vec::new(),
            open_questions: Vec::new(),
            constraints: Vec::new(),
            artifact_refs: Vec::new(),
        };
        store.save("s1", &h).unwrap();
        store.save("s2", &h).unwrap();

        let mut sessions = store.list_sessions().unwrap();
        sessions.sort();
        assert_eq!(sessions, vec!["s1", "s2"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn store_delete() {
        let dir = temp_dir();
        let store = HandoffStore::new(&dir).unwrap();

        let h = SessionHandoff {
            version: 1,
            goal: "g".to_string(),
            prior_decisions: Vec::new(),
            open_questions: Vec::new(),
            constraints: Vec::new(),
            artifact_refs: Vec::new(),
        };
        store.save("s1", &h).unwrap();
        assert!(store.load("s1").is_ok());

        store.delete("s1").unwrap();
        assert!(store.load("s1").is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn handoff_round_trip_via_store() {
        let dir = temp_dir();
        let store = HandoffStore::new(&dir).unwrap();

        let h = SessionHandoff {
            version: 2,
            goal: "round trip".to_string(),
            prior_decisions: vec![Decision {
                context: "c".to_string(),
                choice: "x".to_string(),
                rationale: "y".to_string(),
            }],
            open_questions: vec!["q".to_string()],
            constraints: vec!["c1".to_string()],
            artifact_refs: vec![ArtifactRef {
                id: "a".to_string(),
                path: "/x".to_string(),
                description: "d".to_string(),
            }],
        };

        store.save("rt", &h).unwrap();
        let loaded = store.load("rt").unwrap();

        assert_eq!(loaded.version, h.version);
        assert_eq!(loaded.goal, h.goal);
        assert_eq!(loaded.prior_decisions.len(), h.prior_decisions.len());
        assert_eq!(loaded.open_questions.len(), h.open_questions.len());
        assert_eq!(loaded.constraints.len(), h.constraints.len());
        assert_eq!(loaded.artifact_refs.len(), h.artifact_refs.len());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
