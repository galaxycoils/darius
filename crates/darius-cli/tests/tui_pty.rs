use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn cargo_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_darius")
        .map(Into::into)
        .expect("Cargo must provide CARGO_BIN_EXE_darius for CLI integration tests")
}

fn read_until(reader: &mut dyn Read, needle: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut all = String::new();
    let mut buf = [0_u8; 4096];
    while Instant::now() < deadline {
        if let Ok(n) = reader.read(&mut buf) {
            if n > 0 {
                all.push_str(&String::from_utf8_lossy(&buf[..n]));
                if all.contains(needle) { return all; }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {needle:?}; output={all:?}");
}

#[test]
fn tui_accepts_plain_text_and_quits() {
    let pair = native_pty_system().openpty(PtySize {
        rows: 24, cols: 80, pixel_width: 0, pixel_height: 0,
    }).unwrap();
    let mut cmd = CommandBuilder::new(cargo_bin());
    cmd.args(["tui", "--profile", "tui-e2e"]);
    cmd.env("TERM", "xterm-256color");
    cmd.env("DARIUS_TUI_TEST_MODE", "1");
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    read_until(&mut *reader, "◆ darius", Duration::from_secs(5));
    writer.write_all(b"hello world\r").unwrap();
    let output = read_until(&mut *reader, "Done", Duration::from_secs(5));
    assert!(output.contains("hello world"));
    writer.write_all(b"/quit\r").unwrap();
    assert!(child.wait().unwrap().success());
}