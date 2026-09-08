//! PTY-based TUI integration tests.
//!
//! These tests use portable-pty to spawn the real `darius` binary in a
//! pseudo-terminal, exercising the full TUI lifecycle (setup, input, exit).

mod support;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use support::DariusHomeSnapshot;
use support::fake_provider::{FakeProvider, ScriptedResponse};
use tempfile::TempDir;

/// Helper that spawns `darius` in a PTY with a clean, isolated environment.
struct PtyTestHarness {
    child: Box<dyn portable_pty::Child + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    termios_before: String,
    transcript: Vec<u8>,
    output: mpsc::Receiver<Result<Vec<u8>, String>>,
    reader_completed: mpsc::Receiver<()>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    writer: Option<Box<dyn Write + Send>>,
    _darius_home: TempDir,
    _workspace: TempDir,
    _snapshot: DariusHomeSnapshot,
}

impl PtyTestHarness {
    /// Spawn `darius` with a temporary DARIUS_HOME and workspace.
    /// The binary is resolved via CARGO_BIN_EXE_darius (set by cargo test).
    fn spawn(args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        Self::spawn_with_options(args, &[], |_home, _workspace| Ok(()))
    }

    fn spawn_with_options(
        args: &[&str],
        env_vars: &[(&str, &str)],
        setup: impl FnOnce(&Path, &Path) -> Result<(), Box<dyn std::error::Error>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let home = TempDir::new()?;
        let workspace = TempDir::new()?;
        setup(home.path(), workspace.path())?;
        Self::spawn_in(args, env_vars, home, workspace)
    }

    fn spawn_in(
        args: &[&str],
        env_vars: &[(&str, &str)],
        darius_home: TempDir,
        workspace: TempDir,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let bin = std::env::var_os("DARIUS_BIN_UNDER_TEST")
            .or_else(|| std::env::var_os("CARGO_BIN_EXE_darius"))
            .ok_or(
                "Neither DARIUS_BIN_UNDER_TEST nor CARGO_BIN_EXE_darius set — run via `cargo test`",
            )?;
        let bin_path = PathBuf::from(bin);
        assert!(
            bin_path.exists(),
            "darius binary not found at {:?}",
            bin_path
        );

        // Snapshot real ~/.darius before test
        let snapshot = DariusHomeSnapshot::capture();

        // Build PTY
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Prepare command with clean env
        let mut cmd = CommandBuilder::new(bin_path);
        cmd.args(args);
        cmd.cwd(workspace.path());
        cmd.env("DARIUS_HOME", darius_home.path());
        cmd.env("DARIUS_WORKSPACE", workspace.path());
        cmd.env("HOME", darius_home.path()); // Also redirect HOME so ~/.darius = temp
        for key in [
            "DARIUS_API_KEY",
            "OPENAI_API_KEY",
            "OPENROUTER_API_KEY",
            "DARIUS_BASE_URL",
            "DARIUS_MODEL",
            "DARIUS_OFFLINE",
            "DARIUS_TEST_MISSING_KEY",
        ] {
            cmd.env_remove(key);
        }
        cmd.env_remove("DARIUS_PROFILE"); // No profile preset

        for (k, v) in env_vars {
            cmd.env(k, v);
        }

        let termios_before = format!(
            "{:?}",
            pair.master.get_termios().expect("PTY termios available")
        );
        let child = pair.slave.spawn_command(cmd)?;
        let mut reader = pair.master.try_clone_reader()?;
        let (output_tx, output) = mpsc::channel();
        let (reader_completed_tx, reader_completed) = mpsc::channel();
        let reader_thread = std::thread::spawn(move || {
            let mut chunk = [0_u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(count) => {
                        if output_tx.send(Ok(chunk[..count].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = output_tx.send(Err(error.to_string()));
                        break;
                    }
                }
            }
            let _ = reader_completed_tx.send(());
        });
        let writer = pair.master.take_writer()?;
        drop(pair.slave);

        Ok(Self {
            child,
            master: pair.master,
            termios_before,
            transcript: Vec::new(),
            output,
            reader_completed,
            reader_thread: Some(reader_thread),
            writer: Some(writer),
            _darius_home: darius_home,
            _workspace: workspace,
            _snapshot: snapshot,
        })
    }

    fn workspace_path(&self) -> &Path {
        self._workspace.path()
    }

    /// Read until `needle` appears in output, or timeout.
    /// Returns the accumulated output up to and including the needle.
    fn read_until(
        &mut self,
        needle: &str,
        timeout: Duration,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let deadline = std::time::Instant::now() + timeout;
        let mut bytes = Vec::new();
        let initial_screen = support::screen::render(&self.transcript);

        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match self
                .output
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(Ok(chunk)) => {
                    self.transcript.extend_from_slice(&chunk);
                    bytes.extend_from_slice(&chunk);
                    let output = String::from_utf8_lossy(&bytes);
                    let screen = support::screen::render(&self.transcript);
                    if Self::strip_ansi(&output).contains(needle)
                        || (screen.contains(needle) && !initial_screen.contains(needle))
                    {
                        return Ok(format!("{}\n{screen}", output));
                    }
                }
                Ok(Err(error)) => return Err(error.into()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        Err(format!(
            "timeout waiting for {:?}; got: {}",
            needle,
            support::screen::render(&self.transcript)
        )
        .into())
    }

    /// Read until any of `needles` appears in output, or timeout.
    /// Returns the accumulated output up to and including the first match.
    fn read_until_any(
        &mut self,
        needles: &[&str],
        timeout: Duration,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let deadline = std::time::Instant::now() + timeout;
        let mut bytes = Vec::new();
        let initial_screen = support::screen::render(&self.transcript);

        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match self
                .output
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(Ok(chunk)) => {
                    self.transcript.extend_from_slice(&chunk);
                    bytes.extend_from_slice(&chunk);
                    let output = String::from_utf8_lossy(&bytes);
                    let screen = support::screen::render(&self.transcript);
                    let stripped = Self::strip_ansi(&output);
                    for needle in needles {
                        if stripped.contains(needle)
                            || (screen.contains(needle) && !initial_screen.contains(needle))
                        {
                            return Ok(format!("{}\n{screen}", output));
                        }
                    }
                }
                Ok(Err(error)) => return Err(error.into()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        Err(format!(
            "timeout waiting for any of {:?}; got: {}",
            needles,
            support::screen::render(&self.transcript)
        )
        .into())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let writer = self.writer.as_mut().ok_or("PTY writer is closed")?;
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    fn wait_for_reader(&mut self, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
        drop(self.writer.take());
        self.reader_completed
            .recv_timeout(timeout)
            .map_err(|error| format!("PTY reader did not terminate: {error}"))?;
        if let Some(reader_thread) = self.reader_thread.take() {
            reader_thread
                .join()
                .map_err(|_| "PTY reader thread panicked")?;
        }
        Ok(())
    }

    /// Wait for child exit with timeout; kill on drop if still running.
    fn wait_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<i32>, Box<dyn std::error::Error>> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            match self.child.try_wait()? {
                Some(status) => return Ok(Some(status.exit_code() as i32)),
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        // Timeout: kill the child, then make one bounded attempt to reap it.
        let _ = self.child.kill();
        let reap_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while std::time::Instant::now() < reap_deadline {
            if self.child.try_wait()?.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(None)
    }

    /// Strip ANSI escape sequences for assertion purposes only.
    fn strip_ansi(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                // Consume CSI sequence: ESC [ ... final_byte
                chars.next(); // '['
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn expect(&mut self, text: &str) -> String {
        Self::strip_ansi(&self.read_until(text, Duration::from_secs(3)).unwrap())
    }

    fn command(&mut self, command: &str, expected: &str) -> String {
        self.write_bytes(format!("{command}\r").as_bytes()).unwrap();
        self.expect(expected)
    }

    fn finish_turn(&mut self, marker: &str) -> String {
        let mut output = self.expect(marker);
        if !output.contains("Done") {
            output.push_str(&self.expect("Done"));
        }
        output
    }

    fn quit_restored(&mut self) {
        self.write_bytes(b"/quit\r").unwrap();
        assert_eq!(
            self.wait_with_timeout(Duration::from_secs(3)).unwrap(),
            Some(0)
        );
        assert_eq!(
            format!(
                "{:?}",
                self.master.get_termios().expect("termios after exit")
            ),
            self.termios_before,
            "raw terminal mode not restored"
        );
        self.wait_for_reader(Duration::from_secs(1)).unwrap();
        for chunk in self.output.try_iter().flatten() {
            self.transcript.extend(chunk);
        }
        assert!(
            String::from_utf8_lossy(&self.transcript).contains("\x1b[?1049l"),
            "alternate screen not restored"
        );
        self.assert_home_unchanged("journey");
    }

    /// Verify ~/.darius was not modified.
    fn assert_home_unchanged(&self, label: &str) {
        self._snapshot.assert_unchanged(label);
    }
}

impl Drop for PtyTestHarness {
    fn drop(&mut self) {
        // Best-effort bounded cleanup: kill and reap the child if still alive.
        let _ = self.child.kill();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn clean_home_bare_launch() {
    let mut harness = PtyTestHarness::spawn(&[]).expect("spawn failed");

    let output = harness
        .read_until("Welcome back", Duration::from_secs(5))
        .expect("bare PTY invocation should render the TUI");

    let clean = PtyTestHarness::strip_ansi(&output);
    assert!(
        clean.contains("Welcome back"),
        "bare PTY invocation should render the welcome card, got: {}",
        clean
    );
    assert!(
        !clean.contains("Run `darius help`"),
        "bare PTY invocation should not print a pre-TUI hint, got: {}",
        clean
    );

    // Ctrl-C is the TUI's idle-state quit key and works in raw terminal mode.
    harness.write_bytes(&[0x03]).expect("send Ctrl-C");

    let exit_code = harness
        .wait_with_timeout(Duration::from_secs(5))
        .expect("wait failed")
        .expect("process should exit within 5s");

    assert_eq!(
        exit_code, 0,
        "bare invocation should exit 0, got {}",
        exit_code
    );
    harness
        .wait_for_reader(Duration::from_secs(1))
        .expect("PTY reader should terminate after child exit");
    harness.assert_home_unchanged("clean_home_bare_launch");
}

#[test]
fn cleanup_path_idle_ctrl_c() {
    let mut harness = PtyTestHarness::spawn(&[]).expect("spawn failed");
    harness
        .read_until("Welcome back", Duration::from_secs(5))
        .expect("bare PTY invocation should render the TUI");
    harness.write_bytes(&[0x03]).expect("send Ctrl-C");
    let exit_code = harness
        .wait_with_timeout(Duration::from_secs(5))
        .expect("wait failed")
        .expect("process should exit within 5s");
    assert_eq!(exit_code, 0, "idle Ctrl-C should exit 0");
    harness.wait_for_reader(Duration::from_secs(1)).unwrap();
    harness.assert_home_unchanged("cleanup_path_idle_ctrl_c");
}

#[test]
fn cleanup_path_quit_slash() {
    let mut harness = PtyTestHarness::spawn(&[]).expect("spawn failed");
    harness
        .read_until("Welcome back", Duration::from_secs(5))
        .expect("bare PTY invocation should render the TUI");
    harness.write_bytes(b"/quit\r").expect("send /quit");
    let exit_code = harness
        .wait_with_timeout(Duration::from_secs(5))
        .expect("wait failed")
        .expect("process should exit within 5s");
    assert_eq!(exit_code, 0, "/quit command should exit 0");
    harness.wait_for_reader(Duration::from_secs(1)).unwrap();
    harness.assert_home_unchanged("cleanup_path_quit_slash");
}

#[test]
fn cleanup_path_eof() {
    let mut harness = PtyTestHarness::spawn(&[]).expect("spawn failed");
    harness
        .read_until("Welcome back", Duration::from_secs(5))
        .expect("bare PTY invocation should render the TUI");
    drop(harness.writer.take());
    let _ = harness.wait_with_timeout(Duration::from_secs(5));
    harness.assert_home_unchanged("cleanup_path_eof");
}

#[test]
fn first_run_setup_journey() {
    let mut harness = PtyTestHarness::spawn(&[]).expect("spawn failed");
    let welcome = harness.expect("Welcome back");
    assert!(!welcome.contains("Default fake provider response"));
    let guidance = harness.command("hello", "No goal was run; no completion was claimed.");
    assert!(
        guidance.contains("Setup required") && guidance.contains("config init"),
        "{guidance}"
    );
    harness.command("/config", "Model: not configured");
    let status = harness.command("/status", "Running: false");
    assert!(status.contains("Runtime state: setup"), "{status}");
    harness.command("/help", "Keyboard shortcuts:");
    harness.quit_restored();

    let bin = std::env::var_os("DARIUS_BIN_UNDER_TEST")
        .or_else(|| std::env::var_os("CARGO_BIN_EXE_darius"))
        .unwrap();
    let init = std::process::Command::new(bin)
        .args([
            "config",
            "init",
            "--provider",
            "custom-provider",
            "--base-url",
            "https://provider.invalid/v1",
            "--model",
            "custom-model",
            "--key-env",
            "DARIUS_TEST_MISSING_KEY",
        ])
        .current_dir(harness.workspace_path())
        .env("DARIUS_HOME", harness._darius_home.path())
        .env("HOME", harness._darius_home.path())
        .env_remove("DARIUS_PROFILE")
        .env_remove("DARIUS_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env("DARIUS_TEST_MISSING_KEY", "first-run-secret-sentinel")
        .output()
        .unwrap();
    assert!(init.status.success(), "config init failed: {:?}", init);
    let config = std::fs::read_to_string(
        harness
            ._darius_home
            .path()
            .join("profiles/default/config.toml"),
    )
    .unwrap();
    assert!(config.contains("DARIUS_TEST_MISSING_KEY"));
    assert!(!config.contains("first-run-secret-sentinel"));
    let home = std::mem::replace(&mut harness._darius_home, TempDir::new().unwrap());
    let workspace = std::mem::replace(&mut harness._workspace, TempDir::new().unwrap());
    let mut restarted = PtyTestHarness::spawn_in(&[], &[], home, workspace).unwrap();
    let missing = restarted.expect("DARIUS_TEST_MISSING_KEY");
    assert!(
        missing.to_lowercase().contains("missing") || missing.contains("not set"),
        "{missing}"
    );
    assert!(!missing.contains("first-run-secret-sentinel"));
    assert_eq!(
        restarted.wait_with_timeout(Duration::from_secs(3)).unwrap(),
        Some(1),
        "missing key must fail honestly"
    );
    assert_eq!(
        format!("{:?}", restarted.master.get_termios().unwrap()),
        restarted.termios_before
    );
    restarted.wait_for_reader(Duration::from_secs(1)).unwrap();
    restarted.assert_home_unchanged("first-run restart");
}

fn live_harness(provider: &FakeProvider) -> PtyTestHarness {
    let mut harness = PtyTestHarness::spawn_with_options(
        &[],
        &[("DARIUS_TEST_JOURNEY_KEY", "secret-key-123")],
        |home, workspace| {
            provider
                .write_profile_config(&home.join("profiles/default"), "DARIUS_TEST_JOURNEY_KEY")?;
            std::fs::write(workspace.join("a.txt"), "alpha-read-sentinel")?;
            std::fs::write(workspace.join("b.txt"), "beta-read-sentinel")?;
            std::fs::write(workspace.join("greeting.txt"), "original")?;
            Ok(())
        },
    )
    .unwrap();
    harness.expect("Welcome back");
    harness
}

fn assert_result(provider: &FakeProvider, id: &str, expected: &str) {
    let requests = provider.recorded_requests();
    let result = requests
        .iter()
        .rev()
        .flat_map(|r| r.body["messages"].as_array().unwrap())
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == id)
        .unwrap_or_else(|| panic!("missing result {id}; requests: {requests:?}"));
    assert!(
        result["content"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains(&expected.to_lowercase()),
        "{id}: {result}"
    );
}

fn wait_request(provider: &FakeProvider, before: usize) {
    let start = std::time::Instant::now();
    while provider.request_count() == before {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "provider request never started"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn interrupt_provider(h: &mut PtyTestHarness, provider: &FakeProvider, key: &[u8], marker: &str) {
    provider.push_delay(
        Duration::from_secs(5),
        ScriptedResponse::text("must-never-finish"),
    );
    let before = provider.request_count();
    h.write_bytes(b"delayed goal\r").unwrap();
    wait_request(provider, before);
    let start = std::time::Instant::now();
    h.write_bytes(key).unwrap();
    // Wait for either Done (normal completion) or Interrupted (cancellation)
    let out = h
        .read_until_any(&["Done", "Interrupted"], Duration::from_secs(2))
        .unwrap();
    let interrupted = PtyTestHarness::strip_ansi(&out).contains("Interrupted");
    assert!(start.elapsed() < Duration::from_secs(2));
    provider.push_text(marker);
    h.write_bytes(b"recover after cancellation\r").unwrap();
    h.finish_turn(marker);
    assert!(
        interrupted,
        "missing Interrupted despite Done + successful recovery"
    );
}

fn process_alive(pid: i32) -> bool {
    assert!(pid > 1, "must observe a valid shell PID");
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .unwrap()
        .status
        .success()
}

fn interrupt_shell(h: &mut PtyTestHarness, provider: &FakeProvider) {
    provider.push_tool_call(
        "shell-cancel",
        "shell",
        serde_json::json!({"command":"echo $$ > shell.pid; sleep 30 & echo $! > child.pid; wait"}),
    );
    h.write_bytes(b"long shell\r").unwrap();
    h.expect("Permission Required");
    h.write_bytes(b"\r").unwrap();
    let start = std::time::Instant::now();
    let pids: Vec<i32> = loop {
        let pids: Option<Vec<i32>> = ["shell.pid", "child.pid"]
            .iter()
            .map(|p| {
                std::fs::read_to_string(h.workspace_path().join(p))
                    .ok()?
                    .trim()
                    .parse()
                    .ok()
            })
            .collect();
        if let Some(pids) = pids {
            break pids;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "shell never wrote valid PIDs"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        pids.iter().all(|pid| process_alive(*pid)),
        "shell and child must be alive before Ctrl+C"
    );
    let start = std::time::Instant::now();
    h.write_bytes(&[3]).unwrap();
    let out = h
        .read_until_any(&["Done", "Interrupted"], Duration::from_secs(2))
        .unwrap();
    let interrupted = PtyTestHarness::strip_ansi(&out).contains("Interrupted");
    while pids.iter().any(|pid| process_alive(*pid)) {
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "shell or child not killed/reaped: {pids:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(start.elapsed() < Duration::from_secs(2));
    provider.push_text("shell-recovered");
    h.write_bytes(b"recover shell\r").unwrap();
    h.finish_turn("shell-recovered");
    assert!(
        interrupted,
        "missing Interrupted despite Done + reaped shell/child + successful recovery"
    );
}

#[test]
fn full_agent_journey() {
    let start = std::time::Instant::now();
    let provider = FakeProvider::start_strict("secret-key-123");
    let mut h = live_harness(&provider);
    let config = h.command(
        "/config",
        "API key env: DARIUS_TEST_JOURNEY_KEY (set: true)",
    );
    assert!(
        config.contains("Profile: default") && config.contains("Model name: custom-model"),
        "{config}"
    );
    assert!(
        config.contains(&h.workspace_path().display().to_string()),
        "wrong external workspace: {config}"
    );
    h.command("/model", "custom-provider/custom-model");
    provider.push_text("hello-live-sentinel");
    h.write_bytes(b"say hello\r").unwrap();
    h.finish_turn("hello-live-sentinel");
    provider.push_response(ScriptedResponse::ToolCalls(vec![
        (
            "read-a".into(),
            "read_file".into(),
            serde_json::json!({"path":"a.txt"}),
        ),
        (
            "read-b".into(),
            "read_file".into(),
            serde_json::json!({"path":"b.txt"}),
        ),
    ]));
    provider.push_text("both-reads-correlated");
    h.write_bytes(b"read both files\r").unwrap();
    h.finish_turn("both-reads-correlated");
    assert_result(&provider, "read-a", "alpha-read-sentinel");
    assert_result(&provider, "read-b", "beta-read-sentinel");

    provider.push_tool_call(
        "write-denied",
        "write_file",
        serde_json::json!({"path":"greeting.txt","content":"denied"}),
    );
    provider.push_text("denial-recovered");
    h.write_bytes(b"deny this write\r").unwrap();
    h.expect("Permission Required");
    h.write_bytes(&[27]).unwrap();
    h.finish_turn("denial-recovered");
    assert_eq!(
        std::fs::read_to_string(h.workspace_path().join("greeting.txt")).unwrap(),
        "original"
    );
    assert_result(&provider, "write-denied", "denied");
    provider.push_text("next-after-denial");
    h.write_bytes(b"next after denial\r").unwrap();
    h.finish_turn("next-after-denial");

    let mut diff_visible = false;
    for (id, path, content, approval) in [
        (
            "write-approved",
            "greeting.txt",
            "approved-diff-sentinel",
            Some(b"\x1b[A".as_slice()),
        ),
        (
            "write-reused",
            "greeting.txt",
            "reused-session-sentinel",
            None,
        ),
        (
            "write-other",
            "different.txt",
            "not-authorized",
            Some(b"\x1b".as_slice()),
        ),
    ] {
        provider.push_tool_call(
            id,
            "write_file",
            serde_json::json!({"path":path,"content":content}),
        );
        provider.push_text(format!("completed-{id}"));
        h.write_bytes(format!("request {id}\r").as_bytes()).unwrap();
        if let Some(key) = approval {
            h.expect("Permission Required");
            h.write_bytes(key).unwrap();
            if id == "write-approved" {
                h.expect("❯ Yes, and don't ask again this session");
                h.write_bytes(b"\r").unwrap();
            }
        }
        let out = h.finish_turn(&format!("completed-{id}"));
        if id == "write-approved" {
            diff_visible = (out.contains("+approved-diff-sentinel")
                || out.contains("+ approved-diff-sentinel"))
                && (out.contains("-original") || out.contains("- original"));
        }
        if id == "write-reused" {
            assert!(
                !out.contains("Permission Required"),
                "session grant not reused"
            );
        }
        if id == "write-other" {
            assert!(!h.workspace_path().join(path).exists());
            assert_result(&provider, id, "denied");
        } else {
            assert_eq!(
                std::fs::read_to_string(h.workspace_path().join(path)).unwrap(),
                content
            );
        }
    }
    h.command("/permissions", "Session permissions: 1 approved");
    assert!(
        diff_visible,
        "approved file changed and session grant reused, but no added/deleted diff lines rendered"
    );
    h.command("/mode plan", "Plan");
    provider.push_response(ScriptedResponse::ToolCalls(vec![
        (
            "plan-write".into(),
            "write_file".into(),
            serde_json::json!({"path":"greeting.txt","content":"plan must not write"}),
        ),
        (
            "plan-shell".into(),
            "shell".into(),
            serde_json::json!({"command":"touch forbidden-plan-shell"}),
        ),
    ]));
    provider.push_text("plan-denied-both");
    h.write_bytes(b"plan cannot mutate\r").unwrap();
    let plan = h.finish_turn("plan-denied-both");
    assert!(
        !plan.contains("Permission Required"),
        "Plan should deny without asking"
    );
    assert_result(&provider, "plan-write", "denied");
    assert_result(&provider, "plan-shell", "denied");
    assert_eq!(
        std::fs::read_to_string(h.workspace_path().join("greeting.txt")).unwrap(),
        "reused-session-sentinel"
    );
    assert!(!h.workspace_path().join("forbidden-plan-shell").exists());
    h.command("/mode auto", "Auto");
    interrupt_provider(&mut h, &provider, &[3], "http-recovered");
    interrupt_provider(&mut h, &provider, b"/stop\r", "stop-recovered");
    interrupt_shell(&mut h, &provider);
    h.command("/compact", "Compacted conversation:");
    let memory = h.command("/memory", "Durable memory:");
    assert!(
        !memory.contains("Durable memory: 0 records"),
        "compaction did not persist memory: {memory}"
    );
    h.command("/clear", "Welcome back");
    assert!(
        !support::screen::render(&h.transcript).contains("shell-recovered"),
        "clear left transcript visible"
    );
    h.quit_restored();
    assert!(!String::from_utf8_lossy(&h.transcript).contains("secret-key-123"));
    provider.assert_clean();
    assert!(
        start.elapsed() < Duration::from_secs(45),
        "journey exceeded 45 seconds"
    );
}

#[test]
fn full_agent_journey_mode_command() {
    let provider = FakeProvider::start_strict("secret-key-123");
    let mut h = live_harness(&provider);
    h.command("/mode plan", "Plan");
    h.command("/mode auto", "Auto");
    h.quit_restored();
    provider.assert_clean();
}

#[test]
fn full_agent_journey_stop_command() {
    let provider = FakeProvider::start_strict("secret-key-123");
    let mut h = live_harness(&provider);
    interrupt_provider(&mut h, &provider, b"/stop\r", "stop-recovered");
    h.quit_restored();
    provider.assert_clean();
}

#[test]
fn full_agent_journey_visible_commands() {
    let provider = FakeProvider::start_strict("secret-key-123");
    let mut h = live_harness(&provider);
    h.command("/status", "Running: false");
    h.command("/help", "Keyboard shortcuts:");
    h.command("/permissions", "Session permissions: 0 approved");
    h.command("/tasks", "Task board is empty");
    h.command("/memory", "Durable memory: 0 records");
    h.command(
        "/memory nonexistent-journey-query",
        "No memory records matching",
    );
    h.command("/pack", "MemoryPack: 0 chars across 0 records");

    h.quit_restored();
    provider.assert_clean();
}

#[test]
fn full_agent_journey_provider_cancellation() {
    let provider = FakeProvider::start_strict("secret-key-123");
    let mut h = live_harness(&provider);
    interrupt_provider(&mut h, &provider, &[3], "http-recovered");
    h.quit_restored();
    provider.assert_clean();
}

#[test]
fn full_agent_journey_shell_cancellation() {
    let provider = FakeProvider::start_strict("secret-key-123");
    let mut h = live_harness(&provider);
    interrupt_shell(&mut h, &provider);
    h.quit_restored();
    provider.assert_clean();
}
