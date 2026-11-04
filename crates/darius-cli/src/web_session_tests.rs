use crate::session_test_support::{Observer, runtime};
use darius_cognitive::ChannelEventSink;
use std::sync::{Arc, Mutex};

#[test]
fn serve_executor_passes_each_isolated_runtime_session_to_model() {
    let temp = tempfile::tempdir().unwrap();
    let seen = Arc::new(Mutex::new(vec![]));
    let mut ids = Vec::new();
    let (tx, _rx) = std::sync::mpsc::channel();
    for _ in 0..2 {
        let mut runtime = runtime(&temp);
        ids.push(runtime.session_id().to_owned());
        runtime.model = Box::new(Observer(seen.clone()));
        let sink = Arc::new(ChannelEventSink::new(tx.clone()));
        assert_eq!(super::execute(runtime, "hello", sink).unwrap(), "ok");
    }
    assert_ne!(ids[0], ids[1]);
    assert_eq!(*seen.lock().unwrap(), ids);
}
