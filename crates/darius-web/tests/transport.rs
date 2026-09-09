mod claims_support;
use darius_web::{CanonicalEventSink, CanonicalUiEvent, GoalExecutor, ServerState, create_router};

fn test_executor(output: &str) -> GoalExecutor {
    let output = output.to_owned();
    std::sync::Arc::new(
        move |goal: String, sink: std::sync::Arc<dyn CanonicalEventSink>| {
            sink.emit(CanonicalUiEvent::UserMessage { text: goal });
            sink.emit(CanonicalUiEvent::AssistantDelta {
                text: output.clone(),
            });
            Ok(output.clone())
        },
    )
}

async fn serve(state: ServerState) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, create_router(state)).await.unwrap() });
    address
}

fn body_of(response: &str) -> &str {
    response.split("\r\n\r\n").nth(1).unwrap_or("")
}

async fn poll_task(address: std::net::SocketAddr, id: &str) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let raw = claims_support::request(address, "GET", &format!("/a2a/tasks/{id}")).await;
        let task: serde_json::Value = serde_json::from_str(body_of(&raw).trim()).unwrap();
        if task["state"] == "Completed" || task["state"] == "Failed" {
            return task;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "task {id} never finished: {task}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn goal_post_streams_correlated_sse_until_done() {
    let address = serve(ServerState::with_executor(test_executor(
        "transport-output-7",
    )))
    .await;
    let raw =
        claims_support::request_with_body(address, "POST", "/api/goal", r#"{"goal":"sse goal"}"#)
            .await;
    assert!(raw.starts_with("HTTP/1.1 202"), "{raw}");
    let submitted: serde_json::Value = serde_json::from_str(body_of(&raw).trim()).unwrap();
    let id = submitted["id"].as_str().unwrap().to_owned();

    let sse = claims_support::request(address, "GET", &format!("/api/events?task_id={id}")).await;
    assert!(sse.starts_with("HTTP/1.1 200"), "{sse}");
    assert!(sse.contains("transport-output-7"), "{sse}");
    assert!(sse.contains("\"done\""), "{sse}");
    for line in sse.lines().filter(|line| line.starts_with("data:")) {
        let payload = line.trim_start_matches("data:");
        let event: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(event["task_id"], id, "{line}");
    }

    let task = poll_task(address, &id).await;
    assert_eq!(task["state"], "Completed");
    assert_eq!(task["output"], "transport-output-7");
}

#[tokio::test]
async fn failed_execution_reports_failed_task_and_error_sse() {
    let executor: GoalExecutor = std::sync::Arc::new(|_goal, _sink| Err("executor-boom-3".into()));
    let address = serve(ServerState::with_executor(executor)).await;
    let raw = claims_support::request_with_body(
        address,
        "POST",
        "/a2a/tasks",
        r#"{"goal":"failing goal"}"#,
    )
    .await;
    assert!(raw.starts_with("HTTP/1.1 202"), "{raw}");
    let submitted: serde_json::Value = serde_json::from_str(body_of(&raw).trim()).unwrap();
    let id = submitted["id"].as_str().unwrap().to_owned();

    let task = poll_task(address, &id).await;
    assert_eq!(task["state"], "Failed");
    assert!(task["output"].as_str().unwrap().contains("executor-boom-3"));

    let sse = claims_support::request(address, "GET", &format!("/api/events?task_id={id}")).await;
    assert!(sse.contains("executor-boom-3"), "{sse}");
    assert!(sse.contains("\"error\""), "{sse}");
}
