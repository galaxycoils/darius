//! PTY-based TUI integration tests.
//!
//! These tests use portable-pty to spawn the real `darius` binary in a
//! pseudo-terminal, exercising the full TUI lifecycle (setup, input, exit).
//! They are RED by design — they document expected behavior before implementation.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use tempfile::TempDir;

/// Snapshot of ~/.darius state before/after a test to verify no pollution.
#[derive(Debug, Default)]
struct DariusHomeSnapshot {
    path: Option<PathBuf>,
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
        Self {
            path,
            exists,
            metadata,
        }
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
    pair: PtyPair,
    child: Box<dyn portable_pty::Child + Send>,
    reader: BufReader<Box<dyn std::io::Read + Send>>,
    writer: Box<dyn Write + Send>,
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
        assert!(bin_path.exists(), "darius binary not found at {:?}", bin_path);

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
        let reader = BufReader::new(pair.master.try_clone_reader()?);
        let writer = pair.master.take_writer()?;

        Ok(Self {
            pair,
            child,
            reader,
            writer,
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
        let mut buf = String::new();
        let mut line = String::new();

        while std::time::Instant::now() < deadline {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    buf.push_str(&line);
                    if buf.contains(needle) {
                        return Ok(buf);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }

        Err(format!("timeout waiting for {:?}; got: {}", needle, buf).into())
    }

    /// Write a line to the PTY (with newline).
    fn write_line(&mut self, line: &str) -> Result<(), Box<dyn std::error::Error>> {
        writeln!(self.writer, "{}", line)?;
        self.writer.flush()?;
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
        // Timeout: kill child
        let _ = self.child.kill();
        let _ = self.child.wait()?;
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
        // Best-effort cleanup: kill child if still alive
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// RED TEST: bare `darius` launch with empty temp home and no API key.
///
/// Expected behavior (not yet implemented):
/// - Prints a one-line setup hint (does NOT open interactive TUI)
/// - Exits 0 cleanly
/// - Restores cursor/shell within 5s (no hanging PTY)
///
/// Current behavior (broken):
/// - Opens TUI immediately or hangs
/// - Does not show setup guidance
#[test]
fn clean_home_bare_launch() {
    // Spawn bare `darius` (no subcommand) with clean temp home
    let mut harness = PtyTestHarness::spawn(&[]).expect("spawn failed");

    // Expect the setup hint to appear (not the TUI, not the full usage table).
    // With DARIUS_HOME set but no API key configured, the hint points at
    // `darius help` and `darius config`.
    let output = harness
        .read_until("Run `darius help`", Duration::from_secs(5))
        .expect("should print setup hint for bare invocation");

    // Strip ANSI only for assertions
    let clean = PtyTestHarness::strip_ansi(&output);

    // Verify it printed the setup hint, not the full usage table
    assert!(
        clean.contains("Run `darius help`"),
        "bare invocation should print setup hint, got: {}",
        clean
    );
    assert!(
        !clean.contains("Usage: darius <command>"),
        "bare invocation should NOT print the full usage table, got: {}",
        clean
    );

    // Send /quit (should be no-op if already exited, but harmless)
    let _ = harness.write_line("/quit");

    // Verify clean exit within 5s
    let exit_code = harness
        .wait_with_timeout(Duration::from_secs(5))
        .expect("wait failed")
        .expect("process should exit within 5s");

    assert_eq!(exit_code, 0, "bare invocation should exit 0, got {}", exit_code);

    // Verify ~/.darius untouched
    harness.assert_home_unchanged("clean_home_bare_launch");
}