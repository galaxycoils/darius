use super::*;
use darius_cognitive::DiffKind;
use darius_tui::{Action, AppState, DiffLineKind, Effect, TranscriptItem};
#[test]
fn mode_runtime_change_updates_ui_and_next_submission_mode() {
    let mut state = AppState::default();
    state.composer.input = "/mode plan".into();
    let effect = state.reduce(Action::Submit);
    assert!(matches!(effect, Some(Effect::ExecuteCommand(ref inv)) if inv.args == "plan"));
    let event: UiEvent =
        serde_json::from_value(serde_json::json!({"type":"mode_changed","mode":"plan"}))
            .expect("typed mode change event");
    state.apply_event(event);
    assert_eq!(state.mode, Mode::Plan);
}

#[test]
fn mode_slash_changes_session_and_rejects_invalid_mode() {
    let mut h = Harness::new("http://127.0.0.1:1", Some(vec![text()]));
    for (command, expected) in [
        ("/mode plan", "plan"),
        ("/mode", "auto"),
        ("/mode auto", "auto"),
    ] {
        h.commands.send(slash(command)).unwrap();
        let events = h.until(|e| serde_json::to_value(e).unwrap()["type"] == "mode_changed");
        assert_eq!(
            serde_json::to_value(events.last().unwrap()).unwrap()["mode"],
            expected
        );
    }
    h.commands
        .send(RuntimeCommand::ExecuteSlash(
            darius_tui::CommandInvocation {
                id: darius_tui::CommandId::Mode,
                name: "/mode".into(),
                args: "manual".into(),
            },
        ))
        .unwrap();
    h.until(|e| matches!(e,UiEvent::Error {message} if message.contains("auto or plan")));
    h.shutdown();
}
#[test]
fn permission_lifecycle_prompt_and_approval_listing_redact_secrets() {
    let secret = "sk-proj1234567890abcdef1234";
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call(
                "shell",
                json!({"command":format!("printf '%s' '{secret}'")}),
            ),
            text(),
        ]),
    );
    h.submit("approve");
    let events = h.until(|e| matches!(e, UiEvent::PermissionRequired { .. }));
    let UiEvent::PermissionRequired { id, command, .. } = events.last().unwrap() else {
        unreachable!()
    };
    assert!(!command.contains(secret), "prompt leaked secret");
    h.commands
        .send(RuntimeCommand::ResolvePermission {
            id: id.clone(),
            choice: PermissionChoice::AllowSession,
        })
        .unwrap();
    h.done(false);
    h.commands.send(slash("/permissions")).unwrap();
    let events = h.until(|e| matches!(e,UiEvent::Status{line} if line.starts_with("shell:")));
    assert!(
        !format!("{events:?}").contains(secret),
        "approval listing leaked secret"
    );
    h.shutdown();
}
#[test]
fn permission_lifecycle_session_exact_shell_command_and_complete_task_arguments() {
    let task = |args| call("task_add", args);
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call("shell", json!({"command":"printf one > exact.txt"})),
            text(),
            call("shell", json!({"command":"printf one > exact.txt"})),
            text(),
            call("shell", json!({"command":"printf two > exact.txt"})),
            text(),
            task(json!({"title":"same","metadata":{"z":2,"a":1}})),
            text(),
            task(json!({"metadata":{"a":1,"z":2},"title":"same"})),
            text(),
            task(json!({"title":"same","metadata":{"a":9,"z":2}})),
            text(),
        ]),
    );
    h.submit("shell");
    h.permit(PermissionChoice::AllowSession);
    h.done(false);
    h.submit("exact");
    completed(&mut h);
    h.submit("different command");
    h.permit(PermissionChoice::Deny);
    h.done(false);
    assert_eq!(
        std::fs::read_to_string(h.temp.path().join("exact.txt")).unwrap(),
        "one"
    );
    h.submit("task");
    h.permit(PermissionChoice::AllowSession);
    h.done(false);
    h.submit("reordered");
    completed(&mut h);
    h.submit("different JSON");
    h.permit(PermissionChoice::Deny);
    h.done(false);
    h.shutdown();
}
fn write(path: &str, content: &str) -> ModelOutput {
    call("write_file", json!({"path":path,"content":content}))
}
fn completed(h: &mut Harness) -> Vec<UiEvent> {
    let events = h.until(|e| matches!(e, UiEvent::Done | UiEvent::PermissionRequired { .. }));
    assert!(
        matches!(events.last(), Some(UiEvent::Done)),
        "unexpected approval: {events:?}"
    );
    events
}
#[test]
fn permission_lifecycle_session_normalizes_path_across_turns_but_not_targets() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            write("cached.txt", "one"),
            text(),
            write("./cached.txt", "two"),
            text(),
            write("other.txt", "three"),
            text(),
        ]),
    );
    h.submit("first");
    h.permit(PermissionChoice::AllowSession);
    h.done(false);
    h.submit("normalized");
    completed(&mut h);
    assert_eq!(
        std::fs::read_to_string(h.temp.path().join("cached.txt")).unwrap(),
        "two"
    );
    h.submit("different");
    h.permit(PermissionChoice::Deny);
    h.done(false);
    assert!(!h.temp.path().join("other.txt").exists());
    h.shutdown();
}
#[test]
fn permission_lifecycle_once_does_not_persist_and_denial_is_correlated() {
    let denied = write("once.txt", "two");
    let id = denied.tool_calls[0].id.clone();
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![write("once.txt", "one"), text(), denied, text()]),
    );
    h.submit("first");
    h.permit(PermissionChoice::AllowOnce);
    h.done(false);
    h.submit("again");
    h.permit(PermissionChoice::Deny);
    let events = completed(&mut h);
    assert!(events.iter().any(|e| matches!(e, UiEvent::ToolEnd { id: actual, ok: false, preview, .. } if actual == &id && preview.contains("denied"))));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::AssistantDelta { text } if text == "finished"))
    );
    assert_eq!(
        std::fs::read_to_string(h.temp.path().join("once.txt")).unwrap(),
        "one"
    );
    h.shutdown();
}
#[test]
fn permission_lifecycle_interrupt_clears_pending_and_later_turn_prompts() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            write("cancel.txt", "no"),
            write("cancel.txt", "yes"),
            text(),
        ]),
    );
    h.submit("cancel");
    let events = h.until(|e| matches!(e, UiEvent::PermissionRequired { .. }));
    let UiEvent::PermissionRequired { id, .. } = events.last().unwrap() else {
        unreachable!()
    };
    let stale_id = id.clone();
    h.commands.send(RuntimeCommand::Interrupt).unwrap();
    h.done(true);
    h.commands
        .send(RuntimeCommand::ResolvePermission {
            id: stale_id,
            choice: PermissionChoice::AllowSession,
        })
        .unwrap();
    assert!(!h.temp.path().join("cancel.txt").exists());
    h.submit("recover");
    h.permit(PermissionChoice::AllowOnce);
    h.done(false);
    assert_eq!(
        std::fs::read_to_string(h.temp.path().join("cancel.txt")).unwrap(),
        "yes"
    );
    h.shutdown();
}
#[test]
fn permission_lifecycle_eof_cancels_pending_and_joins() {
    let mut h = Harness::new("http://127.0.0.1:1", Some(vec![write("eof.txt", "no")]));
    h.submit("eof");
    h.until(|e| matches!(e, UiEvent::PermissionRequired { .. }));
    let (unused, _) = tokio::sync::mpsc::unbounded_channel();
    drop(std::mem::replace(&mut h.commands, unused));
    h.done(true);
    let deadline = Instant::now() + BOUND;
    while !h.join.as_ref().unwrap().is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.join.as_ref().unwrap().is_finished());
    h.join.take().unwrap().join().unwrap();
    assert!(!h.temp.path().join("eof.txt").exists());
}
#[test]
fn execution_policy_plan_denies_before_permission_and_readonly_runs() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            write("plan.txt", "no"),
            call("shell", json!({"command":"touch shell.txt"})),
            call("read_file", json!({"path":"input.txt"})),
            text(),
        ]),
    );
    std::fs::write(h.temp.path().join("input.txt"), "readable").unwrap();
    h.commands
        .send(RuntimeCommand::SubmitGoal {
            text: "plan".into(),
            mode: Mode::Plan,
        })
        .unwrap();
    let events = completed(&mut h);
    assert!(!h.temp.path().join("plan.txt").exists());
    assert!(!h.temp.path().join("shell.txt").exists());
    assert!(events.iter().any(
        |e| matches!(e, UiEvent::ToolEnd { ok:true, preview, .. } if preview.contains("readable"))
    ));
    h.shutdown();
}

#[test]
fn tui_allow_once_write_file_creates_disk_file() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![write("allowed_out.txt", "WRITE_OK_MARKER"), text()]),
    );
    h.submit("write it");
    h.permit(PermissionChoice::AllowOnce);
    h.done(false);
    assert_eq!(
        std::fs::read_to_string(h.temp.path().join("allowed_out.txt")).unwrap(),
        "WRITE_OK_MARKER"
    );
    h.shutdown();
}

#[test]
fn tui_deny_write_leaves_no_file() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![write("denied.txt", "no"), text()]),
    );
    h.submit("write it");
    h.permit(PermissionChoice::Deny);
    h.done(false);
    assert!(!h.temp.path().join("denied.txt").exists());
    h.shutdown();
}

#[test]
fn tui_allow_once_shell_echo_appears_in_tool_result() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call("shell", json!({"command":"echo SHELL_OK_TOKEN"})),
            text(),
        ]),
    );
    h.submit("run it");
    let events = h.until(|e| matches!(e, UiEvent::PermissionRequired { .. }));
    let UiEvent::PermissionRequired { id, .. } = events.last().unwrap() else {
        unreachable!()
    };
    h.commands
        .send(RuntimeCommand::ResolvePermission {
            id: id.clone(),
            choice: PermissionChoice::AllowOnce,
        })
        .unwrap();
    let events = h.until(|e| matches!(e, UiEvent::ToolEnd { .. }));
    assert!(events.iter().any(|e| matches!(
        e,
        UiEvent::ToolEnd {
            ok: true,
            preview,
            ..
        } if preview.contains("SHELL_OK_TOKEN")
    )));
    h.done(false);
    h.shutdown();
}

#[test]
fn tui_allow_session_shell_exact_command_caches_and_different_reprompts() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![
            call(
                "shell",
                json!({"command":"printf first > shell_session.txt"}),
            ),
            text(),
            call(
                "shell",
                json!({"command":"printf first > shell_session.txt"}),
            ),
            text(),
            call(
                "shell",
                json!({"command":"printf second > shell_session.txt"}),
            ),
            text(),
        ]),
    );
    h.submit("run first shell");
    h.permit(PermissionChoice::AllowSession);
    h.done(false);
    h.submit("run first shell again");
    completed(&mut h);
    h.submit("run different shell");
    h.permit(PermissionChoice::Deny);
    h.done(false);
    assert_eq!(
        std::fs::read_to_string(h.temp.path().join("shell_session.txt")).unwrap(),
        "first"
    );
    h.shutdown();
}

#[test]
fn tui_write_diff_appears_in_transcript_on_overwrite() {
    let mut h = Harness::new(
        "http://127.0.0.1:1",
        Some(vec![write("diff_target.txt", "new\ncontent\nhere"), text()]),
    );
    std::fs::write(
        h.temp.path().join("diff_target.txt"),
        "original\ncontent\nhere",
    )
    .unwrap();
    h.submit("overwrite it");
    h.permit(PermissionChoice::AllowOnce);
    let events = h.until(|e| matches!(e, UiEvent::Done));
    assert!(!events.iter().any(|e| matches!(e, UiEvent::Error { .. })));
    let diff = events.iter().find(|e| matches!(e, UiEvent::Diff { .. }));
    assert!(diff.is_some(), "expected UiEvent::Diff on overwrite");
    let UiEvent::Diff { file, lines, .. } = diff.unwrap() else {
        unreachable!()
    };
    assert!(file.ends_with("diff_target.txt"));
    assert!(!lines.is_empty());
    assert!(
        lines
            .iter()
            .any(|l| l.kind == DiffKind::Delete && l.text.contains("original"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.kind == DiffKind::Add && l.text.contains("new"))
    );

    let mut state = AppState::default();
    state.apply_event(diff.unwrap().clone());
    assert!(matches!(
        state.transcript.last(),
        Some(TranscriptItem::Diff { diff }) if diff.lines.iter().any(|l| l.kind == DiffLineKind::Add && l.text.contains("new"))
    ));
    h.shutdown();
}

#[test]
fn tui_approval_gates_mutating_mcp_tools_and_plan_denies() {
    let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../darius-tools/tests/fixtures/mock_mcp.py")
        .canonicalize()
        .unwrap();

    let mcp_toml = format!(
        r#"[[mcp.servers]]
name = "mock"
type = "stdio"
command = "python3"
args = ["{}"]
env = {{ "MOCK_MCP_READONLY" = "0" }}
"#,
        script.to_string_lossy()
    );

    // 1. Auto mode: mutating tool call triggers PermissionRequired, AllowOnce executes it
    let mut h = Harness::new_with_mcp(
        "http://127.0.0.1:1",
        Some(vec![
            call("mcp_mock_echo", json!({"text": "MUTATING_PERMITTED"})),
            text(),
        ]),
        &mcp_toml,
    );
    h.submit("call mutating mcp");
    let events = h.until(|e| matches!(e, UiEvent::PermissionRequired { .. }));
    let UiEvent::PermissionRequired {
        id, title, command, ..
    } = events.last().unwrap()
    else {
        unreachable!()
    };
    assert!(title.contains("mcp_mock_echo"));
    assert!(command.contains("MUTATING_PERMITTED"));
    h.commands
        .send(RuntimeCommand::ResolvePermission {
            id: id.clone(),
            choice: PermissionChoice::AllowOnce,
        })
        .unwrap();
    let events = h.until(|e| matches!(e, UiEvent::ToolEnd { .. }));
    assert!(events.iter().any(|e| matches!(
        e,
        UiEvent::ToolEnd {
            ok: true,
            preview,
            ..
        } if preview.contains("MUTATING_PERMITTED")
    )));
    h.done(false);
    h.shutdown();

    // 2. Plan mode: mutating MCP tool is denied before permission prompt
    let mut h_plan = Harness::new_with_mcp(
        "http://127.0.0.1:1",
        Some(vec![
            call("mcp_mock_echo", json!({"text": "SHOULD_BE_DENIED"})),
            text(),
        ]),
        &mcp_toml,
    );
    h_plan
        .commands
        .send(RuntimeCommand::SubmitGoal {
            text: "plan mode call".into(),
            mode: Mode::Plan,
        })
        .unwrap();
    let plan_events = completed(&mut h_plan);
    assert!(
        !plan_events
            .iter()
            .any(|e| matches!(e, UiEvent::PermissionRequired { .. }))
    );
    assert!(
        plan_events
            .iter()
            .any(|e| matches!(e, UiEvent::ToolEnd { ok: false, .. }))
    );
    h_plan.shutdown();
}
