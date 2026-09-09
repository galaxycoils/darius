mod claims_support;
use darius_web::{ServerState, agent_card, create_router};
#[tokio::test]
async fn missing_executor_never_claims_execution() {
    let (state, mut events) = ServerState::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, create_router(state)).await.unwrap() });
    for path in ["/api/goal", "/a2a/tasks"] {
        let response = claims_support::request(address, "POST", path).await;
        assert!(response.starts_with("HTTP/1.1 503"), "{response}");
        assert!(response.contains("unavailable"), "{response}");
    }
    let card = claims_support::request(address, "GET", "/a2a/card").await;
    assert!(card.contains("\"capabilities\":[]"), "{card}");
    assert!(agent_card().capabilities.is_empty());
    assert!(events.try_recv().is_err());
    server.abort();
}
