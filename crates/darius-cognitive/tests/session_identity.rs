use darius_cognitive::*;
use std::sync::{Arc, Mutex};

#[path = "support/session_observer.rs"]
mod session_observer;
use session_observer::Observer;

#[tokio::test]
async fn session_identity_reaches_model_and_stays_constant_across_turns() {
    let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
    let tools = darius_tools::ToolRegistry::new_with_roots(&dir, &dir.join("spill")).unwrap();
    let (tx, _rx) = std::sync::mpsc::channel();
    let seen = Arc::new(Mutex::new(vec![]));
    let mut model = Observer(seen.clone());
    let mut conversation = Conversation::from_messages(vec![]).unwrap();
    let meta = RunMetadata {
        profile: "test".into(),
        model: "observer".into(),
        mode: "auto".into(),
    };
    for _ in 0..2 {
        let agent = AgentLoop::new(
            Arc::new(ChannelEventSink::new(tx.clone())),
            Arc::new(NoopRunControl),
        )
        .with_session_id("opaque-session-a");
        agent
            .run_turn(
                &meta,
                &LoopPolicy::default(),
                "hello",
                &mut conversation,
                &mut model,
                &tools,
                &memory,
                dir.to_str().unwrap(),
            )
            .await
            .unwrap();
    }
    assert_eq!(
        *seen.lock().unwrap(),
        ["opaque-session-a", "opaque-session-a"]
    );
    drop(memory);
    std::fs::remove_dir_all(dir).unwrap();
}
