//! Decode provider responses; rejects malformed tool calls, sanitizes errors.
use super::{usage, usage::ProviderUsage, wire_call::decode_call};
use darius_cognitive::{CognitiveError, ModelOutput};
use serde_json::Value;

pub async fn read_response(
    resp: reqwest::Response,
) -> Result<(ModelOutput, Option<ProviderUsage>), CognitiveError> {
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let detail = resp.text().await.ok().and_then(|t| {
            serde_json::from_str::<Value>(&t).ok().and_then(|v| {
                v.pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
        });
        let msg = match (status, detail) {
            (401 | 403, Some(d)) => format!("authentication failed: {d}"),
            (401 | 403, None) => "authentication failed".into(),
            (429, Some(d)) => format!("rate limited: {d}"),
            (429, None) => "rate limited".into(),
            (500..=599, Some(d)) => format!("provider unavailable ({status}): {d}"),
            (500..=599, None) => "provider unavailable".into(),
            (s, Some(d)) => format!("provider error ({s}): {d}"),
            (s, None) => format!("provider error ({s})"),
        };
        return Err(CognitiveError::Loop(msg));
    }
    let out = match resp.json().await {
        Ok(v) => v,
        Err(_) => return Err(CognitiveError::Loop("invalid response".into())),
    };
    let output = decode_response(&out).map_err(CognitiveError::InvalidPlan)?;
    Ok((output, usage::parse(&out)))
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
