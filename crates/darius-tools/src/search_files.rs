//! Recursive bounded filename/content search in the workspace.
use crate::{PathPolicy, ToolError};
use std::path::Path;

/// Caps: matches per search, and bytes scanned per file for content.
pub const MAX_RESULTS: usize = 50;
pub const MAX_SCAN_BYTES: u64 = 512 * 1024;

/// Search `subdir` recursively (`name` filename, `content` UTF-8 bytes).
/// Symlinks are never followed, so hits stay in the workspace.
pub fn search(
    policy: &PathPolicy,
    subdir: &str,
    name: Option<&str>,
    content: Option<&str>,
    limit: usize,
) -> Result<Vec<String>, ToolError> {
    if name.is_none() && content.is_none() {
        return Err(ToolError::InvalidArgs("name or content required".into()));
    }
    let cap = limit.clamp(1, MAX_RESULTS);
    let (mut out, mut stack) = (Vec::new(), vec![policy.resolve(subdir, false)?]);
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_symlink() {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else if name_ok(&path, name) && content_ok(&path, content) {
                    out.push(path.display().to_string());
                    if out.len() >= cap {
                        return Ok(out);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Filename filter; `None` matches everything.
fn name_ok(path: &Path, name: Option<&str>) -> bool {
    let Some(n) = name else { return true };
    path.file_name()
        .is_some_and(|f| f.to_string_lossy().contains(n))
}

/// Content filter; `None` matches, binary and oversized files never do.
fn content_ok(path: &Path, needle: Option<&str>) -> bool {
    let Some(needle) = needle else { return true };
    match std::fs::read(path) {
        Err(_) => false,
        Ok(bytes) if bytes.len() as u64 > MAX_SCAN_BYTES || bytes.contains(&0) => false,
        Ok(bytes) => String::from_utf8(bytes).is_ok_and(|t| t.contains(needle)),
    }
}
