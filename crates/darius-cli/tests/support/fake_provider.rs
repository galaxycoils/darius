#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum ScriptedResponse {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolCalls(Vec<(String, String, serde_json::Value)>),
    Delay(Duration, Box<ScriptedResponse>),
    Error {
        status: u16,
        message: String,
    },
}

impl ScriptedResponse {
    pub fn text(t: impl Into<String>) -> Self {
        Self::Text(t.into())
    }

    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: args,
        }
    }

    pub fn delay(duration: Duration, next: ScriptedResponse) -> Self {
        Self::Delay(duration, Box::new(next))
    }

    pub fn error(status: u16, message: impl Into<String>) -> Self {
        Self::Error {
            status,
            message: message.into(),
        }
    }

    fn to_http_parts(&self) -> (u16, &'static str, String) {
        match self {
            ScriptedResponse::Text(content) => {
                let body = serde_json::json!({
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": content
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 8,
                        "completion_tokens": 8,
                        "total_tokens": 16
                    }
                })
                .to_string();
                (200, "OK", body)
            }
            ScriptedResponse::ToolCall {
                id,
                name,
                arguments,
            } => {
                let args_str = match arguments {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let body = serde_json::json!({
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "tool_calls": [{
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": args_str
                                }
                            }]
                        },
                        "finish_reason": "tool_calls"
                    }],
                    "usage": {
                        "prompt_tokens": 12,
                        "completion_tokens": 12,
                        "total_tokens": 24
                    }
                })
                .to_string();
                (200, "OK", body)
            }
            ScriptedResponse::ToolCalls(calls) => {
                let tool_calls: Vec<serde_json::Value> = calls
                    .iter()
                    .map(|(id, name, arguments)| {
                        let args_str = match arguments {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": args_str
                            }
                        })
                    })
                    .collect();
                let body = serde_json::json!({
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "tool_calls": tool_calls
                        },
                        "finish_reason": "tool_calls"
                    }],
                    "usage": {
                        "prompt_tokens": 16,
                        "completion_tokens": 16,
                        "total_tokens": 32
                    }
                })
                .to_string();
                (200, "OK", body)
            }
            ScriptedResponse::Error { status, message } => {
                let status_text = match *status {
                    401 => "Unauthorized",
                    403 => "Forbidden",
                    429 => "Too Many Requests",
                    500 => "Internal Server Error",
                    _ => "Error",
                };
                let body = serde_json::json!({
                    "error": {
                        "message": message,
                        "type": "server_error"
                    }
                })
                .to_string();
                (*status, status_text, body)
            }
            ScriptedResponse::Delay(_, inner) => inner.to_http_parts(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
    pub raw_body: String,
}

pub struct FakeProvider {
    url: String,
    port: u16,
    responses: Arc<Mutex<VecDeque<ScriptedResponse>>>,
    default_response: Arc<Mutex<Option<ScriptedResponse>>>,
    recorded_requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
    expected_secret: Option<String>,
    violations: Arc<Mutex<Vec<String>>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn validate_strict_request(
    secret: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &serde_json::Value,
    raw_body: &str,
) -> Option<String> {
    let auth = headers
        .get("authorization")
        .map(String::as_str)
        .unwrap_or("");
    let expected_auth = format!("Bearer {secret}");
    if auth.is_empty() {
        return Some("authorization: missing".into());
    }
    if auth != expected_auth {
        return Some("authorization: invalid".into());
    }
    if path != "/v1/chat/completions" {
        return Some("path: invalid".into());
    }
    let content_type = headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or("");
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Some("content-type: invalid".into());
    }
    if body.get("model").and_then(|v| v.as_str()) != Some("custom-model") {
        return Some("model: invalid".into());
    }
    let Some(messages) = body.get("messages").and_then(|v| v.as_array()) else {
        return Some("messages: missing".into());
    };
    if messages.is_empty() {
        return Some("messages: empty".into());
    }
    if path.contains(secret)
        || headers
            .iter()
            .any(|(k, v)| k != "authorization" && (k.contains(secret) || v.contains(secret)))
    {
        return Some("secret: leaked outside Authorization".into());
    }
    if messages.first().and_then(|m| m["role"].as_str()) != Some("system")
        || messages.get(1).and_then(|m| m["role"].as_str()) != Some("user")
    {
        return Some("messages: expected system then user".into());
    }
    if raw_body.contains(secret) {
        return Some("secret: leaked into body".into());
    }
    validate_tool_causality(messages).or(None)
}

fn validate_tool_causality(messages: &[serde_json::Value]) -> Option<String> {
    let mut expected: Vec<(String, String, Option<serde_json::Value>)> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if !expected.is_empty() && role != "tool" {
            return Some("tool: interleaved message before results".into());
        }
        match role {
            "assistant" => {
                if let Some(calls) = msg.get("tool_calls").and_then(|c| c.as_array()) {
                    for call in calls {
                        let id = call
                            .get("id")
                            .and_then(|i| i.as_str())
                            .map(String::from)
                            .unwrap_or_default();
                        let name = call
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args = call
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .cloned();
                        if id.is_empty() {
                            return Some("tool_call: missing id".into());
                        }
                        expected.push((id, name, args));
                    }
                }
            }
            "tool" => {
                if expected.is_empty() {
                    return Some("tool: orphan result (no pending calls)".into());
                }
                let tc_id = msg
                    .get("tool_call_id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("");
                if seen_ids.contains(tc_id) {
                    return Some("tool: duplicate result".into());
                }
                let (exp_id, _exp_name, _exp_args) = expected.remove(0);
                if tc_id != exp_id {
                    return Some("tool: wrong tool_call_id order or mismatch".into());
                }
                seen_ids.insert(tc_id.to_string());
            }
            "user" | "system" if !expected.is_empty() => {
                return Some("tool: interleaved message before all results received".into());
            }
            _ => {}
        }
    }
    if !expected.is_empty() {
        return Some("tool: missing result for one or more calls".into());
    }
    None
}

fn validate_emitted_calls(
    body: &serde_json::Value,
    emitted: &HashMap<String, serde_json::Value>,
) -> Option<String> {
    for message in body["messages"].as_array()? {
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                let id = call["id"].as_str().unwrap_or("");
                let Some(original) = emitted.get(id) else {
                    return Some("tool: call never emitted".into());
                };
                if original["name"] != call["function"]["name"] {
                    return Some("tool: wrong-name history".into());
                }
                let parse = |v: &serde_json::Value| {
                    v.as_str()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                };
                if parse(&original["arguments"]) != parse(&call["function"]["arguments"]) {
                    return Some("tool: wrong-arguments history".into());
                }
            }
        }
    }
    None
}

impl FakeProvider {
    pub fn start() -> Self {
        Self::start_with_secret(None)
    }

    pub fn start_strict(secret: &str) -> Self {
        Self::start_with_secret(Some(secret.to_string()))
    }

    fn start_with_secret(expected_secret: Option<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake provider listener");
        let port = listener.local_addr().expect("local addr").port();
        let url = format!("http://127.0.0.1:{port}");

        let responses = Arc::new(Mutex::new(VecDeque::new()));
        let default_response = Arc::new(Mutex::new(None));
        let recorded_requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let emitted = Arc::new(Mutex::new(HashMap::<String, serde_json::Value>::new()));
        let violations = Arc::new(Mutex::new(Vec::new()));
        let t_violations = Arc::clone(&violations);

        let t_responses = Arc::clone(&responses);
        let t_default = Arc::clone(&default_response);
        let t_recorded = Arc::clone(&recorded_requests);
        let t_stop = Arc::clone(&stop);
        let t_secret = expected_secret.clone();

        let handle = std::thread::spawn(move || {
            while !t_stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(conn) => conn,
                    Err(_) => break,
                };
                if t_stop.load(Ordering::Relaxed) {
                    break;
                }

                let t_responses = Arc::clone(&t_responses);
                let t_default = Arc::clone(&t_default);
                let t_recorded = Arc::clone(&t_recorded);
                let t_secret = t_secret.clone();
                let emitted = Arc::clone(&emitted);
                let violations = Arc::clone(&t_violations);

                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

                    // Read HTTP request header
                    let mut raw = Vec::new();
                    let mut buf = [0u8; 4096];
                    let mut header_end = None;

                    while header_end.is_none() {
                        match stream.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                raw.extend_from_slice(&buf[..n]);
                                if let Some(pos) = find_subsequence(&raw, b"\r\n\r\n") {
                                    header_end = Some(pos);
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    let header_idx = match header_end {
                        Some(idx) => idx,
                        None => return,
                    };

                    let header_str = String::from_utf8_lossy(&raw[..header_idx]).into_owned();
                    let mut header_lines = header_str.lines();
                    let request_line = header_lines.next().unwrap_or("");
                    let mut parts = request_line.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();

                    let mut headers = HashMap::new();
                    for line in header_lines {
                        if let Some((k, v)) = line.split_once(':') {
                            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                        }
                    }

                    let content_len: usize = headers
                        .get("content-length")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);

                    let body_start = header_idx + 4;
                    while raw.len() < body_start + content_len {
                        match stream.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => raw.extend_from_slice(&buf[..n]),
                            Err(_) => break,
                        }
                    }

                    let raw_body = if raw.len() >= body_start {
                        let end = std::cmp::min(raw.len(), body_start + content_len);
                        String::from_utf8_lossy(&raw[body_start..end]).into_owned()
                    } else {
                        String::new()
                    };

                    let json_body: serde_json::Value =
                        serde_json::from_str(&raw_body).unwrap_or(serde_json::Value::Null);

                    let validation_diagnostic = t_secret.as_ref().and_then(|secret| {
                        if method != "POST" {
                            return Some("method: expected POST".into());
                        }
                        validate_strict_request(secret, &path, &headers, &json_body, &raw_body)
                            .or_else(|| {
                                validate_emitted_calls(&json_body, &emitted.lock().unwrap())
                            })
                    });

                    t_recorded.lock().unwrap().push(RecordedRequest {
                        method,
                        path,
                        headers,
                        body: json_body,
                        raw_body,
                    });
                    if let Some(diagnostic) = validation_diagnostic {
                        violations.lock().unwrap().push(diagnostic.clone());
                        let error_body =
                            serde_json::json!({"error":{"message":diagnostic}}).to_string();
                        let reply = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            error_body.len(),
                            error_body
                        );
                        let _ = stream.write_all(reply.as_bytes());
                        let _ = stream.flush();
                    } else {
                        let next_resp = {
                            let mut q = t_responses.lock().unwrap();
                            q.pop_front()
                        };
                        let resp = next_resp
                            .or_else(|| t_default.lock().unwrap().clone())
                            .unwrap_or_else(|| {
                                if t_secret.is_some() {
                                    violations.lock().unwrap().push("script exhausted".into());
                                    ScriptedResponse::error(500, "script exhausted")
                                } else {
                                    ScriptedResponse::Text("Default fake provider response".into())
                                }
                            });

                        if let ScriptedResponse::Delay(duration, _) = &resp {
                            std::thread::sleep(*duration);
                        }

                        let (status, status_text, body) = resp.to_http_parts();
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body)
                            && let Some(calls) =
                                value["choices"][0]["message"]["tool_calls"].as_array()
                        {
                            let mut history = emitted.lock().unwrap();
                            for call in calls {
                                history.insert(
                                    call["id"].as_str().unwrap().into(),
                                    call["function"].clone(),
                                );
                            }
                        }
                        let reply = format!(
                            "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(reply.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(std::net::Shutdown::Write);
                        let mut discard = [0u8; 256];
                        while let Ok(n) = stream.read(&mut discard) {
                            if n == 0 {
                                break;
                            }
                        }
                    }
                });
            }
        });

        Self {
            url,
            port,
            responses,
            default_response,
            recorded_requests,
            stop,
            expected_secret,
            violations,
            handle: Some(handle),
        }
    }

    pub fn assert_clean(&self) {
        let violations = self.violations.lock().unwrap();
        assert!(violations.is_empty(), "provider violations: {violations:?}");
        assert!(
            self.responses.lock().unwrap().is_empty(),
            "unconsumed script responses"
        );
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn push_response(&self, resp: ScriptedResponse) {
        self.responses.lock().unwrap().push_back(resp);
    }

    pub fn push_text(&self, text: impl Into<String>) {
        self.push_response(ScriptedResponse::text(text));
    }

    pub fn push_tool_call(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
        args: serde_json::Value,
    ) {
        self.push_response(ScriptedResponse::tool_call(id, name, args));
    }

    pub fn push_delay(&self, duration: Duration, next: ScriptedResponse) {
        self.push_response(ScriptedResponse::delay(duration, next));
    }

    pub fn push_error(&self, status: u16, message: impl Into<String>) {
        self.push_response(ScriptedResponse::error(status, message));
    }

    pub fn set_default_response(&self, resp: ScriptedResponse) {
        *self.default_response.lock().unwrap() = Some(resp);
    }

    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.recorded_requests.lock().unwrap().clone()
    }

    pub fn request_count(&self) -> usize {
        self.recorded_requests.lock().unwrap().len()
    }

    pub fn violations(&self) -> Vec<String> {
        self.violations.lock().unwrap().clone()
    }

    pub fn write_profile_config(
        &self,
        profile_dir: &Path,
        api_key_env: &str,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(profile_dir)?;
        let content = format!(
            "[model]\nprovider = \"custom-provider\"\nbase_url = \"{}/v1\"\nmodel = \"custom-model\"\napi_key_env = \"{api_key_env}\"\n",
            self.url
        );
        std::fs::write(profile_dir.join("config.toml"), content)
    }
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(format!("127.0.0.1:{}", self.port));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use darius_cognitive::{AsyncModel, Message, TurnContext};
    use darius_daemon::{LiveModel, Provider};

    fn request(
        provider: &FakeProvider,
        path: &str,
        auth: &str,
        kind: &str,
        body: &serde_json::Value,
    ) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", provider.port())).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let body = body.to_string();
        write!(stream, "POST {path} HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
    }

    fn envelope(messages: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"model":"custom-model", "messages": messages})
    }

    #[test]
    fn fake_provider_protocol_rejects_invalid_transport_and_envelopes() {
        let provider = FakeProvider::start_strict("fixture-secret");
        let good = envelope(serde_json::json!([
            {"role":"system","content":"system"}, {"role":"user","content":"hello"}
        ]));
        for (path, auth, kind, body, reason) in [
            (
                "/v1/chat/completions",
                "",
                "application/json",
                good.clone(),
                "authorization",
            ),
            (
                "/v1/chat/completions",
                "Authorization: Bearer wrong\r\n",
                "application/json",
                good.clone(),
                "authorization",
            ),
            (
                "/wrong",
                "Authorization: Bearer fixture-secret\r\n",
                "application/json",
                good.clone(),
                "path",
            ),
            (
                "/v1/chat/completions",
                "Authorization: Bearer fixture-secret\r\n",
                "text/plain",
                good.clone(),
                "content-type",
            ),
            (
                "/v1/chat/completions",
                "Authorization: Bearer fixture-secret\r\n",
                "application/json",
                serde_json::json!({"model":"wrong","messages":good["messages"]}),
                "model",
            ),
            (
                "/v1/chat/completions",
                "Authorization: Bearer fixture-secret\r\n",
                "application/json",
                envelope(serde_json::json!([])),
                "messages",
            ),
            (
                "/v1/chat/completions",
                "Authorization: Bearer fixture-secret\r\n",
                "application/json",
                envelope(
                    serde_json::json!([{"role":"system","content":"fixture-secret"},{"role":"user","content":"hello"}]),
                ),
                "secret",
            ),
        ] {
            let reply = request(&provider, path, auth, kind, &body);
            assert!(
                reply.starts_with("HTTP/1.1 4"),
                "accepted invalid {reason}: {reply}"
            );
            assert!(
                reply.contains(reason),
                "missing {reason} diagnostic: {reply}"
            );
        }
        provider.push_text("valid request accepted");
        assert!(
            request(
                &provider,
                "/v1/chat/completions",
                "Authorization: Bearer fixture-secret\r\n",
                "application/json",
                &good
            )
            .contains("valid request accepted")
        );
    }

    #[test]
    fn fake_provider_protocol_rejects_noncausal_tool_results() {
        for flaw in [
            "valid",
            "orphan",
            "wrong-id",
            "wrong-name",
            "wrong-arguments",
            "duplicate",
            "missing",
            "interleaved",
            "assistant-interleaved",
        ] {
            let provider = FakeProvider::start_strict("fixture-secret");
            provider.push_response(ScriptedResponse::ToolCalls(vec![
                (
                    "a".into(),
                    "read_file".into(),
                    serde_json::json!({"path":"a.txt"}),
                ),
                (
                    "b".into(),
                    "read_file".into(),
                    serde_json::json!({"path":"b.txt"}),
                ),
            ]));
            let mut messages = serde_json::json!([
                {"role":"system","content":"system"}, {"role":"user","content":"read both"}
            ])
            .as_array()
            .unwrap()
            .clone();
            let first = request(
                &provider,
                "/v1/chat/completions",
                "Authorization: Bearer fixture-secret\r\n",
                "application/json",
                &envelope(serde_json::json!(messages)),
            );
            let reply: serde_json::Value =
                serde_json::from_str(first.split_once("\r\n\r\n").unwrap().1).unwrap();
            messages.push(reply["choices"][0]["message"].clone());
            messages.push(serde_json::json!({"role":"tool","tool_call_id":"a","content":"alpha"}));
            messages.push(serde_json::json!({"role":"tool","tool_call_id":"b","content":"beta"}));
            provider.push_text("valid causal continuation");
            match flaw {
                "valid" => {}
                "orphan" => {
                    messages.remove(2);
                }
                "wrong-name" => {
                    messages[2]["tool_calls"][0]["function"]["name"] = "write_file".into()
                }
                "wrong-arguments" => {
                    messages[2]["tool_calls"][0]["function"]["arguments"] =
                        r#"{"path":"other.txt"}"#.into()
                }
                "assistant-interleaved" => messages.insert(
                    4,
                    serde_json::json!({"role":"assistant","content":"skip result"}),
                ),
                "wrong-id" => messages[3]["tool_call_id"] = "unknown".into(),
                "duplicate" => messages[4]["tool_call_id"] = "a".into(),
                "missing" => {
                    messages.pop();
                }
                "interleaved" => messages.insert(
                    4,
                    serde_json::json!({"role":"user","content":"skip result"}),
                ),
                _ => unreachable!(),
            }
            let bad = request(
                &provider,
                "/v1/chat/completions",
                "Authorization: Bearer fixture-secret\r\n",
                "application/json",
                &envelope(serde_json::json!(messages)),
            );
            if flaw == "valid" {
                assert!(
                    bad.starts_with("HTTP/1.1 200") && bad.contains("valid causal continuation"),
                    "rejected valid history: {bad}"
                );
                provider.assert_clean();
                continue;
            }
            assert!(bad.starts_with("HTTP/1.1 400"), "accepted {flaw}: {bad}");
            assert!(
                bad.contains("tool"),
                "missing tool diagnostic for {flaw}: {bad}"
            );
        }
    }

    #[tokio::test]
    async fn test_fake_provider_completes_tool_call() {
        let provider = FakeProvider::start();
        provider.push_tool_call(
            "tc-1",
            "write_file",
            serde_json::json!({"path": "foo.txt", "content": "hello"}),
        );

        unsafe {
            std::env::set_var("TEST_FAKE_KEY", "dummy");
        }
        let mut model = LiveModel::for_provider(Provider {
            name: "test".into(),
            model: "custom-model".into(),
            base_url: format!("{}/v1", provider.url()),
            enabled: true,
            api_key_env: "TEST_FAKE_KEY".into(),
        })
        .unwrap();

        let msgs = vec![Message::User {
            content: "hi".into(),
        }];
        let ctx = TurnContext::new();
        let res = model.complete(&msgs, &[], &ctx).await;
        assert!(res.is_ok(), "complete failed: {res:?}");
    }
}
