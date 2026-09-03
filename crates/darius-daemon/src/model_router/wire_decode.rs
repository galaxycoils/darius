//! Decode provider responses; rejects malformed tool calls, sanitizes errors.
use super::wire_call::decode_call;
use darius_cognitive::{CognitiveError, ModelOutput};
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
    let content = msg
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_owned);
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
    if out.content.is_none() && out.tool_calls.is_empty() {
        return Err("empty response".into());
    }
    out.validate().map_err(|e| e.to_string())?;
    Ok(out)
}
