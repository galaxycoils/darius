//! Task 3.2: exact configured OpenAI-compatible adapter over wiremock.

use darius_cognitive::{AsyncModel, CognitiveError, Message, ToolSpec, TurnContext};
use darius_daemon::opencode_affinity::session_token;
use darius_daemon::{
    BudgetEnforcer, BudgetScope, LiveBackend, LiveModel, Provider, ProviderRouting,
};
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

fn metered_text_response(prompt_tokens: u64, completion_tokens: u64) -> serde_json::Value {
    serde_json::json!({
        "choices": [{ "message": { "content": "done here", "tool_calls": [] } }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
        },
    })
}

/// Slow-body stall: headers arrive fast, the body stalls. The turn
/// deadline must fire during the body read, not after it.
#[tokio::test]
async fn chat_completions_slow_body_honors_deadline() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_SLOWBODY";
    set_key(KEY_ENV, "slowbody-key");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let _ = stream.read(&mut buf);
        let body = r#"{"choices":[{"message":{"content":"late","tool_calls":[]}}]}"#;
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).unwrap();
        stream.flush().unwrap();
        std::thread::sleep(Duration::from_secs(5));
        let _ = stream.write_all(body.as_bytes());
    });
    let mut live = LiveModel::for_provider(Provider {
        name: "slow".into(),
        model: MODEL.into(),
        base_url: format!("http://{addr}/"),
        enabled: true,
        api_key_env: KEY_ENV.into(),
    })
    .unwrap();
    let ctx = TurnContext::with_timeout(Duration::from_millis(300));
    let start = std::time::Instant::now();
    let err = live
        .complete(&user_msg(), &[], &ctx)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "hung past deadline"
    );
    assert!(err.contains("deadline exceeded"), "got: {err}");
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_uses_configured_provider_not_default() {
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
    assert_eq!(body["max_tokens"], 4096);
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
async fn chat_completions_rejects_missing_or_wrong_authorization() {
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
async fn chat_completions_second_request_correlates_tool_results() {
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
    assert_eq!(messages[2]["name"], "read_file");
    assert_eq!(messages[2]["content"], "out-1");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call_2");
    assert_eq!(messages[3]["name"], "read_file");
    assert_eq!(messages[3]["content"], "out-2");
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_returns_terminal_text() {
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
async fn chat_completions_rejects_arguments_string_not_json() {
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
async fn chat_completions_rejects_tool_call_missing_id() {
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
async fn chat_completions_rejects_duplicate_tool_ids() {
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
async fn chat_completions_maps_401_to_auth_error() {
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
async fn chat_completions_maps_429_to_rate_limit() {
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
async fn chat_completions_maps_5xx_to_server_error() {
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
async fn chat_completions_rejects_non_json_response() {
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
async fn chat_completions_cancel_drops_delayed_response() {
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

    let budget = BudgetEnforcer::with_limit(BudgetScope::Session, 5_000);
    let mut live = LiveModel::for_provider_with_budget(
        test_provider(&server, KEY_ENV),
        budget.clone(),
        BudgetScope::Session,
    )
    .unwrap();
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
    assert!(budget.remaining(BudgetScope::Session) < 5_000);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_short_deadline_errors() {
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

    let budget = BudgetEnforcer::with_limit(BudgetScope::Session, 5_000);
    let mut live = LiveModel::for_provider_with_budget(
        test_provider(&server, KEY_ENV),
        budget.clone(),
        BudgetScope::Session,
    )
    .unwrap();
    let ctx = TurnContext::with_timeout(Duration::from_millis(150));
    let err = live
        .complete(&user_msg(), &read_spec(), &ctx)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CognitiveError::Loop(ref m) if m.contains("deadline")),
        "got: {err}"
    );
    assert!(budget.remaining(BudgetScope::Session) < 5_000);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_errors_never_carry_secrets() {
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

#[tokio::test]
async fn chat_completions_exhausted_session_budget_blocks_before_network() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_EXHAUSTED";
    set_key(KEY_ENV, "test-secret-exhausted");
    let server = MockServer::start().await;
    let budget = BudgetEnforcer::with_limit(BudgetScope::Session, 1);
    budget.charge(BudgetScope::Session, 1).unwrap();
    let mut live = LiveModel::for_provider_with_budget(
        test_provider(&server, KEY_ENV),
        budget,
        BudgetScope::Session,
    )
    .unwrap();

    let err = live
        .complete(&user_msg(), &read_spec(), &TurnContext::new())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("budget exceeded"), "got: {err}");
    assert!(server.received_requests().await.unwrap().is_empty());
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_repeated_rounds_consume_the_shared_session_budget() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_SHARED_BUDGET";
    set_key(KEY_ENV, "test-secret-shared-budget");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metered_text_response(14, 6)))
        .mount(&server)
        .await;
    let budget = BudgetEnforcer::with_limit(BudgetScope::Session, 40);
    let mut live = LiveModel::for_provider_with_budget(
        test_provider(&server, KEY_ENV),
        budget.clone(),
        BudgetScope::Session,
    )
    .unwrap();

    for _ in 0..2 {
        live.complete(&user_msg(), &[], &TurnContext::new())
            .await
            .unwrap();
    }
    assert_eq!(budget.remaining(BudgetScope::Session), 0);
    let err = live
        .complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("budget exceeded"), "got: {err}");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_missing_usage_charges_a_bounded_estimate() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_ESTIMATED_BUDGET";
    set_key(KEY_ENV, "test-secret-estimated-budget");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;
    let budget = BudgetEnforcer::with_limit(BudgetScope::Session, 1_000);
    let mut live = LiveModel::for_provider_with_budget(
        test_provider(&server, KEY_ENV),
        budget.clone(),
        BudgetScope::Session,
    )
    .unwrap();

    live.complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap();
    assert!(budget.remaining(BudgetScope::Session) < 1_000);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_atomic_reservation_blocks_concurrent_overspend() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_ATOMIC_BUDGET";
    set_key(KEY_ENV, "test-secret-atomic-budget");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(300))
                .set_body_json(metered_text_response(60, 30)),
        )
        .mount(&server)
        .await;
    let budget = BudgetEnforcer::with_limit(BudgetScope::Session, 100);
    let mut first = LiveModel::for_provider_with_budget(
        test_provider(&server, KEY_ENV),
        budget.clone(),
        BudgetScope::Session,
    )
    .unwrap();
    let mut second = LiveModel::for_provider_with_budget(
        test_provider(&server, KEY_ENV),
        budget.clone(),
        BudgetScope::Session,
    )
    .unwrap();

    let ctx_a = TurnContext::new();
    let ctx_b = TurnContext::new();
    let messages_a = user_msg();
    let messages_b = user_msg();
    let (a, b) = tokio::join!(
        first.complete(&messages_a, &[], &ctx_a),
        second.complete(&messages_b, &[], &ctx_b)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let rejected = if a.is_err() { a } else { b };
    assert!(
        rejected
            .unwrap_err()
            .to_string()
            .contains("budget exceeded")
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(budget.remaining(BudgetScope::Session), 10);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_dispatched_http_and_decode_errors_consume_budget() {
    const KEY_ENV: &str = "DARIUS_TEST_OPENAI_KEY_ERROR_BUDGET";
    set_key(KEY_ENV, "test-secret-error-budget");
    let cases = [
        ResponseTemplate::new(401),
        ResponseTemplate::new(429),
        ResponseTemplate::new(500),
        ResponseTemplate::new(200).set_body_string("not json"),
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"choices": []})),
    ];
    for response in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(response)
            .mount(&server)
            .await;
        let budget = BudgetEnforcer::with_limit(BudgetScope::Session, 5_000);
        let mut live = LiveModel::for_provider_with_budget(
            test_provider(&server, KEY_ENV),
            budget.clone(),
            BudgetScope::Session,
        )
        .unwrap();

        live.complete(&user_msg(), &read_spec(), &TurnContext::new())
            .await
            .unwrap_err();
        assert!(budget.remaining(BudgetScope::Session) < 5_000);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
    clear_key(KEY_ENV);
}

// WU-04 (2.5): profile-driven request-time auth, both paths.

fn profile_provider(server: &MockServer, name: &str, key_env: &str) -> Provider {
    Provider {
        name: name.into(),
        model: "wu04-test-model".into(),
        base_url: format!("{}/", server.uri()),
        enabled: true,
        api_key_env: key_env.into(),
    }
}

fn authorization_of(request: &wiremock::Request) -> Option<String> {
    request
        .headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

struct NoopSink;
impl darius_cognitive::EventSink for NoopSink {
    fn emit(&self, _event: darius_cognitive::UiEvent) {}
}

#[tokio::test]
async fn chat_completions_keyless_profile_sends_empty_authorization() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "opencode-free", "NONE")).unwrap();
    let out = live
        .complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(authorization_of(&received[0]), Some(String::new()));
}

#[tokio::test]
async fn chat_completions_keyless_profile_stream_sends_empty_authorization() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "opencode-free", "NONE")).unwrap();
    let out = live
        .complete_stream(&user_msg(), &[], &NoopSink, &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(authorization_of(&received[0]), Some(String::new()));
}

#[tokio::test]
async fn chat_completions_keyed_profile_sends_bearer() {
    const KEY_ENV: &str = "DARIUS_TEST_WU04_KEYED";
    const KEY: &str = "wu04-keyed-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", format!("Bearer {KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "openrouter", KEY_ENV)).unwrap();
    let out = live
        .complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        authorization_of(&received[0]),
        Some(format!("Bearer {KEY}"))
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_keyed_profile_stream_sends_bearer() {
    const KEY_ENV: &str = "DARIUS_TEST_WU04_KEYED_STREAM";
    const KEY: &str = "wu04-keyed-stream-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", format!("Bearer {KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "openrouter", KEY_ENV)).unwrap();
    let out = live
        .complete_stream(&user_msg(), &[], &NoopSink, &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        authorization_of(&received[0]),
        Some(format!("Bearer {KEY}"))
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_nous_profile_refreshes_token_on_401() {
    const NOUS_KEY_ENV: &str = "NOUS_API_KEY";
    let prev_key = std::env::var(NOUS_KEY_ENV).ok();
    let prev_home = std::env::var("DARIUS_HOME").ok();
    let prev_base = std::env::var("NOUS_INFERENCE_BASE_URL").ok();
    unsafe {
        std::env::remove_var(NOUS_KEY_ENV);
    }

    let home = std::env::temp_dir().join(format!("darius-wu04-nous-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("auth.json"),
        serde_json::json!({
            "version": 1,
            "providers": {"nous": {
                "access_token": "wu04-stale-token",
                "refresh_token": "wu04-refresh-token",
                "expires_at": 2_000_000_000i64,
                "token_type": "Bearer",
                "quarantined": false,
            }},
        })
        .to_string(),
    )
    .unwrap();
    unsafe {
        std::env::set_var("DARIUS_HOME", &home);
    }

    let server = MockServer::start().await;
    unsafe {
        std::env::set_var("NOUS_INFERENCE_BASE_URL", server.uri());
    }

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer wu04-stale-token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer wu04-fresh-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "wu04-fresh-token",
            "refresh_token": "wu04-refresh-token-2",
            "expires_in": 3600,
            "token_type": "Bearer",
        })))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "nous", NOUS_KEY_ENV)).unwrap();
    let out = live
        .complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    let chats: Vec<_> = received
        .iter()
        .filter(|request| request.url.path() == "/chat/completions")
        .collect();
    assert_eq!(chats.len(), 2);
    assert_eq!(
        authorization_of(chats[0]),
        Some("Bearer wu04-stale-token".to_owned())
    );
    assert_eq!(
        authorization_of(chats[1]),
        Some("Bearer wu04-fresh-token".to_owned())
    );

    unsafe {
        match prev_key {
            Some(value) => std::env::set_var(NOUS_KEY_ENV, value),
            None => std::env::remove_var(NOUS_KEY_ENV),
        }
        match prev_home {
            Some(value) => std::env::set_var("DARIUS_HOME", value),
            None => std::env::remove_var("DARIUS_HOME"),
        }
        match prev_base {
            Some(value) => std::env::set_var("NOUS_INFERENCE_BASE_URL", value),
            None => std::env::remove_var("NOUS_INFERENCE_BASE_URL"),
        }
    }
    let _ = std::fs::remove_dir_all(&home);
}

// WU-05 (3.1 + 3.2): attribution + session-affinity headers, both paths.

fn named_header_of(request: &wiremock::Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn expected_user_agent() -> String {
    format!("Darius/{}", env!("CARGO_PKG_VERSION"))
}

fn assert_attribution(request: &wiremock::Request) {
    assert_eq!(
        named_header_of(request, "http-referer"),
        Some("https://github.com/galaxycoils/darius".to_owned())
    );
    assert_eq!(
        named_header_of(request, "x-title"),
        Some("Darius".to_owned())
    );
    assert_eq!(
        named_header_of(request, "user-agent"),
        Some(expected_user_agent())
    );
}

fn assert_no_attribution(request: &wiremock::Request) {
    assert_eq!(named_header_of(request, "http-referer"), None);
    assert_eq!(named_header_of(request, "x-title"), None);
    assert_eq!(named_header_of(request, "user-agent"), None);
}

#[tokio::test]
async fn chat_completions_opencode_profile_sends_attribution_and_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "opencode-free", "NONE")).unwrap();
    let ctx = TurnContext::new().with_session_id("wu05-session-a");
    let out = live.complete(&user_msg(), &[], &ctx).await.unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_attribution(&received[0]);
    assert_eq!(
        named_header_of(&received[0], "x-opencode-session"),
        Some(session_token("wu05-session-a"))
    );
}

#[tokio::test]
async fn chat_completions_opencode_profile_stream_sends_attribution_and_session() {
    const KEY_ENV: &str = "DARIUS_TEST_WU05_GO_STREAM";
    const KEY: &str = "wu05-go-stream-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "opencode-go", KEY_ENV)).unwrap();
    let ctx = TurnContext::new().with_session_id("wu05-session-b");
    let out = live
        .complete_stream(&user_msg(), &[], &NoopSink, &ctx)
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_attribution(&received[0]);
    assert_eq!(
        named_header_of(&received[0], "x-opencode-session"),
        Some(session_token("wu05-session-b"))
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_openrouter_profile_sends_attribution_without_session() {
    const KEY_ENV: &str = "DARIUS_TEST_WU05_OR";
    const KEY: &str = "wu05-openrouter-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "openrouter", KEY_ENV)).unwrap();
    let out = live
        .complete(
            &user_msg(),
            &[],
            &TurnContext::new().with_session_id("wu05-session-c"),
        )
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_attribution(&received[0]);
    assert_eq!(named_header_of(&received[0], "x-opencode-session"), None);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_legacy_profile_sends_no_attribution_or_session() {
    const KEY_ENV: &str = "DARIUS_TEST_WU05_LEGACY";
    const KEY: &str = "wu05-legacy-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let out = live
        .complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_no_attribution(&received[0]);
    assert_eq!(named_header_of(&received[0], "x-opencode-session"), None);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_legacy_profile_stream_sends_no_attribution_or_session() {
    const KEY_ENV: &str = "DARIUS_TEST_WU05_LEGACY_STREAM";
    const KEY: &str = "wu05-legacy-stream-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(test_provider(&server, KEY_ENV)).unwrap();
    let out = live
        .complete_stream(&user_msg(), &[], &NoopSink, &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_no_attribution(&received[0]);
    assert_eq!(named_header_of(&received[0], "x-opencode-session"), None);
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_session_token_stable_across_turns() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "opencode-free", "NONE")).unwrap();
    let ctx = TurnContext::new().with_session_id("wu05-stable-session");
    live.complete(&user_msg(), &[], &ctx).await.unwrap();
    live.complete(&user_msg(), &[], &ctx).await.unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
    let expected = Some(session_token("wu05-stable-session"));
    assert_eq!(
        named_header_of(&received[0], "x-opencode-session"),
        expected
    );
    assert_eq!(
        named_header_of(&received[1], "x-opencode-session"),
        expected
    );
}

fn wu07_routing() -> ProviderRouting {
    ProviderRouting {
        sort: Some("throughput".into()),
        only: vec!["lambda".into()],
        ignore: vec!["azure".into()],
        order: vec!["lambda".into(), "deepinfra".into()],
    }
}

fn with_routing(mut live: LiveModel, routing: ProviderRouting) -> LiveModel {
    if let LiveBackend::ChatCompletions(inner) = &mut live.backend {
        inner.provider_routing = Some(routing);
    } else {
        panic!("expected chat-completions backend");
    }
    live
}

fn expected_nous_tags() -> serde_json::Value {
    serde_json::json!([
        "product=darius",
        format!("client=darius-client-v{}", env!("CARGO_PKG_VERSION")),
    ])
}

#[tokio::test]
async fn chat_completions_openrouter_sends_provider_routing() {
    const KEY_ENV: &str = "DARIUS_TEST_WU07_OR";
    const KEY: &str = "wu07-openrouter-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live = with_routing(
        LiveModel::for_provider(profile_provider(&server, "openrouter", KEY_ENV)).unwrap(),
        wu07_routing(),
    );
    let out = live
        .complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = received[0].body_json().unwrap();
    assert_eq!(
        body["provider_routing"],
        serde_json::json!({
            "sort": "throughput",
            "only": ["lambda"],
            "ignore": ["azure"],
            "order": ["lambda", "deepinfra"],
        })
    );
    assert!(body.get("extra_body").is_none());
    assert!(body.get("session_id").is_none());
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_openrouter_stream_sends_provider_routing() {
    const KEY_ENV: &str = "DARIUS_TEST_WU07_OR_STREAM";
    const KEY: &str = "wu07-openrouter-stream-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live = with_routing(
        LiveModel::for_provider(profile_provider(&server, "openrouter", KEY_ENV)).unwrap(),
        wu07_routing(),
    );
    let out = live
        .complete_stream(&user_msg(), &[], &NoopSink, &TurnContext::new())
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = received[0].body_json().unwrap();
    assert_eq!(body["provider_routing"]["sort"], "throughput");
    assert_eq!(
        body["provider_routing"]["only"],
        serde_json::json!(["lambda"])
    );
    assert_eq!(
        body["provider_routing"]["order"],
        serde_json::json!(["lambda", "deepinfra"])
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_openrouter_default_sends_no_routing() {
    const KEY_ENV: &str = "DARIUS_TEST_WU07_OR_DEFAULT";
    const KEY: &str = "wu07-openrouter-default-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live =
        LiveModel::for_provider(profile_provider(&server, "openrouter", KEY_ENV)).unwrap();
    live.complete(&user_msg(), &[], &TurnContext::new())
        .await
        .unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = received[0].body_json().unwrap();
    assert!(body.get("provider_routing").is_none());
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_nous_sends_tags_and_sticky_session_id() {
    const KEY_ENV: &str = "DARIUS_TEST_WU07_NOUS";
    const KEY: &str = "wu07-nous-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    // Routing prefs are set deliberately: Nous must never forward them.
    let mut live = with_routing(
        LiveModel::for_provider(profile_provider(&server, "nous", KEY_ENV)).unwrap(),
        wu07_routing(),
    );
    let ctx = TurnContext::new().with_session_id("wu07-nous-session");
    let out = live.complete(&user_msg(), &[], &ctx).await.unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = received[0].body_json().unwrap();
    assert!(body.get("provider_routing").is_none());
    assert_eq!(body["extra_body"]["tags"], expected_nous_tags());
    assert_eq!(
        body["session_id"],
        serde_json::json!(session_token("wu07-nous-session"))
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_nous_stream_sends_tags_and_sticky_session_id() {
    const KEY_ENV: &str = "DARIUS_TEST_WU07_NOUS_STREAM";
    const KEY: &str = "wu07-nous-stream-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live = with_routing(
        LiveModel::for_provider(profile_provider(&server, "nous", KEY_ENV)).unwrap(),
        wu07_routing(),
    );
    let ctx = TurnContext::new().with_session_id("wu07-nous-stream-session");
    let out = live
        .complete_stream(&user_msg(), &[], &NoopSink, &ctx)
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("done here"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    let body: serde_json::Value = received[0].body_json().unwrap();
    assert!(body.get("provider_routing").is_none());
    assert_eq!(body["extra_body"]["tags"], expected_nous_tags());
    assert_eq!(
        body["session_id"],
        serde_json::json!(session_token("wu07-nous-stream-session"))
    );
    clear_key(KEY_ENV);
}

#[tokio::test]
async fn chat_completions_nous_session_id_stable_across_turns() {
    const KEY_ENV: &str = "DARIUS_TEST_WU07_NOUS_STABLE";
    const KEY: &str = "wu07-nous-stable-secret";
    set_key(KEY_ENV, KEY);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(text_response()))
        .mount(&server)
        .await;

    let mut live = LiveModel::for_provider(profile_provider(&server, "nous", KEY_ENV)).unwrap();
    let ctx = TurnContext::new().with_session_id("wu07-stable-session");
    live.complete(&user_msg(), &[], &ctx).await.unwrap();
    live.complete(&user_msg(), &[], &ctx).await.unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
    let expected = serde_json::json!(session_token("wu07-stable-session"));
    for request in &received {
        let body: serde_json::Value = request.body_json().unwrap();
        assert_eq!(body["session_id"], expected);
        assert_eq!(body["extra_body"]["tags"], expected_nous_tags());
    }
    clear_key(KEY_ENV);
}
