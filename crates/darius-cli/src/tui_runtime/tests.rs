use super::*;
mod memory_roundtrip;
mod permission_keys;
mod permission_lifecycle;
mod task_roundtrip;
mod tool_evidence;
use darius_cognitive::{MockModel, ModelOutput};
use darius_tui::{Mode, PermissionChoice, RuntimeCommand};
use serde_json::json;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const BOUND: Duration = Duration::from_secs(2);

struct Server {
    url: String,
    accepted: std::sync::mpsc::Receiver<()>,
    requests: std::sync::mpsc::Receiver<serde_json::Value>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(hold_first: bool) -> Self {
        Self::scripted(
            hold_first,
            |_| json!({"role":"assistant","content":"recovered"}),
        )
    }
    fn scripted(
        hold_first: bool,
        respond: impl Fn(serde_json::Value) -> serde_json::Value + Send + 'static,
    ) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let quitting = stop.clone();
        let (accepted_tx, accepted) = std::sync::mpsc::channel();
        let (tx, requests) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            let mut held = None;
            let mut index = 0;
            while !quitting.load(Ordering::SeqCst) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                // macOS inherits O_NONBLOCK from the listener; request reads must wait.
                socket.set_nonblocking(false).unwrap();
                socket.set_read_timeout(Some(BOUND)).unwrap();
                accepted_tx.send(()).unwrap();
                let mut data = Vec::new();
                let mut byte = [0];
                while !data.ends_with(b"\r\n\r\n") {
                    if socket.read_exact(&mut byte).is_err() {
                        return;
                    }
                    data.push(byte[0]);
                }
                let header = String::from_utf8(data).unwrap();
                assert!(header.starts_with("POST /chat/completions HTTP/1.1"));
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::parse)
                    })
                    .unwrap()
                    .unwrap();
                let mut body = vec![0; length];
                socket.read_exact(&mut body).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                tx.send(request.clone()).unwrap();
                if hold_first && index == 0 {
                    held = Some(socket);
                } else {
                    let body = json!({"choices":[{"message":respond(request)}]}).to_string();
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                }
                index += 1;
            }
            drop(held);
        });
        Self {
            url,
            accepted,
            requests,
            stop,
            join: Some(join),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.join.take().unwrap().join().unwrap();
    }
}
#[test]
fn blocked_control_server_fixture_waits_for_fragments_and_holds_first_response() {
    let server = Server::new(true);
    let address = server.url.strip_prefix("http://").unwrap();
    let mut first = std::net::TcpStream::connect(address).unwrap();
    // Synchronize with accept so the empty/partial reads cannot be hidden by startup.
    server.accepted.recv_timeout(BOUND).unwrap();
    let pause = Duration::from_millis(50);
    let pending = || {
        assert_eq!(
            server.requests.recv_timeout(pause),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        );
    };
    pending();
    first
        .write_all(b"POST /chat/completions HTTP/1.1\r\n")
        .unwrap();
    pending();
    first.write_all(b"Content-Length: 2\r\n\r\n{").unwrap();
    pending();
    first.write_all(b"}").unwrap();
    assert_eq!(server.requests.recv_timeout(BOUND).unwrap(), json!({}));

    // Holding the first socket must not prevent later connections from completing.
    let mut second = std::net::TcpStream::connect(address).unwrap();
    second.set_read_timeout(Some(BOUND)).unwrap();
    second
        .write_all(b"POST /chat/completions HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}")
        .unwrap();
    assert_eq!(server.requests.recv_timeout(BOUND).unwrap(), json!({}));
    let mut response = String::new();
    second.read_to_string(&mut response).unwrap();
    let (header, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(header.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).unwrap()["choices"][0]["message"]["content"],
        "recovered"
    );

    first.set_read_timeout(Some(pause)).unwrap();
    let mut byte = [0];
    let error = first
        .read(&mut byte)
        .expect_err("first response must remain held");
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    drop(server);
    assert_eq!(
        first.read(&mut byte).unwrap(),
        0,
        "teardown closes held socket"
    );
}

struct Harness {
    temp: tempfile::TempDir,
    commands: tokio::sync::mpsc::UnboundedSender<RuntimeCommand>,
    events: tokio::sync::broadcast::Receiver<darius_core::runtime_protocol::RuntimeEvent<UiEvent>>,
    join: Option<std::thread::JoinHandle<()>>,
    fallback_cancel: tokio_util::sync::CancellationToken,
}
impl Harness {
    fn new(url: &str, outputs: Option<Vec<ModelOutput>>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = DariusPaths {
            home: temp.path().join("home"),
            workspace: temp.path().to_owned(),
        };
        let profile = paths.profile("test").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        // A local-only fixture uses an existing non-secret variable; no env mutation.
        assert!(std::env::var("HOME").is_ok());
        std::fs::write(profile.join("config.toml"), format!("[model]\nprovider = 'test'\nbase_url = '{url}'\nmodel = 'test'\napi_key_env = 'HOME'\n")).unwrap();
        let mut runtime = SessionRuntime::from_profile(&paths, "test").unwrap();
        if let Some(outputs) = outputs {
            runtime.model = Box::new(MockModel::new(outputs));
        }
        let fallback_cancel = runtime.cancellation_token();
        let (mut worker, events) = TuiWorker::new(runtime);
        let (commands, rx) = tokio::sync::mpsc::unbounded_channel();
        let join = Some(std::thread::spawn(move || worker.run_loop(rx)));
        Self {
            temp,
            commands,
            events,
            join,
            fallback_cancel,
        }
    }
    fn new_with_mcp(url: &str, outputs: Option<Vec<ModelOutput>>, mcp_servers_toml: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = DariusPaths {
            home: temp.path().join("home"),
            workspace: temp.path().to_owned(),
        };
        let profile = paths.profile("test").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        assert!(std::env::var("HOME").is_ok());
        let toml_content = format!(
            "[model]\nprovider = 'test'\nbase_url = '{url}'\nmodel = 'test'\napi_key_env = 'HOME'\n\n{mcp_servers_toml}\n"
        );
        std::fs::write(profile.join("config.toml"), toml_content).unwrap();
        let mut runtime = SessionRuntime::from_profile(&paths, "test").unwrap();
        if let Some(outputs) = outputs {
            runtime.model = Box::new(MockModel::new(outputs));
        }
        let fallback_cancel = runtime.cancellation_token();
        let (mut worker, events) = TuiWorker::new(runtime);
        let (commands, rx) = tokio::sync::mpsc::unbounded_channel();
        let join = Some(std::thread::spawn(move || worker.run_loop(rx)));
        Self {
            temp,
            commands,
            events,
            join,
            fallback_cancel,
        }
    }
    fn submit(&self, text: &str) {
        self.commands
            .send(RuntimeCommand::SubmitGoal {
                text: text.into(),
                mode: Mode::Auto,
            })
            .unwrap();
    }
    fn until(&mut self, predicate: impl Fn(&UiEvent) -> bool) -> Vec<UiEvent> {
        let deadline = Instant::now() + BOUND;
        let mut seen = vec![];
        while Instant::now() < deadline {
            if let Ok(envelope) = self.events.try_recv() {
                let event = envelope.event;
                let done = predicate(&event);
                seen.push(event);
                if done {
                    return seen;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("control did not respond under 2 s; events: {seen:?}");
    }
    fn permit(&mut self, choice: PermissionChoice) {
        let seen = self.until(|e| matches!(e, UiEvent::PermissionRequired { .. }));
        let UiEvent::PermissionRequired { id, .. } = seen.last().unwrap() else {
            unreachable!()
        };
        self.commands
            .send(RuntimeCommand::ResolvePermission {
                id: id.clone(),
                choice,
            })
            .unwrap();
    }
    fn done(&mut self, interrupted: bool) {
        let events = self.until(|e| matches!(e, UiEvent::Done));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, UiEvent::Interrupted { .. }))
                .count(),
            usize::from(interrupted)
        );
        assert!(
            !events.iter().any(|e| matches!(e, UiEvent::Error { .. })),
            "{events:?}"
        );
    }
    fn shutdown(&mut self) {
        self.commands.send(RuntimeCommand::Shutdown).unwrap();
        let deadline = Instant::now() + BOUND;
        while !self.join.as_ref().unwrap().is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            self.join.as_ref().unwrap().is_finished(),
            "shutdown did not reap turn under 2 s"
        );
        self.join.take().unwrap().join().unwrap();
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.commands.send(RuntimeCommand::Shutdown);
        self.fallback_cancel.cancel();
        if let Some(join) = self.join.take()
            && join.is_finished()
        {
            join.join().unwrap();
        }
    }
}
fn call(name: &str, arguments: serde_json::Value) -> ModelOutput {
    ModelOutput {
        content: None,
        tool_calls: vec![darius_tools::ToolCall {
            id: format!("call-{}", darius_core::runtime_protocol::TurnId::next().0),
            name: name.into(),
            arguments,
        }],
    }
}
fn text() -> ModelOutput {
    ModelOutput {
        content: Some("finished".into()),
        tool_calls: vec![],
    }
}
fn slash(name: &str) -> RuntimeCommand {
    RuntimeCommand::ExecuteSlash(darius_tui::commands::parse_invocation(name).unwrap())
}
#[test]
fn blocked_control_busy_is_typed_and_readonly_status_remains_responsive() {
    let server = Server::new(true);
    let mut h = Harness::new(&server.url, None);
    h.submit("waiting");
    server.requests.recv_timeout(BOUND).unwrap();
    h.submit("must not queue");
    let events = h.until(|e| serde_json::to_value(e).unwrap()["type"] == "busy");
    assert!(!events.iter().any(|e| matches!(e, UiEvent::Done)));
    h.commands.send(slash("/compact")).unwrap();
    h.until(|e| serde_json::to_value(e).unwrap()["type"] == "busy");
    h.commands.send(slash("/status")).unwrap();
    let events = h.until(|e| matches!(e, UiEvent::Status { line } if line == "Running: true"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::Status { line } if line.contains("test/test")))
    );
    h.commands.send(slash("/permissions")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("permission")));
    h.commands.send(slash("/stop")).unwrap();
    h.done(true);
    assert!(server.requests.try_recv().is_err(), "busy goal ran later");
    h.commands.send(slash("/quit")).unwrap();
    let deadline = Instant::now() + BOUND;
    while !h.join.as_ref().unwrap().is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.join.as_ref().unwrap().is_finished());
    h.join.take().unwrap().join().unwrap();
}

#[test]
fn blocked_control_permissions_reads_shared_session_approvals() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call(
                "write_file",
                json!({"path":"cached.txt","content":"approved"}),
            ),
            text(),
            call("shell", json!({"command":"sleep 30"})),
        ]),
    );
    h.submit("cache approval");
    h.permit(PermissionChoice::AllowSession);
    h.done(false);
    h.submit("wait for permission");
    h.until(|e| matches!(e, UiEvent::PermissionRequired { .. }));
    h.commands.send(slash("/permissions")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("write_file: ") && line.ends_with("/cached.txt")));
    h.shutdown();
    h.done(true);
}

#[test]
fn blocked_control_permission_resolves_actual_write() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call(
                "write_file",
                json!({"path":"result.txt","content":"approved"}),
            ),
            text(),
        ]),
    );
    h.submit("write");
    h.permit(PermissionChoice::AllowOnce);
    h.done(false);
    assert_eq!(
        std::fs::read_to_string(h.temp.path().join("result.txt")).unwrap(),
        "approved"
    );
    h.shutdown();
}
#[test]
fn blocked_control_denial_does_not_write_and_later_turn_succeeds() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call("write_file", json!({"path":"denied.txt","content":"no"})),
            text(),
            text(),
        ]),
    );
    h.submit("deny");
    h.permit(PermissionChoice::Deny);
    h.done(false);
    assert!(!h.temp.path().join("denied.txt").exists());
    h.submit("later");
    h.done(false);
    h.shutdown();
}
#[test]
fn blocked_control_provider_interrupt_and_fresh_later_turn() {
    let server = Server::new(true);
    let mut h = Harness::new(&server.url, None);
    h.submit("wait");
    server.requests.recv_timeout(BOUND).unwrap();
    h.commands.send(RuntimeCommand::Interrupt).unwrap();
    h.done(true);
    h.submit("later");
    h.done(false);
    server.requests.recv_timeout(BOUND).unwrap();
    h.shutdown();
}
#[test]
fn blocked_control_shutdown_reaps_active_provider() {
    let server = Server::new(true);
    let mut h = Harness::new(&server.url, None);
    h.submit("wait");
    server.requests.recv_timeout(BOUND).unwrap();
    h.shutdown();
    h.done(true);
}
#[test]
fn blocked_control_shell_interrupt_reaps_and_later_turn_succeeds() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call(
                "shell",
                json!({"command":"echo $$ > shell.pid; exec sleep 30"}),
            ),
            text(),
        ]),
    );
    h.submit("shell");
    h.permit(PermissionChoice::AllowOnce);
    let deadline = Instant::now() + BOUND;
    let pidfile = h.temp.path().join("shell.pid");
    let pid = loop {
        if let Some(pid) = std::fs::read_to_string(&pidfile)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
            .filter(|pid| *pid > 0)
        {
            break pid.to_string();
        }
        assert!(
            Instant::now() < deadline,
            "shell did not publish a valid PID"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let probe = || {
        std::process::Command::new("kill")
            .env("LC_ALL", "C")
            .args(["-0", &pid])
            .output()
            .unwrap()
    };
    assert!(
        probe().status.success(),
        "shell must exist before interruption"
    );
    h.commands.send(RuntimeCommand::Interrupt).unwrap();
    h.done(true);
    let gone = probe();
    assert_eq!(
        gone.status.code(),
        Some(1),
        "unexpected process probe status"
    );
    assert!(
        String::from_utf8_lossy(&gone.stderr).contains("No such process"),
        "expected reaped PID, got {:?}",
        gone
    );
    h.submit("later");
    h.done(false);
    h.shutdown();
}
#[test]
fn blocked_control_successful_conversation_survives_turn_handoff() {
    let server = Server::new(false);
    let mut h = Harness::new(&server.url, None);
    h.submit("remember the first goal");
    h.done(false);
    h.submit("use that context");
    h.done(false);
    server.requests.recv_timeout(BOUND).unwrap();
    let second = server.requests.recv_timeout(BOUND).unwrap();
    let roles: Vec<_> = second["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["role"].as_str())
        .collect();
    assert_eq!(roles, vec!["system", "user", "assistant", "user"]);
    assert_eq!(second["messages"][1]["content"], "remember the first goal");
    h.shutdown();
}

#[test]
fn slash_command_config_redacts_url_credentials() {
    let mut h = Harness::new(
        "http://fixture-user:fixture-password@localhost/v1?token=fixture-query#fixture-fragment",
        None,
    );
    h.commands.send(slash("/config")).unwrap();
    let events =
        h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("API key env:")));
    let displayed = format!("{events:?}");
    for secret in [
        "fixture-user",
        "fixture-password",
        "fixture-query",
        "fixture-fragment",
    ] {
        assert!(!displayed.contains(secret), "config leaked URL credential");
    }
    assert!(displayed.contains("http://localhost/v1"));
    h.shutdown();
}

#[test]
fn slash_command_execution_semantic_table_all_13_commands() {
    let mut h = Harness::new("http://127.0.0.1:1", None);

    // 1. /help
    h.commands.send(slash("/help")).unwrap();
    let events =
        h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("Ctrl+C: Interrupt")));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::Status { line } if line.contains("Available commands:")))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::Status { line } if line.contains("/help")))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::Status { line } if line.contains("/quit")))
    );

    // 2. /clear
    h.commands.send(slash("/clear")).unwrap();
    h.until(|e| matches!(e, UiEvent::ClearTranscript));

    // 3. /compact
    h.commands.send(slash("/compact")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("Compacted conversation:")));

    // 4. /model
    h.commands.send(slash("/model")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("Provider/Model:")));

    // 5. /mode [auto|plan]
    h.commands.send(slash("/mode plan")).unwrap();
    h.until(|e| matches!(e, UiEvent::ModeChanged { mode: Mode::Plan }));
    h.commands.send(slash("/mode auto")).unwrap();
    h.until(|e| matches!(e, UiEvent::ModeChanged { mode: Mode::Auto }));

    // 6. /permissions
    h.commands.send(slash("/permissions")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("Session permissions:")));

    // 7. /memory [query]
    h.commands.send(slash("/memory")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("Durable memory:")));

    // 8. /pack
    h.commands.send(slash("/pack")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("MemoryPack:")));

    // 9. /tasks
    h.commands.send(slash("/tasks")).unwrap();
    h.until(|e| matches!(e, UiEvent::TaskBoard(_)));

    // 10. /status
    h.commands.send(slash("/status")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("Running: false")));

    // 11. /config
    h.commands.send(slash("/config")).unwrap();
    h.until(|e| matches!(e, UiEvent::Status { line } if line.contains("Profile:")));

    // 12. /stop (when idle, no active turn)
    h.commands.send(slash("/stop")).unwrap();

    // 13. /quit
    h.commands.send(slash("/quit")).unwrap();
    let deadline = Instant::now() + BOUND;
    while !h.join.as_ref().unwrap().is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.join.as_ref().unwrap().is_finished());
    h.join.take().unwrap().join().unwrap();
}
