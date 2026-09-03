use darius_tools::{
    ExecutionContext, ToolCall, ToolOutcome, ToolRegistry, register_coding_builtins,
};

fn setup() -> (std::path::PathBuf, ToolRegistry, ToolCall) {
    let dir = std::env::temp_dir().join(format!("darius_context_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut tools = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
    register_coding_builtins(&mut tools);
    let call = ToolCall {
        id: "ctx-shell".into(),
        name: "shell".into(),
        arguments: serde_json::json!({"command": "sleep 2"}),
    };
    (dir, tools, call)
}

#[test]
#[cfg(unix)]
fn model_dispatch_uses_supplied_cancellation_token() {
    let (dir, tools, call) = setup();
    let cancel = tokio_util::sync::CancellationToken::new();
    let ctx = ExecutionContext {
        cancel: cancel.clone(),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
    };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(75));
        cancel.cancel();
    });
    let started = std::time::Instant::now();
    let outcome = tools.execute_model_with_context(&call, &ctx);
    assert!(matches!(outcome, ToolOutcome::Interrupted), "{outcome:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(unix)]
fn model_dispatch_uses_supplied_deadline() {
    let (dir, tools, call) = setup();
    let ctx = ExecutionContext {
        cancel: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() + std::time::Duration::from_millis(75),
    };
    let started = std::time::Instant::now();
    let outcome = tools.execute_model_with_context(&call, &ctx);
    assert!(matches!(outcome, ToolOutcome::TimedOut), "{outcome:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn model_dispatch_preflights_context_for_non_shell_tools() {
    let (dir, tools, _) = setup();
    std::fs::write(dir.join("present.txt"), "content").unwrap();
    let call = ToolCall {
        id: "ctx-read".into(),
        name: "read_file".into(),
        arguments: serde_json::json!({"path": "present.txt"}),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let cancelled = ExecutionContext {
        cancel,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(1),
    };
    assert!(matches!(
        tools.execute_model_with_context(&call, &cancelled),
        ToolOutcome::Interrupted
    ));
    let expired = ExecutionContext {
        cancel: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() - std::time::Duration::from_millis(1),
    };
    assert!(matches!(
        tools.execute_model_with_context(&call, &expired),
        ToolOutcome::TimedOut
    ));
    std::fs::remove_dir_all(dir).unwrap();
}
