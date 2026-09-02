//! Public CLI contract tests — RED-only (these tests should FAIL initially)
//!
//! This test file defines the expected public contract of the darius CLI.
//! Tests assert what the public API surface should be, and they are expected
//! to fail until the CLI implementation is updated to match.

use assert_cmd::Command;
use std::process::Output;

fn run(args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("darius").expect("darius binary not found");
    cmd.args(args);
    cmd.assert()
}

fn run_raw(args: &[&str]) -> Output {
    let mut cmd = Command::cargo_bin("darius").expect("darius binary not found");
    cmd.args(args);
    cmd.output().expect("spawn failed")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn public_help_matches_recovery_surface() {
    let help = run(&["--help"]);
    let out = stdout(&help.get_output());
    help.success();
    for shown in ["tui", "run", "config", "memory"] {
        assert!(out.contains(shown), "help should show '{shown}'");
    }
    for hidden in [
        "daemon", "status", "start", "stop", "attach", "eval", "learn",
        "session-smoke", "serve", "a2a", "cron", "approval-check",
    ] {
        assert!(!out.contains(hidden), "help should NOT leak '{hidden}'");
    }
}

#[test]
fn unknown_command_exits_two() {
    let output = run_raw(&["wat"]);
    assert_eq!(output.status.code(), Some(2), "unknown command should exit 2");
}

#[test]
fn removed_tokens_all_exit_two() {
    // Every hidden/removed command token must exit 2 (not 0, not 1)
    for token in [
        "daemon", "status", "start", "stop", "attach", "eval", "learn",
        "session-smoke", "serve", "a2a", "cron", "approval-check", "help",
    ] {
        let output = run_raw(&[token]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "removed token '{token}' should exit 2, got {:?}",
            output.status.code()
        );
    }
}

#[test]
fn version_flag_works() {
    let output = run_raw(&["--version"]);
    assert!(output.status.success(), "--version should succeed");
    let out = stdout(&output);
    assert!(out.starts_with("darius "), "version output should start with 'darius '");
}

#[test]
fn version_flag_short_works() {
    let output = run_raw(&["-V"]);
    assert!(output.status.success(), "-V should succeed");
    let out = stdout(&output);
    assert!(out.starts_with("darius "), "version output should start with 'darius '");
}

#[test]
fn global_flags_before_subcommand() {
    // --profile and --session before subcommand must be recognized and
    // the help flag after the subcommand must still work.
    let out = run_raw(&["--profile", "default", "tui", "--help"]);
    assert!(out.status.success(), "--profile before subcommand should work");
    let text = stdout(&out);
    assert!(text.contains("Usage: darius <command>"), "help should show usage");

    let out = run_raw(&["--session", "abc123", "tui", "--help"]);
    assert!(out.status.success(), "--session before subcommand should work");
    let text = stdout(&out);
    assert!(text.contains("Usage: darius <command>"), "help should show usage");
}

#[test]
fn global_flags_after_subcommand() {
    // --profile and --session after subcommand must still be recognized and
    // the help flag must work.
    let out = run_raw(&["tui", "--profile", "default", "--help"]);
    assert!(out.status.success(), "--profile after subcommand should work");
    let text = stdout(&out);
    assert!(text.contains("Usage: darius <command>"), "help should show usage");

    let out = run_raw(&["tui", "--session", "abc123", "--help"]);
    assert!(out.status.success(), "--session after subcommand should work");
    let text = stdout(&out);
    assert!(text.contains("Usage: darius <command>"), "help should show usage");
}

#[test]
fn malformed_nested_args() {
    // Missing argument after --profile
    let output = run_raw(&["--profile"]);
    assert_eq!(output.status.code(), Some(2), "missing --profile value should exit 2");
    
    // Missing argument after --session
    let output = run_raw(&["--session"]);
    assert_eq!(output.status.code(), Some(2), "missing --session value should exit 2");
    
    // Unknown flag
    let output = run_raw(&["--unknown-flag"]);
    assert_eq!(output.status.code(), Some(2), "unknown flag should exit 2");
}

#[test]
fn no_arg_non_tty_help() {
    // When no args and not a TTY, should print help and exit 0 (not error)
    let output = run_raw(&[]);
    assert!(output.status.success(), "no args should show help and exit 0");
    let out = stdout(&output);
    assert!(out.contains("No API key configured"), "bare invoke should show setup hint");
    assert!(!out.contains("Usage: darius <command>"), "bare invoke should NOT show full usage table");
}