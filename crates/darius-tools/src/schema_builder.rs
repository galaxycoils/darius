//! Small JSON-schema builder shared by model tool definitions.

pub(crate) fn schema(
    name: &str,
    description: &str,
    entries: &[(&str, &str)],
    required: &[&str],
) -> serde_json::Value {
    let properties: serde_json::Map<String, serde_json::Value> = entries
        .iter()
        .map(|(key, kind)| ((*key).into(), serde_json::json!({"type": kind})))
        .collect();
    serde_json::json!({
        "name": name,
        "description": description,
        "parameters": {
            "type": "object",
            "properties": properties,
            "required": required
        }
    })
}
