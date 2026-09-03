//! PTY-based TUI integration tests.
//!
//! These tests use portable-pty to spawn the real `darius` binary in a
//! pseudo-terminal, exercising the full TUI lifecycle (setup, input, exit).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tempfile::TempDir;

/// Snapshot of ~/.darius state before/after a test to verify no pollution.
#[derive(Debug, Default)]
struct DariusHomeSnapshot {
    exists: bool,
    metadata: Option<std::fs::Metadata>,
}

impl DariusHomeSnapshot {
    fn capture() -> Self {
        let path = dirs::home_dir().map(|h| h.join(".darius"));
        let (exists, metadata) = path
            .as_ref()
            .map(|p| (p.exists(), std::fs::metadata(p).ok()))
            .unwrap_or((false, None));
        Self { exists, metadata }
    }

    fn assert_unchanged(&self, label: &str) {
        let after = Self::capture();
        assert_eq!(
            self.exists, after.exists,
            "{label}: ~/.darius existence changed (before={}, after={})",
            self.exists, after.exists
        );
        if let (Some(before), Some(after)) = (self.metadata.as_ref(), after.metadata.as_ref()) {
            assert_eq!(
                before.modified().ok(),
                after.modified().ok(),
                "{label}: ~/.darius mtime changed",
            );
        }
    }
}

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
        let bin = std::env::var_os("CARGO_BIN_EXE_darius")
            .ok_or("CARGO_BIN_EXE_darius not set — run via `cargo test`")?;
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
        cmd.env("DARIUS_HOME", darius_home.path());
        cmd.env("DARIUS_WORKSPACE", workspace.path());
        cmd.env("HOME", darius_home.path()); // Also redirect HOME so ~/.darius = temp
        cmd.env_remove("DARIUS_API_KEY"); // Ensure no API key leaks in
        cmd.env_remove("DARIUS_PROFILE"); // No profile preset

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
