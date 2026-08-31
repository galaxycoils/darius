//! CognitiveLoop — plan, execute, react, accept.

pub mod skills;

use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Sender};
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
}

impl From<String> for CognitiveError {
    fn from(s: String) -> Self {
        CognitiveError::Board(s)
    }
}

/// UiEvent bus — sole producer of agent progress events.
/// Consumers: TUI, Web (SSE), A2A task status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UiEvent {
    Header {
        profile: String,
        model: String,
        goal: String,
    },
    AssistantDelta {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolStart {
        id: String,
        name: String,
        args_preview: String,
    },
    ToolEnd {
        id: String,
        ok: bool,
        preview: String,
        spilled: Option<String>,
    },
    TaskBoard(Vec<TaskSnapshot>),
    PermissionRequired {
        id: String,
        reason: String,
    },
    Accept {
        passed: bool,
        notes: String,
    },
    Status {
        line: String,
    },
    A2aTask {
        task_id: String,
        state: String,
    },
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskSnapshot {
    pub id: String,
    pub title: String,
    pub status: String,
}

/// CognitiveLoop — emits UiEvent progress via channel.
pub struct CognitiveLoop {
    event_sender: Sender<UiEvent>,
}

impl CognitiveLoop {
    pub fn new() -> (Self, std::sync::mpsc::Receiver<UiEvent>) {
        let (tx, rx) = mpsc::channel();
        (Self { event_sender: tx }, rx)
    }

    pub fn run(
        &self,
        policy: &LoopPolicy,
        goal: &str,
        mut model: Box<dyn Model>,
        tools: &mut darius_tools::ToolRegistry,
        memory: &darius_memory::MemoryEngine,
    ) -> Result<(Plan, Acceptance), CognitiveError> {
        self.emit(UiEvent::Header {
            profile: "default".into(),
            model: "mock".into(),
            goal: goal.into(),
        });

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
                    self.emit(UiEvent::ToolStart {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        args_preview: format!("{:?}", call.arguments),
                    });

                    let outcome = tools.execute(call);
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
                            let _ = board.add_evidence(task_id, &format!("Error: {message}"));
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
        let _ = self.event_sender.send(event);
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
    policy: &LoopPolicy,
    goal: &str,
    mut model: Box<dyn Model>,
    tools: &mut darius_tools::ToolRegistry,
    memory: &darius_memory::MemoryEngine,
) -> Result<(Plan, Acceptance), CognitiveError> {
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

    // Step 3: Execute tasks with ReAct loop
    let task_ids: Vec<String> = board.list().iter().map(|t| t.id.clone()).collect();
    for task_id in &task_ids {
        let mut iter_count = 0;
        while iter_count < policy.max_react_iters {
            // Check if task is complete
            if board
                .get(task_id)
                .map(|t| t.status == darius_tools::TaskStatus::Completed)
                .unwrap_or(false)
            {
                break;
            }

            // Get memory pack
            let pack = memory.build_pack(policy.memory_max_chars, 12)?;
            let pack_text = if pack.plain.is_empty() {
                String::new()
            } else {
                format!("Memory:\n{}", pack.plain)
            };

            // Get task title
            let task_title = board
                .get(task_id)
                .map(|t| t.title.clone())
                .unwrap_or_default();

            // Ask model what to do
            let response = model.react(&format!("Task: {}\n{}", task_title, pack_text))?;

            // Parse tool calls
            let tool_calls = darius_tools::extract_tool_calls(&response);
            if tool_calls.is_empty() {
                // No tools used, check for final answer
                if response.contains("DONE") || response.contains("COMPLETE") {
                    board.complete(task_id)?;
                }
                break;
            }

            // Execute tools
            for call in &tool_calls {
                let outcome = tools.execute(call);
                match &outcome {
                    darius_tools::ToolOutcome::Ok { preview, .. } => {
                        // Store evidence
                        let _ = board.add_evidence(task_id, preview);
                    }
                    darius_tools::ToolOutcome::Err { message } => {
                        let _ = board.add_evidence(task_id, &format!("Error: {message}"));
                    }
                }
            }

            iter_count += 1;
        }
    }

    // Step 4: Accept
    let acceptance = if policy.require_acceptance {
        // Check if all tasks completed
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

    Ok((plan, acceptance))
}

/// Trait for models that can be used in the cognitive loop.
pub trait Model {
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

        let policy = LoopPolicy::default();
        let goal = "test goal";

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working on task"}}"#.to_string(),
            "DONE".to_string(),
        ];

        let model = Box::new(MockModel::new(plan_response, react_responses));

        let (plan, acceptance) = run_loop(&policy, goal, model, &mut tools, &memory).unwrap();

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

        let policy = LoopPolicy::default();
        let goal = "test goal";

        let plan_response = r#"{"tasks":[]}"#.to_string();
        let react_responses = vec![];

        let model = Box::new(MockModel::new(plan_response, react_responses));

        let result = run_loop(&policy, goal, model, &mut tools, &memory);
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

        let policy = LoopPolicy::default();
        let goal = "test goal";

        // Create a plan with more tasks than max_tasks
        let mut tasks = Vec::new();
        for i in 0..20 {
            tasks.push(format!(r#"{{"title":"task {i}"}}"#));
        }
        let plan_response = format!(r#"{{"tasks":[{}]}}"#, tasks.join(","));
        let react_responses = vec![];

        let model = Box::new(MockModel::new(plan_response, react_responses));

        let result = run_loop(&policy, goal, model, &mut tools, &memory);
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

        let policy = LoopPolicy::default();
        let goal = "test goal";

        let plan_response = r#"{"tasks":[{"title":"task 1"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let model = Box::new(MockModel::new(plan_response, react_responses));

        let (loop_instance, rx) = CognitiveLoop::new();
        let (plan, acceptance) = loop_instance
            .run(&policy, goal, model, &mut tools, &memory)
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
}
