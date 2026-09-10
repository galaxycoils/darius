//! Real argument schemas offered for the exact model-tool allowlist.
use crate::model::ToolSpec;

pub fn model_tool_specs() -> Vec<ToolSpec> {
    model_tool_specs_with_dynamic(&[])
}

/// Returns static tool specs combined with any session dynamic tool specs.
pub fn model_tool_specs_with_dynamic(extra: &[ToolSpec]) -> Vec<ToolSpec> {
    let mut specs: Vec<ToolSpec> = darius_tools::model_schemas::model_schemas()
        .into_iter()
        .map(|schema| ToolSpec {
            name: schema["name"].as_str().unwrap_or_default().into(),
            description: schema["description"].as_str().unwrap_or_default().into(),
            parameters: schema["parameters"].clone(),
        })
        .collect();

    for item in extra {
        if !specs.iter().any(|s| s.name == item.name) {
            specs.push(item.clone());
        }
    }
    specs
}
