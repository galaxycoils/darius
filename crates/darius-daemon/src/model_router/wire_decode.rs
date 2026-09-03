//! Decode provider responses; rejects malformed tool calls, sanitizes errors.
use darius_cognitive::{CognitiveError, ModelOutput};
use darius_tools::ToolCall;
use serde_json::Value;

pub async fn read_response(resp: reqwest::Response) -> Result<ModelOutput, CognitiveError> {
    if !resp.status().is_success() {
        return Err(match resp.status().as_u16() {
            401 | 403 => CognitiveError::Loop("authentication failed".into()),
            429 => CognitiveError::Loop("rate limited".into()),
            500..=599 => CognitiveError::Loop("provider unavailable".into()),
            _ => CognitiveError::Loop("provider error".into()),
        });
    }
    let out = match resp.json().await {
        Ok(v) => v,
        Err(_) => return Err(CognitiveError::Loop("invalid response".into())),
    };
    decode_response(&out).map_err(CognitiveError::InvalidPlan)
}

pub fn decode_response(body: &Value) -> Result<ModelOutput, String> {
    let msg = body.pointer("/choices/0/message");
    let msg = msg.ok_or("missing choices[0].message")?;
    let content = msg.get("content");
    let content = content.and_then(Value::as_str);
    let content = content.map(str::to_owned);
    let raw = msg.get("tool_calls");
    let arr = match raw {
        None => None,
        Some(v) => Some(v.as_array().ok_or("bad tool_calls")?),
    };
    let mut calls = Vec::new();
    for tc in arr.cloned().unwrap_or_default() {
        calls.push(decode_call(&tc)?);
    }
    let out = ModelOutput {
        content,
        tool_calls: calls,
    };
    out.validate().map_err(|e| e.to_string())?;
    Ok(out)
}

fn str_at<'a>(v: &'a Value, ptr: &str) -> Option<&'a str> {
    v.pointer(ptr).and_then(Value::as_str)
}

fn decode_call(tc: &Value) -> Result<ToolCall, String> {
    let id = str_at(tc, "/id").ok_or("tool call without id")?;
    let name = str_at(tc, "/function/name").ok_or("call has no name")?;
    let args = str_at(tc, "/function/arguments").ok_or("bad args")?;
    let parsed = serde_json::from_str(args);
    let arguments = parsed.map_err(|_| "tool arguments not json")?;
    Ok(ToolCall {
        id: id.into(),
        name: name.into(),
        arguments,
    })
}
