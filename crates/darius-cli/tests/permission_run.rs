use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
struct Server {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(name: &str, arguments: Value) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(vec![]));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let quit = stop.clone();
        let name = name.to_owned();
        let join = Some(std::thread::spawn(move || {
            while !quit.load(Ordering::SeqCst) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut header = vec![];
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                let header = String::from_utf8(header).unwrap();
                assert!(header.starts_with("POST /chat/completions HTTP/1.1"));
                let length: usize = header
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::parse)
                    })
                    .unwrap()
                    .unwrap();
                let mut bytes = vec![0; length];
                socket.read_exact(&mut bytes).unwrap();
                let first = {
                    let mut requests = recorded.lock().unwrap();
                    requests.push(serde_json::from_slice(&bytes).unwrap());
                    requests.len() == 1
                };
                let message = if first {
                    json!({"role":"assistant","tool_calls":[{"id":"mutation-1","type":"function","function":{"name":name,"arguments":arguments.to_string()}}]})
                } else {
                    json!({"role":"assistant","content":"continued after tool result"})
                };
                let body = json!({"choices":[{"message":message}]}).to_string();
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        }));
        Self {
            url,
            requests,
            stop,
            join,
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.join.take().unwrap().join().unwrap();
    }
}
fn run(name: &str, arguments: Value, mutation: bool) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("input.txt"), "noninteractive read evidence").unwrap();
    let server = Server::new(name, arguments);
    let home = temp.path().join("home");
    let profile = home.join("profiles/default");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("config.toml"),format!("[model]\nprovider='test'\nbase_url='{}'\nmodel='test'\napi_key_env='DARIUS_FIXTURE_KEY'\n",server.url)).unwrap();
    let output = assert_cmd::Command::new(env!("CARGO_BIN_EXE_darius"))
        .env("DARIUS_HOME", home)
        .env("DARIUS_FIXTURE_KEY", "test-only-key")
        .env_remove("DARIUS_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .args(["--cwd", workspace.to_str().unwrap(), "run", "inspect"])
        .timeout(Duration::from_secs(4))
        .assert()
        .get_output()
        .clone();
    assert!(
        !workspace.join("output.txt").exists(),
        "headless mutation executed"
    );
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let results: Vec<_> = requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["tool_call_id"], "mutation-1");
    if mutation {
        assert_eq!(output.status.code(), Some(1));
        assert!(results[0]["content"].as_str().unwrap().contains("denied"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("TUI"), "{stderr}");
    } else {
        assert_eq!(output.status.code(), Some(0));
        assert!(
            results[0]["content"]
                .as_str()
                .unwrap()
                .contains("noninteractive read evidence")
        );
    }
}
#[test]
fn permission_lifecycle_non_tty_write_denied_with_guidance() {
    run(
        "write_file",
        json!({"path":"output.txt","content":"unsafe"}),
        true,
    );
}
#[test]
fn permission_lifecycle_non_tty_shell_denied_with_guidance() {
    run("shell", json!({"command":"touch output.txt"}), true);
}
#[test]
fn permission_lifecycle_non_tty_read_allowed() {
    run("read_file", json!({"path":"input.txt"}), false);
}
