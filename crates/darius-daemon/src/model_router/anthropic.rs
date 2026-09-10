//! Anthropic Messages API client (AnthropicModel) implementing AsyncModel.
use crate::model_router::{BudgetEnforcer, BudgetScope, usage, usage::ProviderUsage};
use darius_cognitive::{AsyncModel, CognitiveError, Message, ModelOutput, ToolSpec, TurnContext};
use serde_json::{Value, json};

const MAX_OUTPUT_TOKENS: u64 = 4096;

pub struct AnthropicModel {
    pub model: String,
    pub base_url: String,
    pub key_env: String,
    pub client: reqwest::Client,
    pub budget: BudgetEnforcer,
    pub scope: BudgetScope,
}

impl AnthropicModel {
    pub fn new(
        model: String,
        base_url: String,
        key_env: String,
        budget: BudgetEnforcer,
        scope: BudgetScope,
    ) -> Result<Self, crate::model_router::RouterError> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|_| {
                crate::model_router::RouterError::Provider("http client init failed".into())
            })?;
        Ok(Self {
            model,
            base_url: base_url.trim_end_matches('/').to_owned(),
            key_env,
            client,
            budget,
            scope,
        })
    }
}

/// Encode domain messages and tools into an Anthropic Messages API JSON payload.
pub fn encode_anthropic_request(
    model: &str,
    messages: &[Message],
    tools: &[ToolSpec],
    max_tokens: u64,
) -> Value {
    let mut system_parts = Vec::new();
    let mut anthropic_msgs: Vec<Value> = Vec::new();

    for msg in messages {
        match msg {
            Message::System { content } => {
                if !content.trim().is_empty() {
                    system_parts.push(content.clone());
                }
            }
            Message::User { content } => {
                let block = json!({
                    "type": "text",
                    "text": content,
                });
                if let Some(last) = anthropic_msgs.last_mut()
                    && last.get("role").and_then(|r| r.as_str()) == Some("user")
                    && let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut())
                {
                    arr.push(block);
                    continue;
                }
                anthropic_msgs.push(json!({
                    "role": "user",
                    "content": [block],
                }));
            }
            Message::Assistant {
                content,
                tool_calls,
            } => {
                let mut blocks = Vec::new();
                if let Some(text) = content
                    && !text.is_empty()
                {
                    blocks.push(json!({
                        "type": "text",
                        "text": text,
                    }));
                }
                for call in tool_calls {
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": call.arguments,
                    }));
                }
                if blocks.is_empty() {
                    blocks.push(json!({
                        "type": "text",
                        "text": "",
                    }));
                }
                anthropic_msgs.push(json!({
                    "role": "assistant",
                    "content": blocks,
                }));
            }
            Message::Tool {
                tool_call_id,
                content,
                ..
            } => {
                let block = json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": content,
                });
                if let Some(last) = anthropic_msgs.last_mut()
                    && last.get("role").and_then(|r| r.as_str()) == Some("user")
                    && let Some(arr) = last.get_mut("content").and_then(|c| c.as_array_mut())
                {
                    arr.push(block);
                    continue;
                }
                anthropic_msgs.push(json!({
                    "role": "user",
                    "content": [block],
                }));
            }
        }
    }

    let mut map = serde_json::Map::new();
    map.insert("model".into(), json!(model));
    map.insert("max_tokens".into(), json!(max_tokens));

    if !system_parts.is_empty() {
        map.insert("system".into(), json!(system_parts.join("\n\n")));
    }

    map.insert("messages".into(), Value::Array(anthropic_msgs));

    if !tools.is_empty() {
        let anthropic_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();
        map.insert("tools".into(), Value::Array(anthropic_tools));
    }

    Value::Object(map)
}

/// Encode domain messages and tools into an Anthropic Messages API streaming payload.
pub fn encode_anthropic_stream_request(
    model: &str,
    messages: &[Message],
    tools: &[ToolSpec],
    max_tokens: u64,
) -> Value {
    let mut val = encode_anthropic_request(model, messages, tools, max_tokens);
    if let Some(map) = val.as_object_mut() {
        map.insert("stream".into(), json!(true));
    }
    val
}
pub fn decode_anthropic_response(body: &Value) -> Result<ModelOutput, String> {
    let content_arr = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing content array in Anthropic response".to_string())?;

    let mut text_acc = String::new();
    let mut tool_calls = Vec::new();

    for block in content_arr {
        let block_type = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    text_acc.push_str(text);
                }
            }
            "tool_use" => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "tool_use missing id".to_string())?
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "tool_use missing name".to_string())?
                    .to_string();
                let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                tool_calls.push(darius_tools::ToolCall {
                    id,
                    name,
                    arguments: input,
                });
            }
            _ => {}
        }
    }

    let content = if text_acc.is_empty() && !tool_calls.is_empty() {
        None
    } else {
        Some(text_acc)
    };

    let out = ModelOutput {
        content,
        tool_calls,
    };
    out.validate().map_err(|e| e.to_string())?;
    Ok(out)
}
/// Parse usage tokens from an Anthropic Messages API response.
pub fn parse_anthropic_usage(body: &Value) -> Option<ProviderUsage> {
    usage::parse(body)
}
#[async_trait::async_trait]
impl AsyncModel for AnthropicModel {
    async fn complete(
        &mut self,
        messages: &[Message],
        tools: &[ToolSpec],
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        let is_local = self.base_url.contains("localhost")
            || self.base_url.contains("127.0.0.1")
            || self.base_url.contains("0.0.0.0")
            || self.key_env.eq_ignore_ascii_case("NONE");
        let key = match std::env::var(&self.key_env) {
            Ok(k) if !k.trim().is_empty() => k,
            _ if is_local => "ollama".to_string(),
            _ => {
                return Err(CognitiveError::Loop(format!(
                    "API key environment variable '{}' is not set; export it or configure Anthropic",
                    self.key_env
                )));
            }
        };

        let input = usage::estimate_input(messages, tools).max(1);
        let mut reservation = self
            .budget
            .reserve(self.scope, input, MAX_OUTPUT_TOKENS)
            .map_err(|error| CognitiveError::Loop(error.to_string()))?;

        let body =
            encode_anthropic_request(&self.model, messages, tools, reservation.output_limit());

        let url = if self.base_url.ends_with("/messages") {
            self.base_url.clone()
        } else {
            format!("{}/messages", self.base_url.trim_end_matches('/'))
        };

        let fetch = async {
            reservation.mark_dispatched();
            let response = self
                .client
                .post(&url)
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        CognitiveError::Loop(
                            "provider request timed out: check network connectivity, \
                             then retry or try again"
                                .into(),
                        )
                    } else {
                        CognitiveError::Loop(
                            "provider request failed: check network connectivity and base_url, \
                             then retry or try again"
                                .into(),
                        )
                    }
                })?;

            let status = response.status().as_u16();
            if !response.status().is_success() {
                let msg = match status {
                    401 => format!(
                        "authentication failed (http 401): invalid API key in {}",
                        self.key_env
                    ),
                    403 => {
                        "access forbidden (http 403): check your Anthropic API permissions".into()
                    }
                    404 => format!(
                        "not found (http 404): model '{}' or endpoint '{}' not found",
                        self.model, url
                    ),
                    429 => "rate limited (http 429): too many requests or Anthropic quota exceeded"
                        .into(),
                    529 => "Anthropic service is overloaded (http 529): retry shortly".into(),
                    500..=599 => format!(
                        "provider unavailable (http {status} server error): try again shortly"
                    ),
                    _ => format!("Anthropic request failed with status http {status}"),
                };
                return Err(CognitiveError::Loop(msg));
            }

            let out = match response.json::<Value>().await {
                Ok(v) => v,
                Err(_) => {
                    return Err(CognitiveError::Loop(
                        "invalid response: Anthropic provider returned an incompatible JSON body"
                            .into(),
                    ));
                }
            };

            let output = decode_anthropic_response(&out).map_err(CognitiveError::InvalidPlan)?;
            Ok((output, parse_anthropic_usage(&out)))
        };

        let cancel = ctx.token();
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CognitiveError::Cancelled),
            output = fetch => output,
            _ = tokio::time::sleep(ctx.deadline_duration()) => {
                Err(CognitiveError::Loop(
                    "turn deadline exceeded after 60s (timed out): the provider did not respond; \
                     retry, check provider latency, or try again"
                        .into(),
                ))
            },
        };

        if let Ok((_, Some(reported))) = &result {
            reservation.reconcile(reported.total_tokens);
        } else if reservation.was_dispatched() {
            reservation.conservative_charge();
        }
        result.map(|(output, _)| output)
    }

    async fn complete_stream(
        &mut self,
        messages: &[Message],
        tools: &[ToolSpec],
        sink: &dyn darius_cognitive::EventSink,
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        use tokio_stream::StreamExt;
        let is_local = self.base_url.contains("localhost")
            || self.base_url.contains("127.0.0.1")
            || self.base_url.contains("0.0.0.0")
            || self.key_env.eq_ignore_ascii_case("NONE");
        let key = match std::env::var(&self.key_env) {
            Ok(k) if !k.trim().is_empty() => k,
            _ if is_local => "ollama".to_string(),
            _ => {
                return Err(CognitiveError::Loop(format!(
                    "API key environment variable '{}' is not set; export it or configure Anthropic",
                    self.key_env
                )));
            }
        };

        let input = usage::estimate_input(messages, tools).max(1);
        let mut reservation = self
            .budget
            .reserve(self.scope, input, MAX_OUTPUT_TOKENS)
            .map_err(|error| CognitiveError::Loop(error.to_string()))?;

        let body = encode_anthropic_stream_request(
            &self.model,
            messages,
            tools,
            reservation.output_limit(),
        );

        let url = if self.base_url.ends_with("/messages") {
            self.base_url.clone()
        } else {
            format!("{}/messages", self.base_url.trim_end_matches('/'))
        };

        let fetch = async {
            reservation.mark_dispatched();
            let response = self
                .client
                .post(&url)
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        CognitiveError::Loop(
                            "provider request timed out: check network connectivity, \
                             then retry or try again"
                                .into(),
                        )
                    } else {
                        CognitiveError::Loop(
                            "provider request failed: check network connectivity and base_url, \
                             then retry or try again"
                                .into(),
                        )
                    }
                })?;

            let status = response.status().as_u16();
            if !response.status().is_success() {
                let msg = match status {
                    401 => format!(
                        "authentication failed (http 401): invalid API key in {}",
                        self.key_env
                    ),
                    403 => {
                        "access forbidden (http 403): check your Anthropic API permissions".into()
                    }
                    404 => format!(
                        "not found (http 404): model '{}' or endpoint '{}' not found",
                        self.model, url
                    ),
                    429 => "rate limited (http 429): too many requests or Anthropic quota exceeded"
                        .into(),
                    529 => "Anthropic service is overloaded (http 529): retry shortly".into(),
                    500..=599 => format!(
                        "provider unavailable (http {status} server error): try again shortly"
                    ),
                    _ => format!("Anthropic request failed with status http {status}"),
                };
                return Err(CognitiveError::Loop(msg));
            }

            let is_json = response
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .map(|ct| ct.contains("application/json"))
                .unwrap_or(false);

            if is_json {
                let out = match response.json::<Value>().await {
                    Ok(v) => v,
                    Err(_) => {
                        return Err(CognitiveError::Loop(
                            "invalid response: Anthropic provider returned an incompatible JSON body".into(),
                        ));
                    }
                };
                let output =
                    decode_anthropic_response(&out).map_err(CognitiveError::InvalidPlan)?;
                if output.tool_calls.is_empty()
                    && let Some(text) = &output.content
                {
                    sink.emit(darius_cognitive::UiEvent::AssistantDelta { text: text.clone() });
                }
                return Ok((output, parse_anthropic_usage(&out)));
            }

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut text_acc = String::new();
            let mut partial_tools: std::collections::BTreeMap<usize, (String, String, String)> =
                std::collections::BTreeMap::new();
            let mut current_input_tokens = 0u64;
            let mut current_output_tokens = 0u64;

            while let Some(chunk_res) = stream.next().await {
                let chunk = chunk_res
                    .map_err(|e| CognitiveError::Loop(format!("stream read error: {e}")))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(idx) = buffer.find('\n') {
                    let line = buffer[..idx].trim().to_string();
                    buffer = buffer[idx + 1..].to_string();
                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    if let Some(data) = line.strip_prefix("data: ")
                        && let Ok(v) = serde_json::from_str::<Value>(data)
                    {
                        let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or_default();
                        match event_type {
                            "message_start" => {
                                if let Some(usage) = v.pointer("/message/usage") {
                                    current_input_tokens = usage
                                        .get("input_tokens")
                                        .and_then(|i| i.as_u64())
                                        .unwrap_or(0);
                                }
                            }
                            "content_block_start" => {
                                let idx =
                                    v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                if let Some(block) = v.get("content_block")
                                    && block.get("type").and_then(|t| t.as_str())
                                        == Some("tool_use")
                                {
                                    let id = block
                                        .get("id")
                                        .and_then(|i| i.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let name = block
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    partial_tools.insert(idx, (id, name, String::new()));
                                }
                            }
                            "content_block_delta" => {
                                let idx =
                                    v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                if let Some(delta) = v.get("delta") {
                                    let delta_type = delta
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or_default();
                                    match delta_type {
                                        "text_delta" => {
                                            if let Some(text) =
                                                delta.get("text").and_then(|t| t.as_str())
                                            {
                                                text_acc.push_str(text);
                                                sink.emit(
                                                    darius_cognitive::UiEvent::AssistantDelta {
                                                        text: text.to_string(),
                                                    },
                                                );
                                            }
                                        }
                                        "input_json_delta" => {
                                            if let Some(partial_json) =
                                                delta.get("partial_json").and_then(|p| p.as_str())
                                                && let Some(entry) = partial_tools.get_mut(&idx)
                                            {
                                                entry.2.push_str(partial_json);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "message_delta" => {
                                if let Some(usage) = v.get("usage") {
                                    current_output_tokens = usage
                                        .get("output_tokens")
                                        .and_then(|o| o.as_u64())
                                        .unwrap_or(0);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            let mut tool_calls = Vec::new();
            for (_, (id, name, args_str)) in partial_tools {
                let parsed_args = serde_json::from_str::<serde_json::Value>(&args_str)
                    .unwrap_or_else(|_| json!({}));
                tool_calls.push(darius_tools::ToolCall {
                    id,
                    name,
                    arguments: parsed_args,
                });
            }

            let content = if text_acc.is_empty() && !tool_calls.is_empty() {
                None
            } else {
                Some(text_acc)
            };

            let out = ModelOutput {
                content,
                tool_calls,
            };
            out.validate()
                .map_err(|e| CognitiveError::InvalidPlan(e.to_string()))?;
            let usage = ProviderUsage {
                total_tokens: current_input_tokens + current_output_tokens,
            };
            Ok((out, Some(usage)))
        };

        let cancel = ctx.token();
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(CognitiveError::Cancelled),
            output = fetch => output,
            _ = tokio::time::sleep(ctx.deadline_duration()) => {
                Err(CognitiveError::Loop(
                    "turn deadline exceeded after 60s (timed out): the provider did not respond; \
                     retry, check provider latency, or try again"
                        .into(),
                ))
            },
        };

        if let Ok((_, Some(reported))) = &result {
            reservation.reconcile(reported.total_tokens);
        } else if reservation.was_dispatched() {
            reservation.conservative_charge();
        }

        result.map(|(output, _)| output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darius_cognitive::{Message, ToolSpec};
    use darius_tools::ToolCall;

    #[test]
    fn anthropic_messages_wire_encode_and_decode() {
        let messages = vec![
            Message::System {
                content: "You are a coding assistant.".into(),
            },
            Message::User {
                content: "Please read the file.".into(),
            },
            Message::Assistant {
                content: Some("I will read it now.".into()),
                tool_calls: vec![ToolCall {
                    id: "toolu_123".into(),
                    name: "read_file".into(),
                    arguments: json!({"path": "src/main.rs"}),
                }],
            },
            Message::Tool {
                tool_call_id: "toolu_123".into(),
                name: "read_file".into(),
                content: "fn main() {}".into(),
            },
        ];

        let tools = vec![ToolSpec {
            name: "read_file".into(),
            description: "Read a file from disk".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
        }];

        let encoded =
            encode_anthropic_request("claude-3-5-sonnet-20241022", &messages, &tools, 4096);

        assert_eq!(encoded["model"], "claude-3-5-sonnet-20241022");
        assert_eq!(encoded["max_tokens"], 4096);
        assert_eq!(encoded["system"], "You are a coding assistant.");

        let req_tools = encoded["tools"].as_array().expect("tools array");
        assert_eq!(req_tools.len(), 1);
        assert_eq!(req_tools[0]["name"], "read_file");
        assert_eq!(req_tools[0]["description"], "Read a file from disk");
        assert_eq!(req_tools[0]["input_schema"]["type"], "object");

        let req_msgs = encoded["messages"].as_array().expect("messages array");
        assert_eq!(req_msgs[0]["role"], "user");
        assert_eq!(req_msgs[1]["role"], "assistant");
        assert_eq!(req_msgs[2]["role"], "user"); // Tool result mapped to user role

        let response_payload = json!({
            "id": "msg_01X",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "Here is the plan."
                },
                {
                    "type": "tool_use",
                    "id": "toolu_999",
                    "name": "read_file",
                    "input": {"path": "Cargo.toml"}
                }
            ],
            "usage": {
                "input_tokens": 150,
                "output_tokens": 42
            }
        });

        let decoded = decode_anthropic_response(&response_payload).expect("decode output");
        assert_eq!(decoded.content.as_deref(), Some("Here is the plan."));
        assert_eq!(decoded.tool_calls.len(), 1);
        assert_eq!(decoded.tool_calls[0].id, "toolu_999");
        assert_eq!(decoded.tool_calls[0].name, "read_file");
        assert_eq!(decoded.tool_calls[0].arguments["path"], "Cargo.toml");

        let usage = parse_anthropic_usage(&response_payload).expect("usage");
        assert_eq!(usage.total_tokens, 192);
    }

    #[tokio::test]
    async fn anthropic_wire_request_headers_and_error_handling() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{header, method, path},
        };

        let server = MockServer::start().await;
        let key_env = "DARIUS_TEST_ANTHROPIC_KEY";
        unsafe { std::env::set_var(key_env, "sk-ant-test-12345") };

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-ant-test-12345"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg_test",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "Anthropic success"}],
                "usage": {"input_tokens": 10, "output_tokens": 5}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let mut model = AnthropicModel::new(
            "claude-3-5-sonnet".into(),
            format!("{}/v1", server.uri()),
            key_env.into(),
            BudgetEnforcer::new(),
            BudgetScope::Session,
        )
        .unwrap();

        let out = model
            .complete(
                &[Message::User {
                    content: "hello".into(),
                }],
                &[],
                &TurnContext::new(),
            )
            .await
            .expect("complete call");

        assert_eq!(out.content.as_deref(), Some("Anthropic success"));

        // Test 401 error mapping
        let server_401 = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server_401)
            .await;

        let mut model_401 = AnthropicModel::new(
            "claude-3-5-sonnet".into(),
            format!("{}/v1", server_401.uri()),
            key_env.into(),
            BudgetEnforcer::new(),
            BudgetScope::Session,
        )
        .unwrap();

        let err = model_401
            .complete(
                &[Message::User {
                    content: "hi".into(),
                }],
                &[],
                &TurnContext::new(),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("authentication failed"));
        unsafe { std::env::remove_var(key_env) };
    }

    #[tokio::test]
    async fn anthropic_complete_stream_emits_deltas_and_accumulates_tools() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{header, method, path},
        };

        let server = MockServer::start().await;
        let key_env = "DARIUS_TEST_ANTHROPIC_STREAM_KEY";
        unsafe { std::env::set_var(key_env, "sk-ant-test-stream") };

        let sse_body = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":12}}}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello \"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"world!\"}}\n\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_test\",\"name\":\"read_file\",\"input\":{}}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"src/lib.rs\\\"}\"}}\n\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n\
event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":28}}\n\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-ant-test-stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .expect(1)
            .mount(&server)
            .await;

        let mut model = AnthropicModel::new(
            "claude-3-5-sonnet".into(),
            format!("{}/v1", server.uri()),
            key_env.into(),
            BudgetEnforcer::new(),
            BudgetScope::Session,
        )
        .unwrap();

        struct CollectSink(std::sync::Arc<parking_lot::Mutex<Vec<String>>>);
        impl darius_cognitive::EventSink for CollectSink {
            fn emit(&self, event: darius_cognitive::UiEvent) {
                if let darius_cognitive::UiEvent::AssistantDelta { text } = event {
                    self.0.lock().push(text);
                }
            }
        }

        let deltas = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let sink = CollectSink(deltas.clone());

        let out = model
            .complete_stream(
                &[Message::User {
                    content: "hello".into(),
                }],
                &[],
                &sink,
                &TurnContext::new(),
            )
            .await
            .expect("stream response");

        let chunks = deltas.lock().clone();
        assert_eq!(chunks, vec!["Hello ", "world!"]);
        assert_eq!(out.content.as_deref(), Some("Hello world!"));
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].id, "toolu_test");
        assert_eq!(out.tool_calls[0].name, "read_file");
        assert_eq!(out.tool_calls[0].arguments["path"], "src/lib.rs");

        unsafe { std::env::remove_var(key_env) };
    }
}
