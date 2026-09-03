//! Atomic same-directory file write bound to the workspace.
use crate::{PathPolicy, ToolError};
use std::io::Write as _;

/// Max bytes accepted for a single write (1 MiB).
pub const MAX_WRITE_BYTES: usize = 1024 * 1024;

/// Write `content` to `raw` atomically: temp file in the same
/// directory, then rename. Rejects NUL bytes.
pub fn write_atomic(policy: &PathPolicy, raw: &str, content: &str) -> Result<usize, ToolError> {
    if content.contains('\0') {
        return Err(ToolError::InvalidArgs("binary content rejected".into()));
    }
    if content.len() > MAX_WRITE_BYTES {
        return Err(ToolError::InvalidArgs("content exceeds 1 MiB".into()));
    }
    crate::ensure_dirs::ensure_parent_dirs(policy.root(), raw)?;
    let path = policy.resolve(raw, true)?;
    let parent = path
        .parent()
        .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
    std::fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(&path).map_err(|e| ToolError::Io(e.error))?;
    Ok(content.len())
}
