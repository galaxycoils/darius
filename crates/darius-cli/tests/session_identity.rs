use darius_cli::{
    paths::DariusPaths,
    runtime::{RuntimeOptions, SessionRuntime},
};
use darius_cognitive::*;
use std::sync::{Arc, Mutex};
#[path = "../../darius-cognitive/tests/support/session_observer.rs"]
mod session_observer;

#[tokio::test]
async fn runtime_session_identity_is_unique_and_reaches_model_across_turns() {
    let temp = tempfile::tempdir().unwrap();
    let paths = DariusPaths {
        home: temp.path().join("home"),
        workspace: temp.path().into(),
    };
    let mut runtime =
        SessionRuntime::from_options(&paths, "test", RuntimeOptions { offline: true }).unwrap();
    let other =
        SessionRuntime::from_options(&paths, "test", RuntimeOptions { offline: true }).unwrap();
    let id = runtime.session_id().to_owned();
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    assert_ne!(id, other.session_id());
    let seen = Arc::new(Mutex::new(vec![]));
    runtime.model = Box::new(session_observer::Observer(seen.clone()));
    let (tx, _rx) = std::sync::mpsc::channel();
    for _ in 0..2 {
        let agent = runtime.agent_loop(
            Arc::new(ChannelEventSink::new(tx.clone())),
            Arc::new(NoopRunControl),
        );
        agent
            .run_turn_with_extra_tools(
                &runtime.metadata,
                &runtime.policy,
                "hello",
                &mut runtime.conversation,
                runtime.model.as_mut(),
                &runtime.tools,
                &runtime.memory,
                paths.workspace.to_str().unwrap(),
                &runtime.dynamic_tool_specs,
            )
            .await
            .unwrap();
    }
    assert_eq!(*seen.lock().unwrap(), [id.clone(), id]);
}
