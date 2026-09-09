mod support;

use darius_cli::{paths::DariusPaths, web_bridge};
use support::fake_provider::FakeProvider;

fn temp_paths() -> (tempfile::TempDir, tempfile::TempDir, DariusPaths) {
    let home = tempfile::TempDir::new().unwrap();
    let workspace = tempfile::TempDir::new().unwrap();
    let paths = DariusPaths {
        home: home.path().to_owned(),
        workspace: workspace.path().to_owned(),
    };
    (home, workspace, paths)
}

fn write_profile_config(home: &std::path::Path, provider_url: &str, key_env: &str) {
    let profile_dir = home.join("profiles").join("default");
    std::fs::create_dir_all(&profile_dir).unwrap();
    let config = format!(
        "[model]\nprovider = \"custom-provider\"\nbase_url = \"{provider_url}/v1\"\nmodel = \"custom-model\"\napi_key_env = \"{key_env}\"\n"
    );
    std::fs::write(profile_dir.join("config.toml"), config).unwrap();
}

#[test]
fn web_bridge_refuses_setup_and_offline_demo() {
    let (home, workspace, paths) = temp_paths();
    let _ = &workspace;
    // No config at all: setup must refuse, never fake execution.
    let error = web_bridge::server_state(paths.clone(), "default".into(), false)
        .err()
        .expect("setup profile must refuse web execution");
    assert!(error.contains("configure"), "{error}");
    // Explicit offline demo must also refuse real execution.
    let error = web_bridge::server_state(paths, "default".into(), true)
        .err()
        .expect("offline demo must refuse web execution");
    assert!(error.contains("offline"), "{error}");
    let _ = &home;
}

#[test]
fn web_executor_runs_agent_loop_with_real_tool_result() {
    let provider = FakeProvider::start();
    let key_env = "DARIUS_WEB_EXEC_KEY_A";
    let key_val = "web-exec-secret-a";
    let (home, workspace, paths) = temp_paths();
    write_profile_config(home.path(), provider.url(), key_env);
    std::fs::write(
        workspace.path().join("readme.txt"),
        "Web executor proof text\n",
    )
    .unwrap();
    // SAFETY: unique env name owned by this test process.
    unsafe { std::env::set_var(key_env, key_val) };

    provider.push_tool_call(
        "call-web-read",
        "read_file",
        serde_json::json!({"path": "readme.txt"}),
    );
    provider.push_text("finished web goal");

    let state = web_bridge::server_state(paths, "default".into(), false)
        .expect("live FakeProvider profile must build");
    let router = darius_web::create_router(state);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let client = reqwest_client();
        let submitted: serde_json::Value = client
            .post(
                format!("http://{address}/a2a/tasks"),
                r#"{"goal":"read the readme"}"#,
            )
            .await;
        let id = submitted["id"].as_str().unwrap().to_owned();
        let task = poll_task(&client, address, &id).await;
        assert_eq!(task["state"], "Completed", "{task}");
        assert!(
            task["output"]
                .as_str()
                .unwrap()
                .contains("finished web goal"),
            "{task}"
        );
    });

    let requests = provider.recorded_requests();
    assert!(
        requests.len() >= 2,
        "agent must return tool result to model"
    );
    let tool_result_seen = requests.iter().skip(1).any(|request| {
        request.body["messages"].as_array().is_some_and(|messages| {
            messages.iter().any(|message| {
                message["role"] == "tool"
                    && message["tool_call_id"] == "call-web-read"
                    && message["content"]
                        .as_str()
                        .is_some_and(|text| text.contains("Web executor proof text"))
            })
        })
    });
    assert!(
        tool_result_seen,
        "actual read result missing from follow-up"
    );
    unsafe { std::env::remove_var(key_env) };
}

#[test]
fn web_executor_denies_headless_mutation() {
    let provider = FakeProvider::start();
    let key_env = "DARIUS_WEB_EXEC_KEY_B";
    let key_val = "web-exec-secret-b";
    let (home, workspace, paths) = temp_paths();
    let _ = &workspace;
    write_profile_config(home.path(), provider.url(), key_env);
    unsafe { std::env::set_var(key_env, key_val) };

    provider.push_tool_call(
        "call-web-write",
        "write_file",
        serde_json::json!({"path": "nope.txt", "content": "x"}),
    );

    let state = web_bridge::server_state(paths, "default".into(), false)
        .expect("live FakeProvider profile must build");
    let router = darius_web::create_router(state);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest_client();
        let submitted: serde_json::Value = client
            .post(
                format!("http://{address}/a2a/tasks"),
                r#"{"goal":"write nope"}"#,
            )
            .await;
        let id = submitted["id"].as_str().unwrap().to_owned();
        let task = poll_task(&client, address, &id).await;
        assert_eq!(task["state"], "Failed", "{task}");
        assert!(
            task["output"].as_str().unwrap().contains("denied")
                || task["output"].as_str().unwrap().contains("approval"),
            "{task}"
        );
    });
    unsafe { std::env::remove_var(key_env) };
}

struct TestClient;

fn reqwest_client() -> TestClient {
    TestClient
}

fn split_url(url: &str) -> (String, String) {
    let rest = url.trim_start_matches("http://");
    let address = rest.split('/').next().unwrap_or("").to_owned();
    let path = format!("/{}", rest.split_once('/').map(|x| x.1).unwrap_or(""));
    (address, path)
}

impl TestClient {
    async fn post(&self, url: String, body: &str) -> serde_json::Value {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (address, path) = split_url(&url);
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let wire = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(wire.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let body_text = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();
        serde_json::from_str(body_text).unwrap()
    }

    async fn get(&self, url: String) -> serde_json::Value {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (address, path) = split_url(&url);
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let wire = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n!");
        socket.write_all(wire.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let body_text = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();
        serde_json::from_str(body_text).unwrap()
    }
}

async fn poll_task(
    client: &TestClient,
    address: std::net::SocketAddr,
    id: &str,
) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let task = client.get(format!("http://{address}/a2a/tasks/{id}")).await;
        if task["state"] == "Completed" || task["state"] == "Failed" {
            return task;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "task {id} never finished: {task}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
