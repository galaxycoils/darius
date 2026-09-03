//! Task-board and shell tool schemas.
use crate::schema_builder::schema;

pub(crate) fn schemas() -> Vec<serde_json::Value> {
    vec![
        schema("task_list", "List tasks", &[], &[]),
        schema("task_add", "Add a task", &[("title", "string")], &["title"]),
        schema(
            "task_complete",
            "Complete a task",
            &[("id", "string")],
            &["id"],
        ),
        schema(
            "shell",
            "Run a shell command",
            &[("command", "string")],
            &["command"],
        ),
    ]
}
