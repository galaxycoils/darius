//! JSON schemas for coding file tools plus the output finalizer.
use crate::ToolOutcome;
use std::path::Path;

/// Byte ceiling before tool output spills to disk (32 KiB).
pub const SPILL_CEILING: usize = 32 * 1024;

/// Truncate `full` to `ceiling` bytes on a UTF-8 boundary (byte semantics).
pub fn truncate_preview(full: &str, ceiling: usize) -> String {
    if full.len() <= ceiling {
        return full.to_string();
    }
    let mut end = ceiling.min(full.len());
    while !full.is_char_boundary(end) {
        end -= 1;
    }
    full[..end].to_string()
}

/// Split `full` into preview plus an optional spill file at `ceiling` bytes.
pub fn finalize(full: String, spill_dir: &Path, ceiling: usize) -> ToolOutcome {
    if full.len() <= ceiling {
        return ToolOutcome::Ok {
            preview: full,
            spilled_path: None,
        };
    }
    let preview = truncate_preview(&full, ceiling);
    let path = spill_dir.join(format!("tool_result_{}.txt", uuid::Uuid::new_v4()));
    match std::fs::write(&path, &full) {
        Ok(()) => ToolOutcome::Ok {
            preview,
            spilled_path: Some(path),
        },
        Err(_) => ToolOutcome::Ok {
            preview,
            spilled_path: None,
        },
    }
}

/// JSON schemas for `read_file`, `search_files`, and `write_file`.
pub fn tool_schemas() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"name": "read_file", "description": "Read a workspace file with line pagination.",
            "parameters": {"type": "object", "properties": {
                "path": {"type": "string"}, "offset": {"type": "integer"},
                "limit": {"type": "integer"}}, "required": ["path"]}}),
        serde_json::json!({"name": "search_files", "description": "Recursively search workspace filenames or contents.",
            "parameters": {"type": "object", "properties": {
                "pattern": {"type": "string"}, "content": {"type": "string"},
                "dir": {"type": "string"}, "limit": {"type": "integer"}}, "required": []}}),
        serde_json::json!({"name": "write_file", "description": "Atomically write a workspace file via same-directory rename.",
            "parameters": {"type": "object", "properties": {
                "path": {"type": "string"}, "content": {"type": "string"}},
                "required": ["path", "content"]}}),
    ]
}
