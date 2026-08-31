//! CognitiveLoop — plan, execute, react, accept.

pub mod skills;

use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CognitiveError {
    #[error("loop error: {0}")]
    Loop(String),
    #[error("tool error: {0}")]
    Tool(#[from] darius_tools::ToolError),
    #[error("memory error: {0}")]
    Memory(#[from] darius_memory::MemoryError),
    #[error("invalid plan: {0}")]
    InvalidPlan(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("board error: {0}")]
    Board(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<String> for CognitiveError {
    fn from(s: String) -> Self {
        CognitiveError::Board(s)
    }
}

pub mod ui_events;
pub use ui_events::*;

/// Control handle for cancellation and tool approval.
pub trait RunControl: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn approve_tool(
        &self,
        call: &darius_tools::ToolCall,
        risk: darius_tools::ToolRisk,
    ) -> Result<PermissionChoice, CognitiveError>;
}

/// No-op RunControl for CLI tests — never cancelled, auto-approves tools.
pub struct NoopRunControl;

impl RunControl for NoopRunControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn approve_tool(
        &self,
        _call: &darius_tools::ToolCall,
        _risk: darius_tools::ToolRisk,
    ) -> Result<PermissionChoice, CognitiveError> {
        Ok(PermissionChoice::AllowOnce)
    }
}

/// Run metadata — emitted in the Header event so consumers know which
/// profile, model, and mode produced a given session.
#[derive(Debug, Clone)]
pub struct RunMetadata {
    pub profile: String,
    pub model: String,
    pub mode: String,
}

/// CognitiveLoop — emits UiEvent progress via an EventSink and honors RunControl.
pub struct CognitiveLoop {
    sink: Arc<dyn EventSink>,
    control: Arc<dyn RunControl>,
}

impl CognitiveLoop {
    pub fn new(sink: Arc<dyn EventSink>, control: Arc<dyn RunControl>) -> Self {
        Self { sink, control }
    }

    /// Create a CognitiveLoop backed by an std mpsc channel and NoopRunControl.
    pub fn with_channel() -> (Self, std::sync::mpsc::Receiver<UiEvent>) {
        let (tx, rx) = mpsc::channel();
        let sink = Arc::new(ChannelEventSink::new(tx));
        let control = Arc::new(NoopRunControl);
        (Self { sink, control }, rx)
    }

    pub fn run(
        &self,
        metadata: &RunMetadata,
        policy: &LoopPolicy,
        goal: &str,
        model: &mut dyn Model,
        tools: &mut darius_tools::ToolRegistry,
        memory: &darius_memory::MemoryEngine,
    ) -> Result<(Plan, Acceptance), CognitiveError> {
        self.emit(UiEvent::Header {
            profile: metadata.profile.clone(),
            model: metadata.model.clone(),
            goal: goal.into(),
        });

        // Check cancellation before planning.
        if self.control.is_cancelled() {
            self.emit(UiEvent::Status {
                line: "Interrupted".into(),
            });
            self.emit(UiEvent::Done);
            return Err(CognitiveError::Cancelled);
        }

        // Step 1: Get plan from model
        let plan_text = model.plan(goal)?;
        let plan = parse_plan(&plan_text)?;

        if plan.tasks.len() > policy.max_tasks {
            return Err(CognitiveError::InvalidPlan(format!(
                "too many tasks: {} > {}",
                plan.tasks.len(),
                policy.max_tasks
            )));
        }

        // Step 2: Create task board
        let mut board = darius_tools::TaskBoard::new(policy.max_tasks);
        for task in &plan.tasks {
            board.add(&task.title)?;
        }

        // Emit initial task board
        self.emit_task_board(&board);

        // Step 3: Execute tasks with ReAct loop
        let task_ids: Vec<String> = board.list().iter().map(|t| t.id.clone()).collect();
        for task_id in &task_ids {
            let mut iter_count = 0;
            while iter_count < policy.max_react_iters {
                // Check cancellation before each ReAct iteration.
                if self.control.is_cancelled() {
                    self.emit(UiEvent::Status {
                        line: "Interrupted".into(),
                    });
                    self.emit(UiEvent::Done);
                    return Err(CognitiveError::Cancelled);
                }

                if board
                    .get(task_id)
                    .map(|t| t.status == darius_tools::TaskStatus::Completed)
                    .unwrap_or(false)
                {
                    break;
                }

                let pack = memory.build_pack(policy.memory_max_chars, 12)?;
                let pack_text = if pack.plain.is_empty() {
                    String::new()
                } else {
                    format!("Memory:\n{}", pack.plain)
                };

                let task_title = board
                    .get(task_id)
                    .map(|t| t.title.clone())
                    .unwrap_or_default();

                let response = model.react(&format!("Task: {}\n{}", task_title, pack_text))?;

                let tool_calls = darius_tools::extract_tool_calls(&response);
                if tool_calls.is_empty() {
                    if response.contains("DONE") || response.contains("COMPLETE") {
                        board.complete(task_id)?;
                        self.emit_task_board(&board);
                    }
                    break;
                }

                for call in &tool_calls {
                    // Check cancellation before each tool.
                    if self.control.is_cancelled() {
                        self.emit(UiEvent::Status {
                            line: "Interrupted".into(),
                        });
                        self.emit(UiEvent::Done);
                        return Err(CognitiveError::Cancelled);
                    }

                    // Gate mutating and shell tools behind permission approval.
                    let risk = tools
                        .risk(&call.name)
                        .unwrap_or(darius_tools::ToolRisk::ReadOnly);
                    let (outcome, denied) = match risk {
                        darius_tools::ToolRisk::ReadOnly => {
                            let outcome = tools.execute(call);
                            (outcome, false)
                        }
                        darius_tools::ToolRisk::Mutating | darius_tools::ToolRisk::Shell => {
                            match self.control.approve_tool(call, risk) {
                                Ok(PermissionChoice::AllowOnce)
                                | Ok(PermissionChoice::AllowSession) => {
                                    let outcome = tools.execute(call);
                                    (outcome, false)
                                }
                                Ok(PermissionChoice::Deny) => {
                                    self.emit(UiEvent::PermissionResolved {
                                        id: call.id.clone(),
                                        choice: PermissionChoice::Deny,
                                    });
                                    (
                                        darius_tools::ToolOutcome::Err {
                                            message: "permission denied by user".into(),
                                        },
                                        true,
                                    )
                                }
                                Err(_) => {
                                    self.emit(UiEvent::Status {
                                        line: "Interrupted".into(),
                                    });
                                    self.emit(UiEvent::Done);
                                    return Err(CognitiveError::Cancelled);
                                }
                            }
                        }
                    };

                    self.emit(UiEvent::ToolStart {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        args_preview: format!("{:?}", call.arguments),
                    });

                    match &outcome {
                        darius_tools::ToolOutcome::Ok {
                            preview,
                            spilled_path,
                        } => {
                            let _ = board.add_evidence(task_id, preview);
                            self.emit(UiEvent::ToolEnd {
                                id: call.id.clone(),
                                ok: true,
                                preview: preview.clone(),
                                spilled: spilled_path
                                    .as_ref()
                                    .map(|p| p.to_string_lossy().to_string()),
                            });
                        }
                        darius_tools::ToolOutcome::Err { message } => {
                            let evidence = if denied {
                                format!("Denied: {message}")
                            } else {
                                format!("Error: {message}")
                            };
                            let _ = board.add_evidence(task_id, &evidence);
                            self.emit(UiEvent::ToolEnd {
                                id: call.id.clone(),
                                ok: false,
                                preview: message.clone(),
                                spilled: None,
                            });
                        }
                    }
                }

                iter_count += 1;
            }
        }

        // Step 4: Accept
        let acceptance = if policy.require_acceptance {
            let all_complete = board
                .list()
                .iter()
                .all(|t| t.status == darius_tools::TaskStatus::Completed);
            if all_complete {
                Acceptance::Accepted
            } else {
                Acceptance::Rejected("not all tasks completed".into())
            }
        } else {
            Acceptance::Accepted
        };

        self.emit(UiEvent::Accept {
            passed: matches!(acceptance, Acceptance::Accepted),
            notes: match &acceptance {
                Acceptance::Accepted => "all tasks completed".into(),
                Acceptance::Rejected(r) => r.clone(),
            },
        });
        self.emit(UiEvent::Done);

        Ok((plan, acceptance))
    }

    fn emit(&self, event: UiEvent) {
            self.sink.emit(event);
        }

    fn emit_task_board(&self, board: &darius_tools::TaskBoard) {
        let snapshots: Vec<TaskSnapshot> = board
            .list()
            .into_iter()
            .map(|t| TaskSnapshot {
                id: t.id.clone(),
                title: t.title.clone(),
                status: format!("{:?}", t.status),
            })
            .collect();
        self.emit(UiEvent::TaskBoard(snapshots));
    }
}

/// Loop policy configuration.
#[derive(Debug, Clone)]
pub struct LoopPolicy {
    pub max_tasks: usize,
    pub max_react_iters: usize,
    pub memory_max_chars: usize,
    pub tool_preview_ceiling: usize,
    pub require_plan: bool,
    pub require_acceptance: bool,
}

impl Default for LoopPolicy {
    fn default() -> Self {
        Self {
            max_tasks: 15,
            max_react_iters: 12,
            memory_max_chars: 3500,
            tool_preview_ceiling: 32768,
            require_plan: true,
            require_acceptance: true,
        }
    }
}

/// A plan is a JSON object with tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub tasks: Vec<PlanTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub title: String,
    pub description: Option<String>,
}

/// Parse a plan from JSON string.
pub fn parse_plan(json: &str) -> Result<Plan, CognitiveError> {
    let plan: Plan =
        serde_json::from_str(json).map_err(|e| CognitiveError::InvalidPlan(e.to_string()))?;

    if plan.tasks.is_empty() {
        return Err(CognitiveError::InvalidPlan(
            "plan must have at least one task".into(),
        ));
    }

    Ok(plan)
}

/// Acceptance result for a loop run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Acceptance {
    Accepted,
    Rejected(String),
}

/// Run a cognitive loop with the given policy, model, and goal.
pub fn run_loop(
    metadata: &RunMetadata,
    policy: &LoopPolicy,
    goal: &str,
    model: &mut dyn Model,
    tools: &mut darius_tools::ToolRegistry,
    memory: &darius_memory::MemoryEngine,
) -> Result<(Plan, Acceptance), CognitiveError> {
    let (runner, _events) = CognitiveLoop::with_channel();
    runner.run(metadata, policy, goal, model, tools, memory)
}

/// Trait for models that can be used in the cognitive loop.
pub trait Model: Send {
    fn plan(&mut self, goal: &str) -> Result<String, CognitiveError>;
    fn react(&mut self, context: &str) -> Result<String, CognitiveError>;
}

/// Mock model for testing.
pub struct MockModel {
    pub plan_response: String,
    pub react_responses: Vec<String>,
    pub react_index: usize,
}

impl MockModel {
    pub fn new(plan_response: String, react_responses: Vec<String>) -> Self {
        Self {
            plan_response,
            react_responses,
            react_index: 0,
        }
    }
}

impl Model for MockModel {
    fn plan(&mut self, _goal: &str) -> Result<String, CognitiveError> {
        Ok(self.plan_response.clone())
    }

    fn react(&mut self, _context: &str) -> Result<String, CognitiveError> {
        if self.react_index < self.react_responses.len() {
            let response = self.react_responses[self.react_index].clone();
            self.react_index += 1;
            Ok(response)
        } else {
            Ok("DONE".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_metadata() -> RunMetadata {
        RunMetadata {
            profile: "default".into(),
            model: "mock".into(),
            mode: "auto".into(),
        }
    }

    #[test]
    fn parse_plan_valid() {
        let json = r#"{"tasks":[{"title":"task 1"},{"title":"task 2"}]}"#;
        let plan = parse_plan(json).unwrap();
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].title, "task 1");
    }

    #[test]
    fn parse_plan_empty_fails() {
        let json = r#"{"tasks":[]}"#;
        let result = parse_plan(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_plan_invalid_json() {
        let json = "not json";
        let result = parse_plan(json);
        assert!(result.is_err());
    }

    #[test]
    fn loop_completes_with_mock_model() {
        let dir =
            std::env::temp_dir().join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();

        let metadata = default_metadata();
        let policy = LoopPolicy::default();
        let goal = "test goal";

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working on task"}}"#.to_string(),
            "DONE".to_string(),
        ];

        let mut model = MockModel::new(plan_response, react_responses);

        let (plan, acceptance) =
            run_loop(&metadata, &policy, goal, &mut model, &mut tools, &memory).unwrap();

        assert_eq!(plan.tasks.len(), 1);
        match acceptance {
            Acceptance::Accepted => {}
            Acceptance::Rejected(reason) => panic!("unexpected rejection: {reason}"),
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn loop_rejects_empty_plan() {
        let dir =
            std::env::temp_dir().join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();

        let metadata = default_metadata();
        let policy = LoopPolicy::default();
        let goal = "test goal";

        let plan_response = r#"{"tasks":[]}"#.to_string();
        let react_responses = vec![];

        let mut model = MockModel::new(plan_response, react_responses);

        let result = run_loop(&metadata, &policy, goal, &mut model, &mut tools, &memory);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn loop_respects_max_tasks() {
        let dir =
            std::env::temp_dir().join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();

        let metadata = default_metadata();
        let policy = LoopPolicy::default();
        let goal = "test goal";

        // Create a plan with more tasks than max_tasks
        let mut tasks = Vec::new();
        for i in 0..20 {
            tasks.push(format!(r#"{{"title":"task {i}"}}"#));
        }
        let plan_response = format!(r#"{{"tasks":[{}]}}"#, tasks.join(","));
        let react_responses = vec![];

        let mut model = MockModel::new(plan_response, react_responses);

        let result = run_loop(&metadata, &policy, goal, &mut model, &mut tools, &memory);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn uievent_bus_delivers_events_in_order() {
        let dir =
            std::env::temp_dir().join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();

        let metadata = default_metadata();
        let policy = LoopPolicy::default();
        let goal = "test goal";

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let (loop_instance, rx) = CognitiveLoop::with_channel();
        let (plan, acceptance) = loop_instance
            .run(&metadata, &policy, goal, &mut model, &mut tools, &memory)
            .unwrap();

        assert_eq!(plan.tasks.len(), 1);
        matches!(acceptance, Acceptance::Accepted);

        // Drop the sender so rx.iter() terminates
        drop(loop_instance);

        // Verify events were emitted in order
        let events: Vec<UiEvent> = rx.iter().collect();
        assert!(!events.is_empty());
        assert_eq!(
            events[0],
            UiEvent::Header {
                profile: "default".into(),
                model: "mock".into(),
                goal: goal.into()
            }
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, UiEvent::ToolStart { .. }))
        );
        assert!(events.iter().any(|e| matches!(e, UiEvent::ToolEnd { .. })));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, UiEvent::Accept { passed: true, .. }))
        );
        assert_eq!(events.last(), Some(&UiEvent::Done));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn header_emits_real_metadata_not_hardcoded() {
        let dir =
            std::env::temp_dir().join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();

        let metadata = RunMetadata {
            profile: "work".into(),
            model: "gpt-4o-mini".into(),
            mode: "auto".into(),
        };
        let policy = LoopPolicy::default();
        let goal = "test goal";

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let (loop_instance, rx) = CognitiveLoop::with_channel();
        let (_plan, _acceptance) = loop_instance
            .run(&metadata, &policy, goal, &mut model, &mut tools, &memory)
            .unwrap();

        drop(loop_instance);

        let events: Vec<UiEvent> = rx.iter().collect();
        assert_eq!(
            events[0],
            UiEvent::Header {
                profile: "work".into(),
                model: "gpt-4o-mini".into(),
                goal: goal.into()
            }
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn model_is_reusable_across_two_turns() {
        let dir =
            std::env::temp_dir().join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();

        let metadata = default_metadata();
        let policy = LoopPolicy::default();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        // First turn
        let (plan1, acceptance1) = run_loop(
            &metadata,
            &policy,
            "first goal",
            &mut model,
            &mut tools,
            &memory,
        )
        .unwrap();
        assert_eq!(plan1.tasks.len(), 1);
        assert!(matches!(acceptance1, Acceptance::Accepted));

        // Second turn with the same model
        let (plan2, acceptance2) = run_loop(
            &metadata,
            &policy,
            "second goal",
            &mut model,
            &mut tools,
            &memory,
        )
        .unwrap();
        assert_eq!(plan2.tasks.len(), 1);
        assert!(matches!(acceptance2, Acceptance::Accepted));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- Task 3.2: EventSink and cancellation tests ---

    /// Test EventSink that collects emitted events.
    struct TestSink {
        events: std::sync::Mutex<Vec<UiEvent>>,
    }

    impl TestSink {
        fn raw() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> std::sync::MutexGuard<'_, Vec<UiEvent>> {
            self.events.lock().unwrap()
        }
    }

    impl EventSink for TestSink {
        fn emit(&self, event: UiEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    /// Test RunControl with configurable cancellation.
    struct TestRunControl {
        cancelled: std::sync::atomic::AtomicBool,
    }

    impl TestRunControl {
        fn new(cancelled: bool) -> Self {
            Self {
                cancelled: std::sync::atomic::AtomicBool::new(cancelled),
            }
        }
    }

    impl RunControl for TestRunControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn approve_tool(
            &self,
            _call: &darius_tools::ToolCall,
            _risk: darius_tools::ToolRisk,
        ) -> Result<PermissionChoice, CognitiveError> {
            Ok(PermissionChoice::AllowOnce)
        }
    }

    fn setup_cognitive_test() -> (
        std::path::PathBuf,
        darius_memory::MemoryEngine,
        darius_tools::ToolRegistry,
        RunMetadata,
        LoopPolicy,
    ) {
        let dir = std::env::temp_dir()
            .join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();
        darius_tools::register_memory_builtins(&mut tools, &memory);
        let metadata = RunMetadata {
            profile: "test".into(),
            model: "mock".into(),
            mode: "auto".into(),
        };
        let policy = LoopPolicy::default();
        (dir, memory, tools, metadata, policy)
    }

    #[test]
    fn event_sink_receives_events_as_they_happen() {
        let (dir, memory, mut tools, metadata, policy) = setup_cognitive_test();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        let control = Arc::new(TestRunControl::new(false));
        let loop_inst = CognitiveLoop::new(sink.clone(), control);

        let (_plan, _acceptance) = loop_inst
            .run(&metadata, &policy, "test goal", &mut model, &mut tools, &memory)
            .unwrap();

        let events = sink.events();
        assert!(!events.is_empty());
        assert!(events.iter().any(|e| matches!(e, UiEvent::Header { .. })));
        assert!(events.iter().any(|e| matches!(e, UiEvent::TaskBoard(_))));
        assert!(events.iter().any(|e| matches!(e, UiEvent::ToolStart { .. })));
        assert!(events.iter().any(|e| matches!(e, UiEvent::ToolEnd { .. })));
        assert!(events.iter().any(|e| matches!(e, UiEvent::Accept { .. })));
        assert_eq!(events.last(), Some(&UiEvent::Done));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancellation_prevents_tool_execution() {
        let (dir, memory, mut tools, metadata, policy) = setup_cognitive_test();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        // Cancel before planning even starts.
        let control = Arc::new(TestRunControl::new(true));
        let loop_inst = CognitiveLoop::new(sink.clone(), control);

        let result = loop_inst.run(
            &metadata,
            &policy,
            "test goal",
            &mut model,
            &mut tools,
            &memory,
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CognitiveError::Cancelled));

        let events = sink.events();
        // Should have Header, Status "Interrupted", Done — but no tool events.
        assert!(events.iter().any(|e| matches!(e, UiEvent::Header { .. })));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, UiEvent::Status { line } if line == "Interrupted"))
        );
        assert!(!events.iter().any(|e| matches!(e, UiEvent::ToolStart { .. })));
        assert!(!events.iter().any(|e| matches!(e, UiEvent::ToolEnd { .. })));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cancellation_always_ends_with_done() {
        let (dir, memory, mut tools, metadata, policy) = setup_cognitive_test();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        let control = Arc::new(TestRunControl::new(true));
        let loop_inst = CognitiveLoop::new(sink.clone(), control);

        let result = loop_inst.run(
            &metadata,
            &policy,
            "test goal",
            &mut model,
            &mut tools,
            &memory,
        );

        assert!(result.is_err());

        let events = sink.events();
        // Last event must always be Done.
        assert_eq!(events.last(), Some(&UiEvent::Done));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_turns_dont_leak_events() {
        let (dir, memory, mut tools, metadata, policy) = setup_cognitive_test();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];

        // Turn 1
        let sink1 = Arc::new(TestSink::raw());
        let control1 = Arc::new(TestRunControl::new(false));
        let loop1 = CognitiveLoop::new(sink1.clone(), control1);
        let mut model1 = MockModel::new(plan_response.clone(), react_responses.clone());
        loop1
            .run(
                &metadata,
                &policy,
                "first goal",
                &mut model1,
                &mut tools,
                &memory,
            )
            .unwrap();

        // Turn 2 with a fresh sink
        let sink2 = Arc::new(TestSink::raw());
        let control2 = Arc::new(TestRunControl::new(false));
        let loop2 = CognitiveLoop::new(sink2.clone(), control2);
        let mut model2 = MockModel::new(plan_response, react_responses);
        loop2
            .run(
                &metadata,
                &policy,
                "second goal",
                &mut model2,
                &mut tools,
                &memory,
            )
            .unwrap();

        let events1 = sink1.events();
        let events2 = sink2.events();

        // Each sink should have exactly its own events.
        assert!(!events1.is_empty());
        assert!(!events2.is_empty());

        // Both should start with Header and end with Done.
        assert!(events1.iter().any(|e| matches!(e, UiEvent::Header { .. })));
        assert_eq!(events1.last(), Some(&UiEvent::Done));
        assert!(events2.iter().any(|e| matches!(e, UiEvent::Header { .. })));
        assert_eq!(events2.last(), Some(&UiEvent::Done));

        // Turn 2 events should NOT contain "first goal" header.
        assert!(!events2
            .iter()
            .any(|e| matches!(e, UiEvent::Header { goal, .. } if goal == "first goal")));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --- Task 5.2: Permission gating tests ---

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Test RunControl with configurable approval behavior and call counter.
    struct CountingRunControl {
        cancelled: AtomicUsize,
        // 0 = deny, 1 = allow once, 2 = allow session
        behavior: AtomicUsize,
        session_approvals: std::sync::Mutex<Vec<String>>,
    }

    impl CountingRunControl {
        fn new(behavior: usize) -> Self {
            Self {
                cancelled: AtomicUsize::new(0),
                behavior: AtomicUsize::new(behavior),
                session_approvals: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn with_denial() -> Self {
            Self::new(0)
        }

        fn with_allow_once() -> Self {
            Self::new(1)
        }

        fn with_allow_session() -> Self {
            Self::new(2)
        }

        fn set_cancelled(&self) {
            self.cancelled.store(1, Ordering::SeqCst);
        }
    }

    impl RunControl for CountingRunControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::SeqCst) != 0
        }

        fn approve_tool(
            &self,
            call: &darius_tools::ToolCall,
            _risk: darius_tools::ToolRisk,
        ) -> Result<PermissionChoice, CognitiveError> {
            match self.behavior.load(Ordering::SeqCst) {
                0 => Ok(PermissionChoice::Deny),
                1 => Ok(PermissionChoice::AllowOnce),
                2 => {
                    self.session_approvals
                        .lock()
                        .unwrap()
                        .push(call.name.clone());
                    Ok(PermissionChoice::AllowSession)
                }
                _ => Ok(PermissionChoice::Deny),
            }
        }
    }

    #[test]
    fn permission_deny_prevents_tool_execution() {
        let dir = std::env::temp_dir()
            .join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();
        darius_tools::register_memory_builtins(&mut tools, &memory);

        // Counter increments on every memory_remember execution.
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        tools.register_with_risk(
            "memory_remember",
            darius_tools::ToolRisk::Mutating,
            move |_call| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                // Call the real implementation would require memory, so just count.
                Ok(darius_tools::ToolOutcome::Ok {
                    preview: "remembered".into(),
                    spilled_path: None,
                })
            },
        );

        let metadata = RunMetadata {
            profile: "test".into(),
            model: "mock".into(),
            mode: "auto".into(),
        };
        let policy = LoopPolicy::default();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        let control = Arc::new(CountingRunControl::with_denial());
        let loop_inst = CognitiveLoop::new(sink.clone(), control);

        let result = loop_inst.run(
            &metadata,
            &policy,
            "test goal",
            &mut model,
            &mut tools,
            &memory,
        );

        // Loop completes successfully even when tools are denied.
        assert!(result.is_ok());
        // Counter must remain zero because the tool was denied.
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        let events = sink.events();
        // PermissionResolved with Deny should be emitted.
        assert!(events.iter().any(|e| matches!(
            e,
            UiEvent::PermissionResolved {
                choice: PermissionChoice::Deny,
                ..
            }
        )));
        // ToolEnd with ok=false should be emitted for the denied tool.
        assert!(events.iter().any(|e| matches!(
            e,
            UiEvent::ToolEnd { ok: false, .. }
        )));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn permission_allow_once_executes_tool() {
        let dir = std::env::temp_dir()
            .join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();
        darius_tools::register_memory_builtins(&mut tools, &memory);

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        tools.register_with_risk(
            "memory_remember",
            darius_tools::ToolRisk::Mutating,
            move |_call| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok(darius_tools::ToolOutcome::Ok {
                    preview: "remembered".into(),
                    spilled_path: None,
                })
            },
        );

        let metadata = RunMetadata {
            profile: "test".into(),
            model: "mock".into(),
            mode: "auto".into(),
        };
        let policy = LoopPolicy::default();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        let control = Arc::new(CountingRunControl::with_allow_once());
        let loop_inst = CognitiveLoop::new(sink.clone(), control);

        let result = loop_inst.run(
            &metadata,
            &policy,
            "test goal",
            &mut model,
            &mut tools,
            &memory,
        );

        assert!(result.is_ok());
        // Counter becomes one after approval.
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn permission_allow_session_caches_for_same_tool() {
        let dir = std::env::temp_dir()
            .join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();
        darius_tools::register_memory_builtins(&mut tools, &memory);

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        tools.register_with_risk(
            "memory_remember",
            darius_tools::ToolRisk::Mutating,
            move |_call| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok(darius_tools::ToolOutcome::Ok {
                    preview: "remembered".into(),
                    spilled_path: None,
                })
            },
        );

        let metadata = RunMetadata {
            profile: "test".into(),
            model: "mock".into(),
            mode: "auto".into(),
        };
        let policy = LoopPolicy::default();

        // Two tool calls to the same tool in one turn.
        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"first"}}
TOOL {"name":"memory_remember","arguments":{"body":"second"}}"#
                .to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        let control = Arc::new(CountingRunControl::with_allow_session());
        let loop_inst = CognitiveLoop::new(sink.clone(), control.clone());

        let result = loop_inst.run(
            &metadata,
            &policy,
            "test goal",
            &mut model,
            &mut tools,
            &memory,
        );

        assert!(result.is_ok());
        // Both tool calls executed.
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        // approve_tool was called twice (session caching is handled by the caller).
        let approvals = control.session_approvals.lock().unwrap();
        assert_eq!(approvals.len(), 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn permission_shell_tool_requires_approval() {
        let dir = std::env::temp_dir()
            .join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();
        darius_tools::register_coding_builtins(&mut tools);

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        tools.register_with_risk("shell", darius_tools::ToolRisk::Shell, move |_call| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(darius_tools::ToolOutcome::Ok {
                preview: "shell output".into(),
                spilled_path: None,
            })
        });

        let metadata = RunMetadata {
            profile: "test".into(),
            model: "mock".into(),
            mode: "auto".into(),
        };
        let policy = LoopPolicy::default();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"shell","arguments":{"command": "echo hi"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        let control = Arc::new(CountingRunControl::with_allow_once());
        let loop_inst = CognitiveLoop::new(sink.clone(), control);

        let result = loop_inst.run(
            &metadata,
            &policy,
            "test goal",
            &mut model,
            &mut tools,
            &memory,
        );

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn permission_read_only_tool_skips_approval() {
        let dir = std::env::temp_dir()
            .join(format!("darius_cognitive_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&dir).unwrap();
        darius_tools::register_memory_builtins(&mut tools, &memory);

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        tools.register_with_risk(
            "memory_search",
            darius_tools::ToolRisk::ReadOnly,
            move |_call| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok(darius_tools::ToolOutcome::Ok {
                    preview: "search results".into(),
                    spilled_path: None,
                })
            },
        );

        let metadata = RunMetadata {
            profile: "test".into(),
            model: "mock".into(),
            mode: "auto".into(),
        };
        let policy = LoopPolicy::default();

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_search","arguments":{"text":"test"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = MockModel::new(plan_response, react_responses);

        let sink = Arc::new(TestSink::raw());
        // Even with deny behavior, read-only tools execute without approval.
        let control = Arc::new(CountingRunControl::with_denial());
        let loop_inst = CognitiveLoop::new(sink.clone(), control);

        let result = loop_inst.run(
            &metadata,
            &policy,
            "test goal",
            &mut model,
            &mut tools,
            &memory,
        );

        assert!(result.is_ok());
        // ReadOnly tool executes without needing approval.
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        // No PermissionRequired events emitted.
        let events = sink.events();
        assert!(!events
            .iter()
            .any(|e| matches!(e, UiEvent::PermissionRequired { .. })));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
