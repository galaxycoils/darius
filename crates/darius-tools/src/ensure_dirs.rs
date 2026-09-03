//! Contained creation of missing ancestors for create-paths.
use crate::ToolError;
use std::path::{Component, Path, PathBuf};

/// Join `raw` onto `root` (absolute kept as-is); containment is enforced
/// by canonicalizing the nearest existing ancestor plus a post-check.
fn join_root(root: &Path, raw: &Path) -> PathBuf {
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    }
}

/// Nearest existing ancestor of `parent`, canonicalized and root-checked.
fn verified_base(root: &Path, parent: &Path) -> Result<(), ToolError> {
    let mut probe = parent.to_path_buf();
    loop {
        if probe.exists() {
            let canon = probe
                .canonicalize()
                .map_err(|_| ToolError::InvalidArgs("parent directory missing".into()))?;
            if !canon.starts_with(root) {
                return Err(ToolError::InvalidArgs("path escapes workspace".into()));
            }
            return Ok(());
        }
        if !probe.pop() {
            return Err(ToolError::InvalidArgs("path required".into()));
        }
    }
}

/// Create missing ancestors of `raw` under `root`. Existing ancestors
/// are canonicalized and must stay inside `root` (symlink escape
/// rejected) before anything is created.
pub fn ensure_parent_dirs(root: &Path, raw: &str) -> Result<(), ToolError> {
    if raw.is_empty() {
        return Err(ToolError::InvalidArgs("path required".into()));
    }
    let rel = Path::new(raw);
    if rel.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(ToolError::InvalidArgs("path must not contain '..'".into()));
    }
    let joined = join_root(root, rel);
    let parent = joined
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
    verified_base(root, parent)?;
    std::fs::create_dir_all(parent)?;
    let canon = parent
        .canonicalize()
        .map_err(|_| ToolError::InvalidArgs("parent directory missing".into()))?;
    if !canon.starts_with(root) {
        return Err(ToolError::InvalidArgs("path escapes workspace".into()));
    }
    Ok(())
}
