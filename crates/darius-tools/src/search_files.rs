//! Recursive bounded filename/content search in the workspace.
use crate::{PathPolicy, ToolError};

/// Caps: matches per search, bytes scanned per file, entries visited, depth.
pub const MAX_RESULTS: usize = 50;
pub const MAX_SCAN_BYTES: u64 = 512 * 1024;
pub const MAX_VISITED: usize = 20_000;
pub const MAX_DEPTH: usize = 24;

/// Search `subdir` recursively (`name` filename, `content` UTF-8 bytes).
/// Symlinks are never followed, so hits stay in the workspace.
pub fn search(
    policy: &PathPolicy,
    subdir: &str,
    name: Option<&str>,
    content: Option<&str>,
    limit: usize,
) -> Result<Vec<String>, ToolError> {
    search_with_budget(policy, subdir, name, content, limit, MAX_VISITED, MAX_DEPTH)
        .map(|(hits, _)| hits)
}

/// Same as [`search`] with explicit budgets; returns hits plus entries visited.
pub fn search_with_budget(
    policy: &PathPolicy,
    subdir: &str,
    name: Option<&str>,
    content: Option<&str>,
    limit: usize,
    max_visited: usize,
    max_depth: usize,
) -> Result<(Vec<String>, usize), ToolError> {
    if name.is_none() && content.is_none() {
        return Err(ToolError::InvalidArgs("name or content required".into()));
    }
    let cap = limit.clamp(1, MAX_RESULTS);
    let root = policy.resolve(subdir, false)?;
    Ok(crate::search_walk::walk(
        root,
        cap,
        max_visited,
        max_depth,
        |path| {
            crate::search_filter::name_ok(path, name)
                && crate::search_filter::content_ok(path, content)
        },
    ))
}
