//! Decode provider responses; rejects malformed tool calls, sanitizes errors.
use super::{usage, usage::ProviderUsage, wire_call::decode_call};
use darius_cognitive::{CognitiveError, ModelOutput};
use serde_json::Value;

pub async fn read_response(
    resp: reqwest::Response,
) -> Result<(ModelOutput, Option<ProviderUsage>), CognitiveError> {
    if !resp.status().is_success() {
        // Never echo provider body content: error pages may reflect
        // request secrets or control bytes. Report only static guidance.
        let status = resp.status().as_u16();
        let msg = match status {
            401 | 403 => format!(
                "authentication failed (http {status}): check that the configured api key is set, \
                 then run `darius config init` to configure the provider"
            ),
            429 => format!("rate limited (http {status}): wait a moment, then retry or try again"),
            500..=599 => format!(
                "provider unavailable (http {status} server error): try again or retry shortly"
            ),
            s => format!("provider error (http {s}): check the endpoint and retry"),
        };
        return Err(CognitiveError::Loop(msg));
    }
    let out = match resp.json().await {
        Ok(v) => v,
        Err(_) => {
            return Err(CognitiveError::Loop(
                "invalid response: provider returned an incompatible body; \
                 check the base_url is an openai-compatible endpoint, then retry"
                    .into(),
            ));
        }
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
