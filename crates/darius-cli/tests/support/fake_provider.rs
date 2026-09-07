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
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FakeProvider {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake provider listener");
        let port = listener.local_addr().expect("local addr").port();
        let url = format!("http://127.0.0.1:{port}");

        let responses = Arc::new(Mutex::new(VecDeque::new()));
        let default_response = Arc::new(Mutex::new(None));
        let recorded_requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let t_responses = Arc::clone(&responses);
        let t_default = Arc::clone(&default_response);
        let t_recorded = Arc::clone(&recorded_requests);
        let t_stop = Arc::clone(&stop);

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

                    t_recorded.lock().unwrap().push(RecordedRequest {
                        method,
                        path,
                        headers,
                        body: json_body,
                        raw_body,
                    });

                    // Determine response
                    let next_resp = {
                        let mut q = t_responses.lock().unwrap();
                        q.pop_front()
                    };
                    let resp = next_resp
                        .or_else(|| t_default.lock().unwrap().clone())
                        .unwrap_or_else(|| {
                            ScriptedResponse::Text("Default fake provider response".into())
                        });

                    if let ScriptedResponse::Delay(duration, _) = &resp {
                        std::thread::sleep(*duration);
                    }

                    let (status, status_text, body) = resp.to_http_parts();
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
            handle: Some(handle),
        }
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
