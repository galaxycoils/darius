//! Real argument schemas offered for the exact model-tool allowlist.
use crate::model::ToolSpec;

pub fn model_tool_specs() -> Vec<ToolSpec> {
    darius_tools::model_schemas::model_schemas()
        .into_iter()
        .map(|schema| ToolSpec {
            name: schema["name"].as_str().unwrap_or_default().into(),
            description: schema["description"].as_str().unwrap_or_default().into(),
            parameters: schema["parameters"].clone(),
        })
        .collect()
}
