//! Isolated worktrees — git worktree management for safe parallel work.

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

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    pub commit: Option<String>,
}

/// Worktree manager — creates and manages isolated git worktrees.
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

    /// Create a new worktree for a branch.
    pub fn create(
        &mut self,
        branch: &str,
        base_commit: Option<&str>,
    ) -> Result<Worktree, WorktreeError> {
        let path = self.root.join(format!("wt-{}", branch.replace('/', "-")));
        if path.exists() {
            return Err(WorktreeError::Git(format!(
                "worktree path already exists: {}",
                path.display()
            )));
        }

        let mut cmd = Command::new("git");
        cmd.arg("worktree").arg("add").arg(&path).arg(branch);
        if let Some(commit) = base_commit {
            cmd.arg(commit);
        }

        let output = cmd.output()?;
        if !output.status.success() {
            return Err(WorktreeError::Git(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let worktree = Worktree {
            path: path.clone(),
            branch: branch.to_string(),
            commit: base_commit.map(|s| s.to_string()),
        };
        self.worktrees.push(worktree.clone());
        Ok(worktree)
    }

    /// Remove a worktree.
    pub fn remove(&mut self, path: &Path) -> Result<(), WorktreeError> {
        let output = Command::new("git")
            .arg("worktree")
            .arg("remove")
            .arg(path)
            .output()?;

        if !output.status.success() {
            return Err(WorktreeError::Git(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        self.worktrees.retain(|w| w.path != path);
        Ok(())
    }

    /// List all worktrees.
    pub fn list(&self) -> &[Worktree] {
        &self.worktrees
    }

    /// Prune stale worktrees.
    pub fn prune(&self) -> Result<(), WorktreeError> {
        let output = Command::new("git").arg("worktree").arg("prune").output()?;

        if !output.status.success() {
            return Err(WorktreeError::Git(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(())
    }
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
}
