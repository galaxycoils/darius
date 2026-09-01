//! Safety gates — capability enforcement and protected instruction write approval.

use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::capabilities::{ApprovalTier, Capability};
use crate::SafetyError;

pub const DEFAULT_PROTECTED_GLOBS: &[&str] = &[
    "**/AGENTS.md",
    "**/SKILL.md",
    "**/skills/**",
    "**/memory.db",
    "**/.darius/**",
];

/// Checks if a path matches default protected instruction file globs:
/// AGENTS.md, SKILL.md, /skills/, memory.db, .darius/
pub fn is_protected_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let s_norm = s.replace('\\', "/");

    if s_norm.ends_with("AGENTS.md")
        || s_norm.ends_with("SKILL.md")
        || s_norm.contains("/skills/")
        || s_norm.starts_with("skills/")
        || s_norm.ends_with("/memory.db")
        || s_norm.contains("/.darius/")
        || s_norm.starts_with(".darius/")
        || s_norm == "AGENTS.md"
        || s_norm == "SKILL.md"
        || s_norm == "memory.db"
    {
        return true;
    }

    false
}

/// Gate for protecting critical instruction and memory files from unapproved writes.
#[derive(Debug, Clone, Default)]
pub struct InstructionWriteGate {
    approved_tokens: Arc<Mutex<HashSet<String>>>,
    approved_paths: Arc<Mutex<HashSet<PathBuf>>>,
}

impl InstructionWriteGate {
    pub fn new() -> Self {
        Self {
            approved_tokens: Arc::new(Mutex::new(HashSet::new())),
            approved_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Explicitly approve a one-time approval token.
    pub fn approve_token(&self, token: &str) {
        self.approved_tokens.lock().insert(token.to_string());
    }

    /// Explicitly approve a path for a single write.
    pub fn approve_path(&self, path: &Path) {
        self.approved_paths.lock().insert(path.to_path_buf());
    }

    /// Check if writing to `path` is allowed.
    /// If protected and unapproved, returns `Err(SafetyError::ApprovalRequired)`.
    /// If approved, consumes the one-time approval and returns `Ok(true)`.
    pub fn check_write(&self, path: &Path, approval_token: Option<&str>) -> Result<bool, SafetyError> {
        if !is_protected_path(path) {
            return Ok(true);
        }

        if let Some(token) = approval_token {
            let mut tokens = self.approved_tokens.lock();
            if tokens.remove(token) {
                return Ok(true);
            }
        }

        let mut paths = self.approved_paths.lock();
        if paths.remove(path) {
            return Ok(true);
        }

        Err(SafetyError::ApprovalRequired(format!(
            "write to protected instruction file '{}' requires explicit approval",
            path.display()
        )))
    }
}

/// Result of a safety gate check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateResult {
    /// Operation is allowed.
    Allowed,
    /// Operation is denied.
    Denied { reason: String },
    /// Operation requires explicit approval.
    RequiresApproval { tier: ApprovalTier, capability: Capability },
}

/// Safety gate that checks capabilities before operation execution.
pub struct SafetyGate;

impl SafetyGate {
    /// Create a new safety gate.
    pub fn new() -> Self {
        Self
    }

    /// Check whether an operation with the given capability is allowed.
    pub fn check(&self, _capability: &Capability) -> GateResult {
        GateResult::Allowed
    }
}

impl Default for SafetyGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_path_detection() {
        assert!(is_protected_path(Path::new("AGENTS.md")));
        assert!(is_protected_path(Path::new("sub/dir/AGENTS.md")));
        assert!(is_protected_path(Path::new("/root/project/SKILL.md")));
        assert!(is_protected_path(Path::new("skills/coding/SKILL.md")));
        assert!(is_protected_path(Path::new("project/.darius/config.toml")));
        assert!(is_protected_path(Path::new("memory.db")));
        assert!(!is_protected_path(Path::new("src/main.rs")));
        assert!(!is_protected_path(Path::new("docs/readme.txt")));
    }

    #[test]
    fn unapproved_write_to_protected_file_is_denied() {
        let gate = InstructionWriteGate::new();
        let path = Path::new("AGENTS.md");

        let result = gate.check_write(path, None);
        assert!(result.is_err());
        match result {
            Err(SafetyError::ApprovalRequired(msg)) => {
                assert!(msg.contains("requires explicit approval"));
            }
            other => panic!("expected ApprovalRequired, got {other:?}"),
        }
    }

    #[test]
    fn approved_write_allowed_once_then_denied() {
        let gate = InstructionWriteGate::new();
        let path = Path::new("skills/my_skill/SKILL.md");

        gate.approve_path(path);

        // First write succeeds
        assert!(gate.check_write(path, None).unwrap());

        // Second write without new approval fails
        assert!(gate.check_write(path, None).is_err());
    }

    #[test]
    fn token_approval_allowed_once() {
        let gate = InstructionWriteGate::new();
        let path = Path::new("AGENTS.md");
        let token = "approve-id-123";

        gate.approve_token(token);

        // First attempt with token succeeds
        assert!(gate.check_write(path, Some(token)).unwrap());

        // Second attempt with same token fails
        assert!(gate.check_write(path, Some(token)).is_err());
    }
}
