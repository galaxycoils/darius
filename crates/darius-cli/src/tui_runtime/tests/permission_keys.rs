use super::*;
struct Control {
    control: Arc<ChannelRunControl>,
    events: std::sync::mpsc::Receiver<UiEvent>,
}
impl Control {
    fn new(workspace: &std::path::Path, cache: crate::runtime::SessionPermissions) -> Self {
        let (tx, events) = std::sync::mpsc::channel();
        let sink = Arc::new(darius_cognitive::ChannelEventSink::new(tx));
        let mut control = ChannelRunControl::new(
            sink,
            Default::default(),
            darius_tools::PathPolicy::new(workspace).unwrap(),
        );
        control.session_cache = cache;
        Self {
            control: Arc::new(control),
            events,
        }
    }
    fn ask(&self, name: &str, args: serde_json::Value, choice: Option<PermissionChoice>) {
        let call = call(name, args).tool_calls.remove(0);
        let control = self.control.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            let answer = control.approve_tool(
                &call,
                darius_tools::model_tools::model_tool_risk(&call.name).unwrap(),
            );
            tx.send(answer).unwrap();
        });
        if let Some(choice) = choice {
            let UiEvent::PermissionRequired { id, .. } = self.events.recv_timeout(BOUND).unwrap()
            else {
                panic!("not a permission request")
            };
            self.control.resolve(&id, choice);
        }
        let result = rx.recv_timeout(BOUND);
        self.control.cancellation.cancel(); // Also bound teardown when an assertion fails.
        join.join().unwrap();
        assert!(result.unwrap().is_ok());
        assert!(self.control.pending.lock().unwrap().is_empty());
        if choice.is_none() {
            assert!(self.events.try_recv().is_err());
        }
    }
}
#[test]
fn permission_lifecycle_canonical_absolute_paths_share_only_session_cache() {
    let temp = tempfile::tempdir().unwrap();
    let cache: crate::runtime::SessionPermissions = Arc::default();
    Control::new(temp.path(), cache.clone()).ask(
        "write_file",
        json!({"path":"target.txt"}),
        Some(PermissionChoice::AllowSession),
    );
    Control::new(temp.path(), cache).ask(
        "write_file",
        json!({"path":temp.path().canonicalize().unwrap().join("target.txt")}),
        None,
    );
    Control::new(temp.path(), Arc::default()).ask(
        "write_file",
        json!({"path":"target.txt"}),
        Some(PermissionChoice::Deny),
    );
}
#[test]
fn permission_lifecycle_shell_key_binds_canonical_workspace_and_exact_command() {
    let temp = tempfile::tempdir().unwrap();
    let cache: crate::runtime::SessionPermissions = Arc::default();
    Control::new(&temp.path().join("one"), cache.clone()).ask(
        "shell",
        json!({"command":"true"}),
        Some(PermissionChoice::AllowSession),
    );
    Control::new(&temp.path().join("one/./"), cache.clone()).ask(
        "shell",
        json!({"command":"true"}),
        None,
    );
    Control::new(&temp.path().join("two"), cache.clone()).ask(
        "shell",
        json!({"command":"true"}),
        Some(PermissionChoice::Deny),
    );
    Control::new(&temp.path().join("one"), cache).ask(
        "shell",
        json!({"command":"true "}),
        Some(PermissionChoice::Deny),
    );
}
#[test]
fn permission_lifecycle_cancel_clears_pending_sender_without_grant() {
    let temp = tempfile::tempdir().unwrap();
    let c = Control::new(temp.path(), Arc::default());
    let control = c.control.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let join = std::thread::spawn(move || {
        tx.send(control.approve_tool(
            &call("write_file", json!({"path":"cancel.txt"})).tool_calls[0],
            ToolRisk::Mutating,
        ))
        .unwrap()
    });
    let UiEvent::PermissionRequired { id, .. } = c.events.recv_timeout(BOUND).unwrap() else {
        unreachable!()
    };
    c.control.cancellation.cancel();
    assert!(matches!(
        rx.recv_timeout(BOUND).unwrap(),
        Err(darius_cognitive::CognitiveError::Cancelled)
    ));
    join.join().unwrap();
    assert!(c.control.pending.lock().unwrap().is_empty());
    c.control.resolve(&id, PermissionChoice::AllowSession);
    assert!(c.control.session_cache.lock().unwrap().is_empty());
}
