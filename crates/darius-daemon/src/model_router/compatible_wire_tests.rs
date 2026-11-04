use super::{LiveModel, Provider};

#[tokio::test]
async fn claude_compatible_request_uses_chat_completions_wire() {
    use darius_cognitive::{AsyncModel, Message, TurnContext};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "ok"}}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let mut provider = Provider::from_profile("openrouter").unwrap();
    provider.base_url = server.uri();
    provider.model = "claude-sonnet-4.6".into();
    provider.api_key_env = "NONE".into();
    let mut live = LiveModel::for_provider(provider).unwrap();
    let out = live
        .complete(
            &[Message::User {
                content: "hi".into(),
            }],
            &[],
            &TurnContext::new(),
        )
        .await
        .unwrap();
    assert_eq!(out.content.as_deref(), Some("ok"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].headers.contains_key("anthropic-version"));
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["model"],
        "claude-sonnet-4.6"
    );
}
