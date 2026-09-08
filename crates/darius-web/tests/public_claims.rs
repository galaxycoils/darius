mod claims_support;
use darius_web::{ServerState, agent_card, create_router};

#[tokio::test]
async fn unavailable_web_surface_never_claims_execution() {
    let (state, mut events) = ServerState::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = create_router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    for path in [
        "/",
        "/a2a/card",
        "/api/events",
        "/api/goal",
        "/a2a/tasks",
        "/a2a/tasks/id",
        "/a2a/peer",
        "/a2a/inbox/handle",
        "/health",
        "/status",
    ] {
        let response = claims_support::request(address, "GET", path).await;
        assert!(
            response.to_lowercase().contains("unavailable"),
            "{path}: {response}"
        );
        if path == "/" {
            for active in ["<button", "<input", "EventSource", "fetch("] {
                assert!(!response.contains(active), "active control: {active}");
            }
        } else if path == "/a2a/card" {
            let body = response.split("\r\n\r\n").nth(1).unwrap();
            let card: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(card["capabilities"], serde_json::json!([]));
        } else {
            assert!(response.starts_with("HTTP/1.1 503"), "{path}: {response}");
            let post = claims_support::request(address, "POST", path).await;
            assert!(post.starts_with("HTTP/1.1 503"), "{path}: {post}");
            for fake in ["delivered", "pending", "completed", "\"ok\""] {
                assert!(!post.contains(fake), "{path}: fake success {fake}");
            }
        }
    }
    assert!(agent_card().capabilities.is_empty());
    assert!(state.tasks.lock().unwrap().is_empty());
    assert!(state.peer_inbox.lock().unwrap().is_empty());
    assert!(events.try_recv().is_err());
    server.abort();
}
