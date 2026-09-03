//! Complete JSON schemas for the closed model-tool allowlist.
use serde_json::{Value, json};

pub fn model_schemas() -> Vec<Value> {
    let mut schemas = crate::spec::tool_schemas();
    schemas.extend(crate::schema_memory::schemas());
    schemas.extend(crate::schema_tasks::schemas());
    for item in &mut schemas {
        item["parameters"]["additionalProperties"] = json!(false);
        match item["name"].as_str().unwrap_or_default() {
            "read_file" => {
                item["parameters"]["properties"]["offset"]["minimum"] = json!(1);
                item["parameters"]["properties"]["limit"]["minimum"] = json!(1);
                item["parameters"]["properties"]["limit"]["maximum"] =
                    json!(crate::read_file::MAX_LIMIT);
            }
            "search_files" => {
                item["parameters"]["properties"]["limit"]["minimum"] = json!(1);
                item["parameters"]["properties"]["limit"]["maximum"] =
                    json!(crate::search_files::MAX_RESULTS);
                item["parameters"]["anyOf"] = json!([
                    {"required": ["content"]},
                    {"required": ["pattern"]}
                ]);
            }
            "spill_read" => {
                item["parameters"]["properties"]["offset"]["minimum"] = json!(0);
                item["parameters"]["properties"]["limit"]["minimum"] = json!(1);
                item["parameters"]["properties"]["limit"]["maximum"] =
                    json!(crate::PREVIEW_CEILING);
            }
            "memory_remember" => {
                item["parameters"]["properties"]["kind"]["enum"] =
                    json!(["fact", "decision", "preference", "episode", "note"]);
            }
            _ => {}
        }
    }
    schemas
}
