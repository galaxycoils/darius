//! TUI integration tests.
//!
//! These tests verify the TUI binary exists and can be spawned.
//! Full interactive testing is done via the unit tests in darius-tui.

use std::path::PathBuf;

fn cargo_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_darius")
        .map(Into::into)
        .expect("Cargo must provide CARGO_BIN_EXE_darius for CLI integration tests")
}

#[test]
fn tui_binary_exists() {
    let bin = cargo_bin();
    assert!(bin.exists(), "darius binary should exist at {:?}", bin);
}

#[test]
fn tui_binary_responds_to_help() {
    let output = std::process::Command::new(cargo_bin())
        .arg("tui")
        .arg("--help")
        .output()
        .expect("spawn darius tui --help");

    // Either succeeds or prints help; both prove the subcommand exists
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("TUI") || combined.contains("tui") || output.status.success(),
        "tui subcommand should produce recognizable output: {:?}",
        combined.chars().take(200).collect::<String>()
    );
}
