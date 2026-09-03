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
    /// Absolute paths are allowed only when they resolve inside the root.
    /// A missing final component is allowed only when `for_create` is
    /// set (its parent must already exist). A pre-existing final-component
    /// symlink is always rejected on the create path.
    pub fn resolve(&self, raw: &str, for_create: bool) -> Result<PathBuf, ToolError> {
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
        if for_create {
            let parent = joined.parent().unwrap_or(&self.root).to_path_buf();
            let canon = parent
                .canonicalize()
                .map_err(|_| ToolError::InvalidArgs("parent directory missing".into()))?;
            if !canon.starts_with(&self.root) {
                return Err(ToolError::InvalidArgs("path escapes workspace".into()));
            }
            let name = joined
                .file_name()
                .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
            let resolved = canon.join(name);
            if let Ok(meta) = std::fs::symlink_metadata(&resolved)
                && meta.file_type().is_symlink()
            {
                return Err(ToolError::InvalidArgs("path escapes workspace".into()));
            }
            Ok(resolved)
        } else {
            let canon = joined
                .canonicalize()
                .map_err(|_| ToolError::InvalidArgs("path does not exist".into()))?;
            if !canon.starts_with(&self.root) {
                return Err(ToolError::InvalidArgs("path escapes workspace".into()));
            }
            Ok(canon)
        }
    }
}
