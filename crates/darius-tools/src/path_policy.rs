//! Workspace containment for tool file paths.
use crate::ToolError;
use std::path::{Component, Path, PathBuf};

/// Binds tool file access to one canonical workspace root.
#[derive(Clone, Debug)]
pub struct PathPolicy {
    root: PathBuf,
}

impl PathPolicy {
    /// Bind to a workspace root, creating it if missing, then canonicalize.
    pub fn new(workspace_root: &Path) -> Result<Self, ToolError> {
        std::fs::create_dir_all(workspace_root)?;
        Ok(Self {
            root: workspace_root.canonicalize()?,
        })
    }

    /// Canonical workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve `raw` inside the root. Rejects `..` and symlink escape.
    /// Absolute paths allowed only inside the root. Missing final
    /// component allowed only when `for_create` is set.
    pub fn resolve(&self, raw: &str, for_create: bool) -> Result<PathBuf, ToolError> {
        if for_create {
            return crate::create_path::resolve_create(&self.root, raw);
        }
        if raw.is_empty() {
            return Err(ToolError::InvalidArgs("path required".into()));
        }
        let rel = Path::new(raw);
        if rel.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(ToolError::InvalidArgs("path must not contain '..'".into()));
        }
        let joined = if rel.is_absolute() {
            rel.to_path_buf()
        } else {
            self.root.join(rel)
        };
        let canon = joined
            .canonicalize()
            .map_err(|_| ToolError::InvalidArgs("path does not exist".into()))?;
        if !canon.starts_with(&self.root) {
            return Err(ToolError::InvalidArgs("path escapes workspace".into()));
        }
        Ok(canon)
    }
}
