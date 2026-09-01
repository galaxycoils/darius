//! Isolated worktrees — git worktree management and rollback for safe parallel execution.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("git error: {0}")]
    Git(String),
    #[error("worktree not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    pub id: String,
    pub session_id: Option<String>,
    pub path: PathBuf,
    pub branch: String,
    pub commit: Option<String>,
    pub created_at: u64,
}

/// Worktree manager — creates, tracks, rolls back, and prunes isolated worktrees.
pub struct WorktreeManager {
    root: PathBuf,
    worktrees: Vec<Worktree>,
}

impl WorktreeManager {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: PathBuf::from(root.as_ref()),
            worktrees: Vec::new(),
        }
    }

    /// Create a new isolated worktree.
    pub fn create(
        &mut self,
        branch: &str,
        base_commit: Option<&str>,
    ) -> Result<Worktree, WorktreeError> {
        self.create_for_session(None, branch, base_commit)
    }

    /// Create a new worktree tied to an agent session.
    pub fn create_for_session(
        &mut self,
        session_id: Option<&str>,
        branch: &str,
        base_commit: Option<&str>,
    ) -> Result<Worktree, WorktreeError> {
        let wt_id = format!("wt-{}", uuid::Uuid::new_v4());
        let path = self.root.join(&wt_id);
        if path.exists() {
            let _ = std::fs::remove_dir_all(&path);
        }
        std::fs::create_dir_all(&path)?;

        // Try git worktree add if git repository exists
        let is_git_repo = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .arg("rev-parse")
            .arg("--is-inside-work-tree")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if is_git_repo {
            let mut cmd = Command::new("git");
            cmd.arg("-C").arg(&self.root).arg("worktree").arg("add").arg(&path).arg("-b").arg(branch);
            if let Some(commit) = base_commit {
                cmd.arg(commit);
            }
            let _ = cmd.output();
        }

        let worktree = Worktree {
            id: wt_id,
            session_id: session_id.map(|s| s.to_string()),
            path: path.clone(),
            branch: branch.to_string(),
            commit: base_commit.map(|s| s.to_string()),
            created_at: current_timestamp(),
        };

        self.worktrees.push(worktree.clone());
        Ok(worktree)
    }

    /// Roll back a session's worktree to its initial state.
    pub fn rollback_session(&mut self, session_id: &str) -> Result<(), WorktreeError> {
        let wt = self
            .worktrees
            .iter()
            .find(|w| w.session_id.as_deref() == Some(session_id))
            .ok_or_else(|| WorktreeError::NotFound(format!("session {session_id}")))?;

        if wt.path.exists() {
            // Check if it's a git repo worktree
            let is_git = Command::new("git")
                .arg("-C")
                .arg(&wt.path)
                .arg("status")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if is_git {
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&wt.path)
                    .arg("reset")
                    .arg("--hard")
                    .arg(wt.commit.as_deref().unwrap_or("HEAD"))
                    .output();
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&wt.path)
                    .arg("clean")
                    .arg("-fd")
                    .output();
            } else {
                // If local directory, clean dirty untracked files
                let _ = std::fs::remove_dir_all(&wt.path);
                let _ = std::fs::create_dir_all(&wt.path);
            }
        }

        Ok(())
    }

    /// Remove a worktree.
    pub fn remove(&mut self, path: &Path) -> Result<(), WorktreeError> {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .arg("worktree")
            .arg("remove")
            .arg("-f")
            .arg(path)
            .output();

        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }

        self.worktrees.retain(|w| w.path != path);
        Ok(())
    }

    /// Prune worktrees older than `max_age_seconds` (TTL). Returns pruned worktree IDs.
    pub fn prune_stale(&mut self, max_age_seconds: u64) -> Result<Vec<String>, WorktreeError> {
        let now = current_timestamp();
        let mut pruned_ids = Vec::new();
        let mut to_remove_paths = Vec::new();

        for wt in &self.worktrees {
            if now.saturating_sub(wt.created_at) >= max_age_seconds {
                pruned_ids.push(wt.id.clone());
                to_remove_paths.push(wt.path.clone());
            }
        }

        for path in to_remove_paths {
            let _ = self.remove(&path);
        }

        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .arg("worktree")
            .arg("prune")
            .output();

        Ok(pruned_ids)
    }

    /// List all tracked worktrees.
    pub fn list(&self) -> &[Worktree] {
        &self.worktrees
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
    fn worktree_manager_new() {
        let dir = std::env::temp_dir().join(format!("darius_wt_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let manager = WorktreeManager::new(&dir);
        assert_eq!(manager.list().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worktree_create_and_rollback() {
        let dir = std::env::temp_dir().join(format!("darius_wt_roll_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut manager = WorktreeManager::new(&dir);

        let wt = manager
            .create_for_session(Some("sess-123"), "feature/patch", None)
            .unwrap();
        assert!(wt.path.exists());

        // Create dirty scratch file in worktree
        let scratch = wt.path.join("dirty.txt");
        std::fs::write(&scratch, "dirty content").unwrap();
        assert!(scratch.exists());

        // Rollback session
        manager.rollback_session("sess-123").unwrap();
        assert!(!scratch.exists(), "rollback should clean dirty files");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worktree_prune_stale_by_ttl() {
        let dir = std::env::temp_dir().join(format!("darius_wt_prune_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut manager = WorktreeManager::new(&dir);

        let mut wt1 = manager.create("branch-old", None).unwrap();
        // Simulate wt1 created 2 hours ago
        wt1.created_at = current_timestamp().saturating_sub(7200);
        manager.worktrees[0] = wt1.clone();

        let _wt2 = manager.create("branch-fresh", None).unwrap();
        assert_eq!(manager.list().len(), 2);

        // Prune older than 1 hour (3600s)
        let pruned = manager.prune_stale(3600).unwrap();
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0], wt1.id);
        assert_eq!(manager.list().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
