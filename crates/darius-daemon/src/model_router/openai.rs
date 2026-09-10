//! OpenAiModel owns one validated provider; exact cancellable protocol, no fallback.
use crate::model_router::{BudgetEnforcer, BudgetScope, usage, wire, wire_decode};
use darius_cognitive::{AsyncModel, CognitiveError, Message, ModelOutput, ToolSpec, TurnContext};
const MAX_OUTPUT_TOKENS: u64 = 4096;

pub struct OpenAiModel {
    pub model: String,
    pub base_url: String,
    pub key_env: String,
    pub client: reqwest::Client,
    pub budget: BudgetEnforcer,
    pub scope: BudgetScope,
}

pub type LiveModel = crate::model_router::LiveModel;

#[async_trait::async_trait]
impl AsyncModel for OpenAiModel {
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
            _ => return Err(CognitiveError::Loop("authentication failed".into())),
        };
        let input = usage::estimate_input(messages, tools).max(1);
        let mut reservation = self
            .budget
            .reserve(self.scope, input, MAX_OUTPUT_TOKENS)
            .map_err(|error| CognitiveError::Loop(error.to_string()))?;
        let body = wire::encode_request(&self.model, messages, tools, reservation.output_limit());
        let url = format!("{}/chat/completions", self.base_url);
        let fetch = async {
            reservation.mark_dispatched();
            let response = self
                .client
                .post(url)
                .bearer_auth(key)
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
            wire_decode::read_response(response).await
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
            _ => return Err(CognitiveError::Loop("authentication failed".into())),
        };
        let input = usage::estimate_input(messages, tools).max(1);
        let mut reservation = self
            .budget
            .reserve(self.scope, input, MAX_OUTPUT_TOKENS)
            .map_err(|error| CognitiveError::Loop(error.to_string()))?;
        let body =
            wire::encode_stream_request(&self.model, messages, tools, reservation.output_limit());
        let url = format!("{}/chat/completions", self.base_url);
        let fetch = async {
            reservation.mark_dispatched();
            let response = self
                .client
                .post(url)
                .bearer_auth(key)
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
            if !response.status().is_success() {
                return wire_decode::read_response(response).await;
            }

            let is_json = response
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .map(|ct| ct.contains("application/json"))
                .unwrap_or(false);

            if is_json {
                let (output, usage) = wire_decode::read_response(response).await?;
                if output.tool_calls.is_empty()
                    && let Some(text) = &output.content
                {
                    sink.emit(darius_cognitive::UiEvent::AssistantDelta { text: text.clone() });
                }
                return Ok((output, usage));
            }
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut text_acc = String::new();
            let mut partial_tools: std::collections::BTreeMap<usize, (String, String, String)> =
                std::collections::BTreeMap::new();
            let mut reported_usage: Option<usage::ProviderUsage> = None;

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
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            break;
                        }
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(u) = usage::parse(&v) {
                                reported_usage = Some(u);
                            }
                            if let Some(delta) = v.pointer("/choices/0/delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str())
                                    && !content.is_empty()
                                {
                                    text_acc.push_str(content);
                                    sink.emit(darius_cognitive::UiEvent::AssistantDelta {
                                        text: content.to_string(),
                                    });
                                }
                                if let Some(tc_arr) =
                                    delta.get("tool_calls").and_then(|t| t.as_array())
                                {
                                    for tc in tc_arr {
                                        let idx =
                                            tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                                as usize;
                                        let entry = partial_tools.entry(idx).or_insert_with(|| {
                                            (String::new(), String::new(), String::new())
                                        });
                                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                            entry.0.push_str(id);
                                        }
                                        if let Some(name) =
                                            tc.pointer("/function/name").and_then(|n| n.as_str())
                                        {
                                            entry.1.push_str(name);
                                        }
                                        if let Some(args) = tc
                                            .pointer("/function/arguments")
                                            .and_then(|a| a.as_str())
                                        {
                                            entry.2.push_str(args);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let mut tool_calls = Vec::new();
            for (_, (id, name, args_str)) in partial_tools {
                let parsed_args = serde_json::from_str::<serde_json::Value>(&args_str)
                    .unwrap_or_else(|_| serde_json::json!({}));
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
            Ok((out, reported_usage))
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
    use darius_cognitive::Message;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[tokio::test]
    async fn openai_complete_stream_emits_deltas_and_accumulates_tools() {
        let server = MockServer::start().await;
        let key_env = "DARIUS_TEST_OPENAI_STREAM_KEY";
        unsafe { std::env::set_var(key_env, "sk-test-openai-stream") };

        let sse_body = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"world!\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"Cargo.toml\\\"}\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{}}],\"usage\":{\"total_tokens\":55}}\n\n\
data: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test-openai-stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .expect(1)
            .mount(&server)
            .await;

        let mut model = OpenAiModel {
            model: "gpt-4o-mini".into(),
            base_url: format!("{}/v1", server.uri()),
            key_env: key_env.into(),
            client: reqwest::Client::new(),
            budget: BudgetEnforcer::new(),
            scope: BudgetScope::Session,
        };

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
                    content: "hi".into(),
                }],
                &[],
                &sink,
                &TurnContext::new(),
            )
            .await
            .expect("stream output");

        let chunks = deltas.lock().clone();
        assert_eq!(chunks, vec!["Hello ", "world!"]);
        assert_eq!(out.content.as_deref(), Some("Hello world!"));
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].id, "call_abc");
        assert_eq!(out.tool_calls[0].name, "read_file");
        assert_eq!(out.tool_calls[0].arguments["path"], "Cargo.toml");

        unsafe { std::env::remove_var(key_env) };
    }
}
