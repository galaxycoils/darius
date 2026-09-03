//! Decode one provider tool call; arguments arrive as a JSON string.
use darius_tools::ToolCall;
use serde_json::Value;

fn str_at<'a>(v: &'a Value, ptr: &str) -> Option<&'a str> {
    v.pointer(ptr).and_then(Value::as_str)
}

pub(crate) fn decode_call(tc: &Value) -> Result<ToolCall, String> {
    let id = str_at(tc, "/id").ok_or("tool call without id")?;
    let name = str_at(tc, "/function/name").ok_or("call has no name")?;
    if name.is_empty() {
        return Err("call has no name".into());
    }
    let args = str_at(tc, "/function/arguments").ok_or("bad args")?;
    let arguments = serde_json::from_str(args).map_err(|_| "tool arguments not json")?;
    Ok(ToolCall {
        id: id.into(),
        name: name.into(),
        arguments,
    })
}
