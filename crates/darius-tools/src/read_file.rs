//! Line-paged UTF-8 file read bound to the workspace.
use crate::{PathPolicy, ToolError};

/// Default page size (lines) when the caller passes no limit.
pub const DEFAULT_LIMIT: u64 = 200;
/// Hard cap on lines served per page.
pub const MAX_LIMIT: u64 = 1000;

/// Read `raw` as UTF-8 text, rejecting binary, returning lines
/// `[offset, offset+limit)` with a 1-based offset.
pub fn read_paged(
    policy: &PathPolicy,
    raw: &str,
    offset: u64,
    limit: u64,
) -> Result<String, ToolError> {
    let path = policy.resolve(raw, false)?;
    let bytes = std::fs::read(&path)?;
    if bytes.contains(&0) {
        return Err(ToolError::InvalidArgs("binary file rejected".into()));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| ToolError::InvalidArgs("non-UTF8 file rejected".into()))?;
    let limit = limit.clamp(1, MAX_LIMIT) as usize;
    let start = offset.max(1) as usize - 1;
    Ok(text
        .lines()
        .skip(start)
        .take(limit)
        .collect::<Vec<_>>()
        .join("\n"))
}
