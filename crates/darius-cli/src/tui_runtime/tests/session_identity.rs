use crate::session_test_support::{Observer, runtime};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn tui_session_identity_survives_turn_worker_recreation() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime(&temp);
    let id = runtime.session_id().to_owned();
    let seen = Arc::new(Mutex::new(vec![]));
    runtime.model = Box::new(Observer(seen.clone()));
    let (events, _rx) = tokio::sync::broadcast::channel(256);
    for turn in 1..=2 {
        let (join, _) = super::super::turn::spawn(
            runtime,
            "hello".into(),
            darius_core::runtime_protocol::TurnId(turn),
            events.clone(),
        );
        let result = join.await.unwrap();
        assert_eq!(result.outcome.unwrap(), "ok");
        runtime = result.runtime;
        assert_eq!(runtime.session_id(), id);
    }
    assert_eq!(*seen.lock().unwrap(), [id.clone(), id]);
}
