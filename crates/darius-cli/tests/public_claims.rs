use darius_cli::{paths::DariusPaths, runtime::SessionRuntime, tui_runtime::TuiWorker};
use darius_cognitive::UiEvent;
use darius_core::commands::{COMMANDS, parse_invocation};
use darius_tui::RuntimeCommand;

#[test]
fn generated_slash_help_and_status_are_truthful() {
    let temp = tempfile::tempdir().unwrap();
    let paths = DariusPaths {
        home: temp.path().join("home"),
        workspace: temp.path().into(),
    };
    let options = darius_cli::runtime::RuntimeOptions { offline: true };
    let runtime = SessionRuntime::from_options(&paths, "claims", options).unwrap();
    let (mut worker, mut events) = TuiWorker::new(runtime);
    let (commands, receiver) = tokio::sync::mpsc::unbounded_channel();
    for text in ["/help", "/status"] {
        commands
            .send(RuntimeCommand::ExecuteSlash(
                parse_invocation(text).unwrap(),
            ))
            .unwrap();
    }
    commands.send(RuntimeCommand::Shutdown).unwrap();
    worker.run_loop(receiver);
    let mut output = String::new();
    while let Ok(envelope) = events.try_recv() {
        assert!(!matches!(envelope.event, UiEvent::Done));
        if let UiEvent::Status { line } = envelope.event {
            output.push_str(&line);
        }
    }
    assert!(output.contains("Available commands:"));
    assert!(output.contains("Runtime state: offline-demo"));
    for command in COMMANDS {
        assert!(output.contains(command.name));
    }
    for hidden in [
        "cron",
        "approval-check",
        "peer_send",
        "mcp",
        "subagent",
        "worktree",
        "rollback",
        "a2a",
    ] {
        assert!(
            !output.to_lowercase().contains(hidden),
            "unsupported: {hidden}"
        );
    }
}
