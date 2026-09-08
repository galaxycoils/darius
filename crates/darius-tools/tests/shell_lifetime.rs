#![cfg(unix)]
use darius_tools::{
    ExecutionContext, ToolCall, ToolOutcome, ToolRegistry, register_coding_builtins,
};
use std::time::{Duration, Instant};

#[test]
fn shell_success_and_failure_terminate_background_children() {
    for exit in [0, 7] {
        let dir = tempfile::tempdir().unwrap();
        let mut tools =
            ToolRegistry::new_with_roots(dir.path(), &dir.path().join("spill")).unwrap();
        register_coding_builtins(&mut tools);
        let call = ToolCall {
            id: "background".into(),
            name: "shell".into(),
            arguments: serde_json::json!({"command": format!(
                "sleep 30 & child=$!; kill -0 $child || exit 99; printf '%s' $child > child.pid; exit {exit}"
            )}),
        };
        let ctx = ExecutionContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + Duration::from_secs(3),
        };
        let outcome = tools.execute_model_with_context(&call, &ctx);
        let pid: i32 = std::fs::read_to_string(dir.path().join("child.pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(pid > 1);
        let deadline = Instant::now() + Duration::from_secs(1);
        while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let survived = unsafe { libc::kill(pid, 0) } == 0;
        if survived {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
        assert!(!survived, "background child survived shell exit {exit}");
        if exit == 0 {
            assert!(matches!(outcome, ToolOutcome::Ok { .. }), "{outcome:?}");
        } else {
            assert!(matches!(outcome, ToolOutcome::Err { .. }), "{outcome:?}");
        }
    }
}
