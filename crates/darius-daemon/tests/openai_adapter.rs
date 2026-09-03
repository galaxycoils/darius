//! Task 3.2 RED: exact configured OpenAI-compatible adapter over wiremock.
//! `LiveModel::for_provider` + `AsyncModel` do not exist yet — this must fail.

use darius_cognitive::{AsyncModel, CognitiveError, Message, ToolSpec, TurnContext};
use darius_daemon::{LiveModel, Provider};
use std::time::Duration;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

const MODEL: &str = "acme-model-1";

fn test_provider(server: &MockServer, key_env: &str) -> Provider {
    Provider {
        name: "acme-custom".into(),
        model: MODEL.into(),
        base_url: format!("{}/", server.uri()),
        enabled: true,
        api_key_env: key_env.into(),
    }
}

fn set_key(env: &str, val: &str) {
    unsafe {
        std::env::set_var(env, val);
    }
}

fn clear_key(env: &str) {
    unsafe {
        std::env::remove_var(env);
    }
}

fn user_msg() -> Vec<Message> {
    vec![Message::User {
        content: "hi there".into(),
    }]
}

fn read_spec() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "read_file".into(),
        description: "read a file".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
        }),
    }]
}

fn two_calls_response() -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "content": null,
                "tool_calls": [
                    {"id": "call_1", "type": "function",
                     "function": {"name": "read_file",
                                 "arguments": "{\"path\":\"a.txt\"}"}},
                    {"id": "call_2", "type": "function",
                     "function": {"name": "read_file",
                                 "arguments": "{\"path\":\"b.txt\"}"}},
                ],
            },
        }],
    })
}

fn text_response() -> serde_json::Value {
    serde_json::json!({
        "choices": [{ "message": { "content": "done here", "tool_calls": [] } }],
    })
}

#[tokio::test]
async fn openai_uses_configured_provider_not_default() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_CONFIGURED";
    const KEY: &str = "test-secret-configured";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", format!("Bearer {KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(two_calls_response()))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let out = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap();

    assert_eq!(out.tool_calls.len(), 2);
    assert_eq!(out.tool_calls[0].id, "call_1");
    assert_eq!(out.tool_calls[0].name, "read_file");
    assert_eq!(
        out.tool_calls[0].arguments,
        serde_json::json!({"path": "a.txt"})
    );
    assert_eq!(out.tool_calls[1].id, "call_2");
    assert_eq!(
        out.tool_calls[1].arguments,
        serde_json::json!({"path": "b.txt"})
    );

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].url.path(), "/chat/completions");
    assert!(received[0].url.query().is_none());
    let body: serde_json::Value = received[0].body_json().unwrap();
    assert_eq!(body["model"], MODEL);
    assert_eq!(
        body["messages"][0],
        serde_json::json!({"role": "user", "content": "hi there"})
    );
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "read_file");
    assert_eq!(body["tools"][0]["function"]["description"], "read a file");
    assert_eq!(
        body["tools"][0]["function"]["parameters"]["properties"]["path"]["type"],
        "string"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_rejects_missing_or_wrong_authorization() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_AUTH";
    const KEY: &str = "test-secret-auth";
    let server = MockServer::start().await;

    // Wrong key presented: server answers 401.
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    set_key(KEY_ENV, "wrong-key");
    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("authentication failed")),
        "got: {err}"
    );

    // Missing key: rejected client-side without a request.
    clear_key(KEY_ENV);
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("authentication failed")),
        "got: {err}"
    );

    // Correct key: two exact nested tool calls.
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", format!("Bearer {KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(two_calls_response()))
        .mount(&server)
        .await;
    set_key(KEY_ENV, KEY);
    let out = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.tool_calls.len(), 2);
    assert_eq!(out.tool_calls[0].id, "call_1");
    assert_eq!(out.tool_calls[1].id, "call_2");
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_second_request_correlates_tool_results() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_CORRELATE";
    const KEY: &str = "test-secret-correlate";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(two_calls_response()))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let first = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(first.tool_calls.len(), 2);
    let first_seen = server.received_requests().await.unwrap();
    assert_eq!(first_seen.len(), 1);

    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let history = vec![
        Message::User {
            content: "hi there".into(),
        },
        Message::Assistant {
            content: Some("working".into()),
            tool_calls: first.tool_calls.clone(),
        },
        Message::Tool {
            tool_call_id: "call_1".into(),
            name: "read_file".into(),
            content: "out-1".into(),
        },
        Message::Tool {
            tool_call_id: "call_2".into(),
            name: "read_file".into(),
            content: "out-2".into(),
        },
    ];
    let second = live
        .complete(&history, &read_spec(), &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(second.content.as_deref(), Some("done here"));
    assert!(second.tool_calls.is_empty());

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = received[0].body_json().unwrap();
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"], "working");
    assert_eq!(messages[1]["tool_calls"][0]["id"], "call_1");
    assert_eq!(
        messages[1]["tool_calls"][0]["function"]["arguments"],
        "{\"path\":\"a.txt\"}"
    );
    assert_eq!(messages[1]["tool_calls"][1]["id"], "call_2");
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["tool_call_id"], "call_1");
    assert_eq!(messages[2]["content"], "out-1");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call_2");
    assert_eq!(messages[3]["content"], "out-2");
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_returns_terminal_text() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_TEXT";
    const KEY: &str = "test-secret-text";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let out = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));
    assert!(out.tool_calls.is_empty());
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_rejects_arguments_string_not_json() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_BADARGS";
    set_key(KEY_ENV, "test-secret-badargs");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "oops",
                    "tool_calls": [
                        {"id": "call_1", "type": "function",
                         "function": {"name": "read_file",
                                     "arguments": "not-json{{{oops"}},
                        {"id": "call_2", "type": "function",
                         "function": {"name": "read_file",
                                     "arguments": "{\"path\":\"b.txt\"}"}},
                    ],
                },
            }],
        })))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::InvalidPlan(_)),
        "mixed malformed calls must be rejected, got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_rejects_tool_call_missing_id() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_NOID";
    set_key(KEY_ENV, "test-secret-noid");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [
                        {"type": "function",
                         "function": {"name": "read_file",
                                     "arguments": "{\"path\":\"a.txt\"}"}},
                    ],
                },
            }],
        })))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::InvalidPlan(_)),
        "missing tool id must be rejected, got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_rejects_duplicate_tool_ids() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_DUPID";
    set_key(KEY_ENV, "test-secret-dupid");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [
                        {"id": "call_1", "type": "function",
                         "function": {"name": "read_file",
                                     "arguments": "{\"path\":\"a.txt\"}"}},
                        {"id": "call_1", "type": "function",
                         "function": {"name": "read_file",
                                     "arguments": "{\"path\":\"b.txt\"}"}},
                    ],
                },
            }],
        })))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::InvalidPlan(_)),
        "duplicate tool ids must be rejected, got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_maps_401_to_auth_error() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_401";
    set_key(KEY_ENV, "test-secret-401");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("authentication failed")),
        "got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_maps_429_to_rate_limit() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_429";
    set_key(KEY_ENV, "test-secret-429");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("rate limited")),
        "got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_maps_5xx_to_server_error() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_500";
    set_key(KEY_ENV, "test-secret-500");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("provider unavailable")),
        "got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_rejects_non_json_response() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_NONJSON";
    set_key(KEY_ENV, "test-secret-nonjson");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("this is not json"))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("invalid response")),
        "got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_cancel_drops_delayed_response() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_CANCEL";
    set_key(KEY_ENV, "test-secret-cancel");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(30))
                .set_body_json(text_response()),
        )
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let ctx = TurnContext::new();
    let token = ctx.token();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });
    let start = std::time::Instant::now();
    let err = live
        .complete(&user_msg(), &read_spec(), &ctx)
        .await
        .unwrap_err();
    let elapsed = start.elapsed();
    assert!(
        matches!(err, CognitiveError::Cancelled),
        "cancel must win, got: {err}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "cancelled turn must drop fast, took {elapsed:?}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_short_deadline_errors() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_DEADLINE";
    set_key(KEY_ENV, "test-secret-deadline");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_json(text_response()),
        )
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let ctx = TurnContext::with_timeout(Duration::from_millis(150));
    let err = live
        .complete(&user_msg(), &read_spec(), &ctx)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("deadline")),
        "got: {err}"
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn openai_errors_never_carry_secrets() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_LEAK";
    const KEY: &str = "sk-test-ultra-secret-zzz-32";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": format!("boom {KEY} backend exploded"),
        })))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    let display = format!("{err}");
    assert!(
        !display.contains(KEY),
        "secret leaked into error: {display}"
    );
    assert!(
        !display.contains("boom"),
        "server body leaked into error: {display}"
    );
    assert!(!display.contains(KEY_ENV));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].url.path(), "/chat/completions");
    assert!(received[0].url.query().is_none());
    clear_key(KEY_ENV);
}
