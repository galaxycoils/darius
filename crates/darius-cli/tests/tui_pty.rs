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

        // Create isolated temp directories
        let darius_home = TempDir::new()?;
        let workspace = TempDir::new()?;

        setup(darius_home.path(), workspace.path())?;

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
        cmd.env_remove("DARIUS_API_KEY"); // Ensure no API key leaks in
        cmd.env_remove("DARIUS_PROFILE"); // No profile preset

        for (k, v) in env_vars {
            cmd.env(k, v);
        }

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

        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(Ok(chunk)) => {
                    bytes.extend_from_slice(&chunk);
                    let output = String::from_utf8_lossy(&bytes);
                    if output.contains(needle) {
                        return Ok(output.into_owned());
                    }
                }
                Ok(Err(error)) => return Err(error.into()),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        Err(format!(
            "timeout waiting for {:?}; got: {}",
            needle,
            String::from_utf8_lossy(&bytes)
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
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
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
    harness
        .read_until("Welcome back", Duration::from_secs(5))
        .expect("bare PTY invocation should render the TUI");

    // Submit a goal on first-run without configured API key
    harness.write_bytes(b"hello\r").expect("submit goal");
    harness
        .read_until("Setup required", Duration::from_secs(5))
        .expect("should display setup guidance");

    // Check /config command
    harness.write_bytes(b"/config\r").expect("send /config");
    harness
        .read_until("Profile: default", Duration::from_secs(5))
        .expect("should display config status");

    // Exit cleanly with /quit
    harness.write_bytes(b"/quit\r").expect("send /quit");
    let exit_code = harness
        .wait_with_timeout(Duration::from_secs(5))
        .expect("wait failed")
        .expect("process should exit within 5s");
    assert_eq!(exit_code, 0, "first_run_setup_journey should exit 0");
    harness.wait_for_reader(Duration::from_secs(1)).unwrap();
    harness.assert_home_unchanged("first_run_setup_journey");
}

#[test]
fn full_agent_journey() {
    let provider = FakeProvider::start();
    let provider_url = provider.url().to_string();

    // Turn 1: model executes write_file tool call requiring permission
    provider.push_tool_call(
        "call-1",
        "write_file",
        serde_json::json!({
            "path": "greeting.txt",
            "content": "hello from darius agent journey\n"
        }),
    );
    // After tool completes, model provides final text
    provider.push_text("Successfully wrote greeting.txt!");

    // Turn 2: model execution that we will interrupt with Ctrl-C
    provider.push_delay(
        Duration::from_secs(5),
        ScriptedResponse::text("this delayed response should not finish"),
    );

    let key_env = "DARIUS_TEST_JOURNEY_KEY";
    let key_val = "secret-key-123";

    let mut harness = PtyTestHarness::spawn_with_options(
        &[],
        &[(key_env, key_val)],
        |home, _ws| {
            let profile_dir = home.join("profiles/default");
            std::fs::create_dir_all(&profile_dir)?;
            let config = format!(
                "[model]\nprovider = \"custom-provider\"\nbase_url = \"{provider_url}/v1\"\nmodel = \"custom-model\"\napi_key_env = \"{key_env}\"\n"
            );
            std::fs::write(profile_dir.join("config.toml"), config)?;
            Ok(())
        },
    )
    .expect("spawn failed");

    harness
        .read_until("Welcome back", Duration::from_secs(5))
        .expect("PTY should render welcome card");

    // Turn 1: Submit goal that invokes write_file tool
    harness
        .write_bytes(b"write greeting file\r")
        .expect("send goal");

    // Wait for permission prompt to appear
    harness
        .read_until("Permission Required", Duration::from_secs(5))
        .expect("permission prompt should appear");

    // Press Enter to accept "Allow once"
    harness.write_bytes(b"\r").expect("accept permission");

    // Wait for model completion and turn 1 Done
    let turn1_out = harness
        .read_until("Successfully wrote greeting.txt!", Duration::from_secs(5))
        .expect("agent should finish writing file");

    if !turn1_out.contains("Done") {
        harness
            .read_until("Done", Duration::from_secs(2))
            .expect("turn 1 should complete to Done");
    }

    // Verify workspace artifact was actually written
    let created_file = harness.workspace_path().join("greeting.txt");
    assert!(
        created_file.exists(),
        "greeting.txt should have been created"
    );
    let content = std::fs::read_to_string(&created_file).expect("read greeting.txt");
    assert_eq!(content, "hello from darius agent journey\n");

    let initial_count = provider.request_count();

    // Turn 2: Submit long goal and interrupt with Ctrl-C
    harness
        .write_bytes(b"long task to interrupt\r")
        .expect("send long goal");

    // Wait until goal appears in transcript (confirming turn has started in TUI)
    harness
        .read_until("long task to interrupt", Duration::from_secs(5))
        .expect("turn 2 goal should appear in transcript");

    // Wait until fake provider receives the request
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while std::time::Instant::now() < deadline {
        if provider.request_count() > initial_count {
            started = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(started, "Turn 2 should have reached the provider");

    // Send Ctrl-C to interrupt the turn
    harness.write_bytes(&[0x03]).expect("send Ctrl-C interrupt");

    // Wait for turn to be cancelled and return to Done
    harness
        .read_until("Done", Duration::from_secs(5))
        .expect("should return to Done state after interrupt");

    // Send /quit to exit TUI
    harness.write_bytes(b"/quit\r").expect("send /quit");
    let exit_code = harness
        .wait_with_timeout(Duration::from_secs(5))
        .expect("wait failed")
        .expect("process should exit within 5s");
    assert_eq!(exit_code, 0, "full_agent_journey should exit 0");

    harness.wait_for_reader(Duration::from_secs(1)).unwrap();
    harness.assert_home_unchanged("full_agent_journey");
}
