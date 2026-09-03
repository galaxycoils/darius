//! Complete JSON schemas for the closed model-tool allowlist.

pub fn model_schemas() -> Vec<serde_json::Value> {
    let mut schemas = crate::spec::tool_schemas();
    schemas.extend(crate::schema_memory::schemas());
    schemas.extend(crate::schema_tasks::schemas());
    schemas
}
