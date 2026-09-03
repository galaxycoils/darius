//! Memory and spill tool schemas.
use crate::schema_builder::schema;

pub(crate) fn schemas() -> Vec<serde_json::Value> {
    vec![
        schema(
            "memory_search",
            "Search memory records by text",
            &[("text", "string")],
            &["text"],
        ),
        schema(
            "memory_pack",
            "Build a bounded memory context pack",
            &[],
            &[],
        ),
        schema(
            "spill_read",
            "Read spilled tool output",
            &[
                ("path", "string"),
                ("offset", "integer"),
                ("limit", "integer"),
            ],
            &["path"],
        ),
        schema(
            "memory_remember",
            "Store a memory record",
            &[("body", "string"), ("kind", "string"), ("title", "string")],
            &["body"],
        ),
    ]
}
