//! Minimal tool ACI — registry, spill, TOOL line protocol.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("memory error: {0}")]
    Memory(#[from] darius_memory::MemoryError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolOutcome {
    Ok {
        preview: String,
        spilled_path: Option<PathBuf>,
    },
    Err {
        message: String,
    },
}

const DEFAULT_PREVIEW_CEILING: usize = 32_768;

/// Tool registry with disk spill for large results.
pub struct ToolRegistry {
    spill_dir: PathBuf,
    preview_ceiling: usize,
    // Tool handlers: name -> handler function
    handlers: HashMap<String, Box<dyn Fn(&ToolCall) -> Result<ToolOutcome, ToolError>>>,
}

impl ToolRegistry {
    pub fn new(profile_dir: &Path) -> Result<Self, ToolError> {
        let spill_dir = profile_dir.join("tool_results");
        std::fs::create_dir_all(&spill_dir)?;
        Ok(Self {
            spill_dir,
            preview_ceiling: DEFAULT_PREVIEW_CEILING,
            handlers: HashMap::new(),
        })
    }

    pub fn register<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(&ToolCall) -> Result<ToolOutcome, ToolError> + 'static,
    {
        self.handlers.insert(name.into(), Box::new(handler));
    }

    pub fn execute(&self, call: &ToolCall) -> ToolOutcome {
        match self.handlers.get(&call.name) {
            Some(handler) => match handler(call) {
                Ok(outcome) => outcome,
                Err(e) => ToolOutcome::Err {
                    message: e.to_string(),
                },
            },
            None => ToolOutcome::Err {
                message: format!("unknown tool: {}", call.name),
            },
        }
    }

    /// Spill content to disk if it exceeds the preview ceiling.
    /// Returns (preview, spilled_path) where preview is at most `preview_ceiling` bytes.
    pub fn spill(&self, content: &str) -> (String, Option<PathBuf>) {
        if content.len() <= self.preview_ceiling {
            return (content.to_string(), None);
        }

        let preview = content
            .chars()
            .take(self.preview_ceiling)
            .collect::<String>();
        let filename = format!("tool_result_{}.txt", uuid::Uuid::new_v4());
        let path = self.spill_dir.join(&filename);

        match std::fs::write(&path, content) {
            Ok(()) => (preview, Some(path)),
            Err(_) => (preview, None),
        }
    }

    pub fn set_preview_ceiling(&mut self, ceiling: usize) {
        self.preview_ceiling = ceiling;
    }
}

/// Parse a TOOL line from model output.
/// Format: TOOL {"name":"memory_search","arguments":{"text":"..."}}
pub fn parse_tool_line(line: &str) -> Option<ToolCall> {
    let line = line.trim();
    if !line.starts_with("TOOL ") {
        return None;
    }

    let json_str = &line[5..].trim();
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let name = value.get("name")?.as_str()?.to_string();
    let arguments = value
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    Some(ToolCall {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        arguments,
    })
}

/// Extract all TOOL calls from mixed prose.
pub fn extract_tool_calls(text: &str) -> Vec<ToolCall> {
    text.lines().filter_map(parse_tool_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tool_returns_error() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let registry = ToolRegistry::new(&dir).unwrap();

        let call = ToolCall {
            id: "test-1".into(),
            name: "nonexistent".into(),
            arguments: serde_json::Value::Null,
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Err { message } => assert!(message.contains("unknown tool")),
            _ => panic!("expected error"),
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn large_payload_spills_to_disk() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new(&dir).unwrap();
        registry.set_preview_ceiling(100);

        let large_content = "x".repeat(500);
        let (preview, spilled_path) = registry.spill(&large_content);

        assert_eq!(preview.len(), 100);
        assert!(spilled_path.is_some());
        assert!(
            spilled_path.as_ref().unwrap().exists(),
            "spilled file should exist"
        );

        let spilled_content = std::fs::read_to_string(spilled_path.as_ref().unwrap()).unwrap();
        assert_eq!(spilled_content.len(), 500);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn small_payload_does_not_spill() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new(&dir).unwrap();
        registry.set_preview_ceiling(100);

        let small_content = "hello world";
        let (preview, spilled_path) = registry.spill(small_content);

        assert_eq!(preview, small_content);
        assert!(spilled_path.is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_tool_line_valid() {
        let line = r#"TOOL {"name":"memory_search","arguments":{"text":"wal"}}"#;
        let call = parse_tool_line(line).unwrap();
        assert_eq!(call.name, "memory_search");
        assert_eq!(call.arguments.get("text").unwrap().as_str().unwrap(), "wal");
    }

    #[test]
    fn parse_tool_line_invalid() {
        assert!(parse_tool_line("not a tool line").is_none());
        assert!(parse_tool_line("TOOL not json").is_none());
    }

    #[test]
    fn extract_tool_calls_from_prose() {
        let text = r#"
Let me search for that information.
TOOL {"name":"memory_search","arguments":{"text":"wal"}}
Here are the results...
TOOL {"name":"memory_remember","arguments":{"body":"important fact"}}
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "memory_search");
        assert_eq!(calls[1].name, "memory_remember");
    }
}
