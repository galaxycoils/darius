//! Task 3.3 RED: one multi-turn coding agent loop over Conversation.
use darius_cognitive::{
    AgentLoop, AsyncModel, CognitiveError, Conversation, EventSink, LoopPolicy, MockModel,
    ModelOutput, NoopRunControl, PermissionChoice, RunControl, RunMetadata, ToolSpec, TurnContext,
    UiEvent, coding_system_prompt, compact_tool_results, model_tool_specs, transcript_chars,
};
use darius_tools::{ToolCall, ToolRegistry};
use std::sync::{Arc, Mutex};

struct Collect {
    events: Mutex<Vec<UiEvent>>,
}

impl EventSink for Collect {
    fn emit(&self, event: UiEvent) {
        self.events.lock().unwrap().push(event);
    }
}

fn collect() -> Arc<Collect> {
    Arc::new(Collect {
        events: Mutex::new(vec![]),
    })
}

struct GateControl {
    approvals: Mutex<usize>,
    deny: bool,
}

impl RunControl for GateControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn approve_tool(
        &self,
        _call: &ToolCall,
        _risk: darius_tools::ToolRisk,
    ) -> Result<PermissionChoice, CognitiveError> {
        *self.approvals.lock().unwrap() += 1;
        if self.deny {
            Ok(PermissionChoice::Deny)
        } else {
            Ok(PermissionChoice::AllowOnce)
        }
    }
}

struct CancelledControl;

impl RunControl for CancelledControl {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn approve_tool(
        &self,
        _call: &ToolCall,
        _risk: darius_tools::ToolRisk,
    ) -> Result<PermissionChoice, CognitiveError> {
        Err(CognitiveError::Cancelled)
    }
}

struct FailModel;

#[async_trait::async_trait]
impl AsyncModel for FailModel {
    async fn complete(
        &mut self,
        _messages: &[darius_cognitive::Message],
        _tools: &[ToolSpec],
        _ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        Err(CognitiveError::Loop("boom".into()))
    }
}

fn text_out(text: &str) -> ModelOutput {
    ModelOutput {
        content: Some(text.into()),
        tool_calls: vec![],
    }
}

fn harness(
    builtins: bool,
) -> (
    std::path::PathBuf,
    darius_memory::MemoryEngine,
    ToolRegistry,
    RunMetadata,
    LoopPolicy,
) {
    let dir = std::env::temp_dir().join(format!("darius_agent_loop_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
    let mut tools = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
    if builtins {
        darius_tools::register_memory_builtins(&mut tools, &memory);
        darius_tools::register_coding_builtins(&mut tools);
    }
    let meta = RunMetadata {
        profile: "test".into(),
        model: "mock".into(),
        mode: "auto".into(),
    };
    (dir, memory, tools, meta, LoopPolicy::default())
}

fn done_count(events: &[UiEvent]) -> usize {
    events.iter().filter(|e| matches!(e, UiEvent::Done)).count()
}

#[tokio::test]
async fn agent_loop_text_terminal_emits_user_assistant_and_single_done() {
    let (dir, memory, tools, meta, policy) = harness(false);
    let sink = collect();
    let loopt = AgentLoop::new(sink.clone(), Arc::new(NoopRunControl));
    let mut model = MockModel::new(vec![text_out("fixed it")]);
    let mut convo = Conversation::from_messages(vec![]).unwrap();
    let ws = dir.to_string_lossy().to_string();
    let text = loopt
        .run_turn(
            &meta,
            &policy,
            "fix the bug",
            &mut convo,
            &mut model,
            &tools,
            &memory,
            &ws,
        )
        .await
        .unwrap();
    assert_eq!(text, "fixed it");
    assert_eq!(convo.messages().len(), 2);
    let events = sink.events.lock().unwrap();
    assert!(matches!(&events[0], UiEvent::Header { goal, .. } if goal == "fix the bug"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::UserMessage { text } if text == "fix the bug"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::AssistantDelta { text } if text == "fixed it"))
    );
    assert_eq!(done_count(&events), 1);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, UiEvent::Error { .. } | UiEvent::Interrupted { .. }))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn agent_loop_tool_round_trip_appends_correlated_result() {
    let (dir, memory, tools, meta, policy) = harness(true);
    let sink = collect();
    let loopt = AgentLoop::new(sink.clone(), Arc::new(NoopRunControl));
    let mut model = MockModel::new(vec![
        ModelOutput {
            content: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "memory_remember".into(),
                arguments: serde_json::json!({"body": "working on task"}),
            }],
        },
        text_out("done"),
    ]);
    let mut convo = Conversation::from_messages(vec![]).unwrap();
    let ws = dir.to_string_lossy().to_string();
    let text = loopt
        .run_turn(
            &meta,
            &policy,
            "remember this",
            &mut convo,
            &mut model,
            &tools,
            &memory,
            &ws,
        )
        .await
        .unwrap();
    assert_eq!(text, "done");
    assert_eq!(convo.messages().len(), 4);
    assert!(matches!(
        &convo.messages()[2],
        darius_cognitive::Message::Tool { tool_call_id, .. } if tool_call_id == "c1"
    ));
    let events = sink.events.lock().unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::ToolStart { id, .. } if id == "c1"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::ToolEnd { id, ok: true, .. } if id == "c1"))
    );
    assert_eq!(done_count(&events), 1);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, UiEvent::Error { .. } | UiEvent::Interrupted { .. }))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn agent_loop_hidden_tool_rejected_before_permission() {
    let (dir, memory, tools, meta, policy) = harness(false);
    let sink = collect();
    let control = Arc::new(GateControl {
        approvals: Mutex::new(0),
        deny: false,
    });
    let loopt = AgentLoop::new(sink.clone(), control.clone());
    let mut model = MockModel::new(vec![
        ModelOutput {
            content: None,
            tool_calls: vec![ToolCall {
                id: "h1".into(),
                name: "peer_send".into(),
                arguments: serde_json::json!({}),
            }],
        },
        text_out("done"),
    ]);
    let mut convo = Conversation::from_messages(vec![]).unwrap();
    let ws = dir.to_string_lossy().to_string();
    let text = loopt
        .run_turn(
            &meta,
            &policy,
            "phone home",
            &mut convo,
            &mut model,
            &tools,
            &memory,
            &ws,
        )
        .await
        .unwrap();
    assert_eq!(text, "done");
    assert_eq!(*control.approvals.lock().unwrap(), 0);
    assert!(matches!(
        &convo.messages()[2],
        darius_cognitive::Message::Tool { tool_call_id, content, .. }
            if tool_call_id == "h1" && content.contains("peer_send") && content.contains("h1")
    ));
    let events = sink.events.lock().unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiEvent::ToolEnd { id, ok: false, .. } if id == "h1"))
    );
    assert_eq!(done_count(&events), 1);
    assert!(!events.iter().any(|e| matches!(e, UiEvent::Error { .. })));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn agent_loop_denied_mutating_tool_resolves_permission() {
    let (dir, memory, tools, meta, policy) = harness(true);
    let sink = collect();
    let control = Arc::new(GateControl {
        approvals: Mutex::new(0),
        deny: true,
    });
    let loopt = AgentLoop::new(sink.clone(), control.clone());
    let target = dir.join("new.txt");
    let mut model = MockModel::new(vec![
        ModelOutput {
            content: None,
            tool_calls: vec![ToolCall {
                id: "w1".into(),
                name: "write_file".into(),
                arguments: serde_json::json!({
                    "path": target.to_string_lossy(),
                    "content": "denied content",
                }),
            }],
        },
        text_out("done"),
    ]);
    let mut convo = Conversation::from_messages(vec![]).unwrap();
    let ws = dir.to_string_lossy().to_string();
    let text = loopt
        .run_turn(
            &meta, &policy, "write it", &mut convo, &mut model, &tools, &memory, &ws,
        )
        .await
        .unwrap();
    assert_eq!(text, "done");
    assert_eq!(*control.approvals.lock().unwrap(), 1);
    assert!(!target.exists());
    let events = sink.events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        UiEvent::PermissionResolved {
            id,
            choice: darius_cognitive::PermissionChoice::Deny,
        } if id == "w1"
    )));
    assert!(matches!(
        &convo.messages()[2],
        darius_cognitive::Message::Tool { content, .. } if content.contains("permission denied")
    ));
    assert_eq!(done_count(&events), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn agent_loop_cancel_before_turn_emits_single_interrupted() {
    let (dir, memory, tools, meta, policy) = harness(false);
    let sink = collect();
    let loopt = AgentLoop::new(sink.clone(), Arc::new(CancelledControl));
    let mut model = MockModel::new(vec![text_out("never")]);
    let mut convo = Conversation::from_messages(vec![]).unwrap();
    let ws = dir.to_string_lossy().to_string();
    let err = loopt
        .run_turn(
            &meta, &policy, "do it", &mut convo, &mut model, &tools, &memory, &ws,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CognitiveError::Cancelled));
    assert!(convo.messages().is_empty());
    let events = sink.events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, UiEvent::Interrupted { .. }))
            .count(),
        1
    );
    assert_eq!(done_count(&events), 1);
    assert!(!events.iter().any(|e| matches!(e, UiEvent::Error { .. })));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn agent_loop_model_error_emits_single_error() {
    let (dir, memory, tools, meta, policy) = harness(false);
    let sink = collect();
    let loopt = AgentLoop::new(sink.clone(), Arc::new(NoopRunControl));
    let mut model = FailModel;
    let mut convo = Conversation::from_messages(vec![]).unwrap();
    let ws = dir.to_string_lossy().to_string();
    let err = loopt
        .run_turn(
            &meta, &policy, "do it", &mut convo, &mut model, &tools, &memory, &ws,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("boom"));
    let events = sink.events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, UiEvent::Error { .. }))
            .count(),
        1
    );
    assert_eq!(done_count(&events), 1);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, UiEvent::Interrupted { .. }))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn agent_loop_stops_after_twelve_rounds() {
    let (dir, memory, tools, meta, policy) = harness(true);
    let sink = collect();
    let loopt = AgentLoop::new(sink.clone(), Arc::new(NoopRunControl));
    let model_calls = Arc::new(Mutex::new(0usize));
    struct CountingInfinite {
        calls: Arc<Mutex<usize>>,
    }
    #[async_trait::async_trait]
    impl AsyncModel for CountingInfinite {
        async fn complete(
            &mut self,
            _messages: &[darius_cognitive::Message],
            _tools: &[ToolSpec],
            _ctx: &TurnContext,
        ) -> Result<ModelOutput, CognitiveError> {
            let mut n = self.calls.lock().unwrap();
            *n += 1;
            Ok(ModelOutput {
                content: None,
                tool_calls: vec![ToolCall {
                    id: format!("c{n}"),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "missing.txt"}),
                }],
            })
        }
    }
    let mut model = CountingInfinite {
        calls: model_calls.clone(),
    };
    let mut convo = Conversation::from_messages(vec![]).unwrap();
    let ws = dir.to_string_lossy().to_string();
    let err = loopt
        .run_turn(
            &meta,
            &policy,
            "loop forever",
            &mut convo,
            &mut model,
            &tools,
            &memory,
            &ws,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("12"), "got: {err}");
    assert_eq!(*model_calls.lock().unwrap(), 12);
    let events = sink.events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, UiEvent::Error { .. }))
            .count(),
        1
    );
    assert_eq!(done_count(&events), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn agent_loop_compacts_oldest_tool_results_first() {
    use darius_cognitive::Message;
    let mut msgs = vec![
        Message::System {
            content: "s".into(),
        },
        Message::User {
            content: "u".into(),
        },
        Message::Tool {
            tool_call_id: "t0".into(),
            name: "read_file".into(),
            content: "A".repeat(1000),
        },
        Message::Tool {
            tool_call_id: "t1".into(),
            name: "read_file".into(),
            content: "B".repeat(1000),
        },
        Message::Tool {
            tool_call_id: "t2".into(),
            name: "read_file".into(),
            content: "C".repeat(1000),
        },
    ];
    compact_tool_results(&mut msgs, 2300);
    assert!(transcript_chars(&msgs) <= 2300);
    assert!(matches!(
        &msgs[2],
        Message::Tool { content, .. } if content.contains("[compacted") && content.len() < 1000
    ));
    assert!(matches!(
        &msgs[3],
        Message::Tool { content, .. } if content.len() == 1000
    ));
    assert!(matches!(
        &msgs[4],
        Message::Tool { content, .. } if content.len() == 1000
    ));
}

#[test]
fn agent_loop_prompt_covers_policy() {
    let prompt = coding_system_prompt("/ws");
    assert!(prompt.contains("/ws"), "{prompt}");
    assert!(prompt.contains("Inspect"), "{prompt}");
    assert!(prompt.contains("test"), "{prompt}");
    assert!(prompt.contains("secret"), "{prompt}");
    assert!(prompt.contains("Plan"), "{prompt}");
}

#[test]
fn agent_loop_specs_match_allowlist() {
    let specs = model_tool_specs();
    let mut names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    let mut expected: Vec<&str> = darius_tools::model_tools::MODEL_TOOLS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    expected.sort_unstable();
    assert_eq!(names, expected);
    assert!(specs.iter().all(|s| !s.description.is_empty()));
}
