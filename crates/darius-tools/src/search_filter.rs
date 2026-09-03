//! Filename/content match filters for workspace search.
use crate::search_files::MAX_SCAN_BYTES;
use std::path::Path;

/// Filename filter; `None` matches everything.
pub fn name_ok(path: &Path, name: Option<&str>) -> bool {
    let Some(n) = name else { return true };
    path.file_name()
        .is_some_and(|f| f.to_string_lossy().contains(n))
}

/// Content filter; `None` matches, binary and oversized files never do.
/// Size is gated via metadata BEFORE reading, so oversized files are
/// skipped without a full load (TOCTOU growth still re-checked after).
pub fn content_ok(path: &Path, needle: Option<&str>) -> bool {
    let Some(needle) = needle else { return true };
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_SCAN_BYTES {
        return false;
    }
    match std::fs::read(path) {
        Err(_) => false,
        Ok(bytes) if bytes.len() as u64 > MAX_SCAN_BYTES || bytes.contains(&0) => false,
        Ok(bytes) => String::from_utf8(bytes).is_ok_and(|t| t.contains(needle)),
    }
}
