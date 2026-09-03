//! Canonical AllowSession keys: stable per (tool, target) identity.
use crate::model_tools::is_model_tool;
use crate::{PathPolicy, ToolCall};
use std::path::Path;

/// Stable session key: write = tool + canonical path; shell = tool +
/// canonical workspace + exact command; memory/task = tool + canonical
/// full-argument JSON. `None` for read-only file tools and non-tools.
pub fn allow_session_key(call: &ToolCall, workspace: &Path, policy: &PathPolicy) -> Option<String> {
    match call.name.as_str() {
        "read_file" | "search_files" | "spill_read" => None,
        "write_file" => {
            let raw = call.arguments.get("path")?.as_str()?;
            let canon = policy.resolve(raw, true).ok()?;
            Some(format!("write_file:{}", canon.display()))
        }
        "shell" => {
            let command = call.arguments.get("command")?.as_str()?;
            let ws = workspace
                .canonicalize()
                .unwrap_or_else(|_| workspace.to_path_buf());
            Some(format!("shell:{}:{command}", ws.display()))
        }
        name if is_model_tool(name) => Some(format!("{name}:{}", canonical_json(&call.arguments))),
        _ => None,
    }
}

/// Canonical JSON: object keys sorted recursively for stable keys.
fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let parts: Vec<_> = keys
                .iter()
                .map(|k| format!("{}:{}", json_str(k), canonical_json(&map[*k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(items) => {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(canonical_json)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        _ => value.to_string(),
    }
}

/// Quote a key exactly like serde_json (no untrusted input in keys).
fn json_str(key: &str) -> String {
    serde_json::to_string(key).unwrap_or_else(|_| format!("{key:?}"))
}
