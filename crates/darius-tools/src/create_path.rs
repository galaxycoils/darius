//! Create-path resolution inside the workspace root.
use crate::ToolError;
use std::path::{Component, Path, PathBuf};

/// Resolve `raw` for creation: parent must exist under `root`; a
/// pre-existing final-component symlink is rejected.
pub fn resolve_create(root: &Path, raw: &str) -> Result<PathBuf, ToolError> {
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
        root.join(rel)
    };
    let parent = joined.parent().unwrap_or(root).to_path_buf();
    let canon = parent
        .canonicalize()
        .map_err(|_| ToolError::InvalidArgs("parent directory missing".into()))?;
    if !canon.starts_with(root) {
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
}
