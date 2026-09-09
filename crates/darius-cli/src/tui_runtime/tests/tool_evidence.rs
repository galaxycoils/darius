use super::*;
pub(super) fn reply(id: &str, name: &str, args: serde_json::Value) -> serde_json::Value {
    json!({"role":"assistant","tool_calls":[{"id":id,"type":"function",
        "function":{"name":name,"arguments":args.to_string()}}]})
}
pub(super) fn results(request: &serde_json::Value) -> Vec<&serde_json::Value> {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .collect()
}
pub(super) fn result<'a>(request: &'a serde_json::Value, id: &str, name: &str) -> &'a str {
    let messages = request["messages"].as_array().unwrap();
    assert!(messages.iter().any(|m| m["role"] == "assistant"
        && m["tool_calls"].as_array().is_some_and(|calls| {
            calls
                .iter()
                .any(|c| c["id"] == id && c["function"]["name"] == name)
        })));
    let matched: Vec<_> = results(request)
        .into_iter()
        .filter(|m| m["tool_call_id"] == id)
        .collect();
    assert_eq!(matched.len(), 1, "missing or duplicate correlated result");
    matched[0]["content"].as_str().unwrap()
}
pub(super) fn finish(h: &mut Harness) -> Vec<UiEvent> {
    let events = h.until(|e| matches!(e, UiEvent::Done | UiEvent::PermissionRequired { .. }));
    assert!(matches!(events.last(), Some(UiEvent::Done)), "{events:?}");
    assert!(
        !events.iter().any(|e| matches!(
            e,
            UiEvent::Error { .. } | UiEvent::ToolEnd { ok: false, .. }
        )),
        "{events:?}"
    );
    events
}
