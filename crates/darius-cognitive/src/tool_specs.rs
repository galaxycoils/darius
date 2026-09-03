//! Real tool specs offered to the model: exactly the visible allowlist.
use crate::model::ToolSpec;
const SPECS: &[(&str, &str)] = &[
    ("read_file", "Read a file from the workspace"),
    ("search_files", "Search file contents in the workspace"),
    ("memory_search", "Search memory records by text"),
    ("memory_pack", "Build a bounded memory context pack"),
    ("task_list", "List tasks on the shared board"),
    ("spill_read", "Read a page of spilled tool output"),
    ("write_file", "Write a file (approval required)"),
    (
        "memory_remember",
        "Store a memory record (approval required)",
    ),
    ("task_add", "Add a task to the shared board"),
    ("task_complete", "Complete a task on the shared board"),
    ("shell", "Run a shell command (approval required)"),
];
pub fn model_tool_specs() -> Vec<ToolSpec> {
    SPECS
        .iter()
        .map(|(name, desc)| ToolSpec {
            name: (*name).into(),
            description: (*desc).into(),
            parameters: serde_json::json!({"type": "object"}),
        })
        .collect()
}
