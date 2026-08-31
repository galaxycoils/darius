#![allow(dead_code, unused_imports)]
//! E2E integration harness — MockLlm, TestDaemon, full-session pipeline tests.

use darius_daemon::Daemon;
use darius_rlm::{IsolationTier, RlmKernel, RlmOptions, RlmStatus, rlm, rlm_evaluate};

/// Mock LLM for testing.
pub struct MockLlm {
    responses: Vec<String>,
    next: usize,
}

impl MockLlm {
    pub fn new(responses: Vec<String>) -> Self {
        Self { responses, next: 0 }
    }

    pub fn next_response(&mut self) -> Option<String> {
        if self.next < self.responses.len() {
            let r = self.responses[self.next].clone();
            self.next += 1;
            Some(r)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.next = 0
    }
}

/// Test daemon for E2E testing.
pub struct TestDaemon {
    running: bool,
    profile: String,
}

impl TestDaemon {
    pub fn new(profile: impl Into<String>) -> Self {
        Self {
            running: false,
            profile: profile.into(),
        }
    }

    pub fn start(&mut self) {
        self.running = true;
    }
    pub fn stop(&mut self) {
        self.running = false;
    }
    pub fn is_running(&self) -> bool {
        self.running
    }
}

/// E2E report.
#[derive(Debug, Clone, Default)]
pub struct E2EReport {
    pub passed: bool,
    pub steps: usize,
    pub errors: Vec<String>,
}

/// E2E error.
#[derive(Debug, thiserror::Error)]
pub enum E2EError {
    #[error("e2e setup failed: {0}")]
    Setup(String),
    #[error("e2e step failed: {0}")]
    Step(String),
}

/// Run a full E2E session pipeline test.
pub fn run_e2e() -> Result<E2EReport, E2EError> {
    let mut report = E2EReport::default();

    // Step 1: Create a profile.
    let profile_dir = std::env::temp_dir().join(format!("darius_e2e_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&profile_dir).map_err(|e| E2EError::Setup(e.to_string()))?;
    report.steps += 1;

    // Step 2: Start the daemon.
    let mut daemon = Daemon::new(&profile_dir);
    daemon.start().map_err(|e| E2EError::Step(e.to_string()))?;
    report.steps += 1;

    // Step 3: Create a session.
    let session = daemon
        .create_session("default", "test goal")
        .map_err(|e| E2EError::Step(e.to_string()))?;
    report.steps += 1;

    // Step 4: Attach to the session.
    daemon
        .attach_session(&session.id)
        .map_err(|e| E2EError::Step(e.to_string()))?;
    report.steps += 1;

    // Step 5: Verify session is active.
    let s = daemon
        .get_session(&session.id)
        .map_err(|e| E2EError::Step(e.to_string()))?;
    assert!(s.running);
    report.steps += 1;

    // Step 6: End the session (emits handoff).
    daemon
        .end_session(&session.id)
        .map_err(|e| E2EError::Step(e.to_string()))?;
    report.steps += 1;

    // Step 7: Verify handoff was emitted.
    let store = daemon.handoff_store();
    let store = store.lock();
    let store = store.as_ref().unwrap();
    let handoff = store
        .load(&session.id)
        .map_err(|e| E2EError::Step(e.to_string()))?;
    assert_eq!(handoff.goal, "test goal");
    report.steps += 1;

    // Cleanup.
    let _ = std::fs::remove_dir_all(&profile_dir);

    report.passed = true;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_llm_round_trip() {
        let mut llm = MockLlm::new(vec!["hello".into(), "world".into()]);
        assert_eq!(llm.next_response(), Some("hello".into()));
        assert_eq!(llm.next_response(), Some("world".into()));
        assert_eq!(llm.next_response(), None);
    }

    #[test]
    fn test_daemon_start_stop() {
        let mut daemon = TestDaemon::new("test-profile");
        assert!(!daemon.is_running());
        daemon.start();
        assert!(daemon.is_running());
        daemon.stop();
        assert!(!daemon.is_running());
    }

    #[test]
    fn e2e_full_session_pipeline() {
        let report = run_e2e().expect("e2e should pass");
        assert!(report.passed);
        assert!(report.steps >= 7);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn rlm_kernel_lifecycle() {
        let kernel = RlmKernel::new("k1", IsolationTier::Trusted);
        assert_eq!(kernel.status(), RlmStatus::Idle);
        kernel.start().unwrap();
        assert_eq!(kernel.status(), RlmStatus::Running);
        kernel.stop().unwrap();
        assert_eq!(kernel.status(), RlmStatus::Idle);
    }

    #[test]
    fn rlm_returns_handle() {
        let handle = rlm("ping", RlmOptions::default()).unwrap();
        assert_eq!(handle.status(), RlmStatus::Running);
    }

    #[test]
    fn rlm_evaluate_returns_grade() {
        let grade = rlm_evaluate("target", "rubric").unwrap();
        assert!(grade.passed);
    }

    #[test]
    fn cognitive_integration_e2e_on_temp_profile() {
        let profile_dir =
            std::env::temp_dir().join(format!("darius_cognitive_e2e_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&profile_dir).unwrap();

        // Step 1: Memory upsert + pack
        let memory = darius_memory::MemoryEngine::open(&profile_dir).unwrap();
        memory
            .upsert(darius_memory::NewRecord {
                kind: darius_memory::RecordKind::Fact,
                title: Some("test fact".into()),
                body: "the capital of France is Paris".into(),
                tags: vec!["geography".into()],
                importance: 0.8,
                source: None,
            })
            .unwrap();

        let pack = memory.build_pack(3500, 12).unwrap();
        assert!(pack.plain.contains("Paris"));
        assert_eq!(pack.record_ids.len(), 1);

        // Step 2: Tool registry + memory_search
        let mut tools = darius_tools::ToolRegistry::new(&profile_dir).unwrap();
        darius_tools::register_memory_builtins(&mut tools, &memory);

        let search_call = darius_tools::ToolCall {
            id: "s1".into(),
            name: "memory_search".into(),
            arguments: serde_json::json!({"text": "France"}),
        };
        let outcome = tools.execute(&search_call);
        match outcome {
            darius_tools::ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("Paris"));
            }
            darius_tools::ToolOutcome::Err { message } => panic!("search failed: {message}"),
        }

        // Step 3: CognitiveLoop with MockModel
        let policy = darius_cognitive::LoopPolicy::default();
        let plan_response = r#"{"tasks":[{"title":"answer geography question"}]}"#.to_string();
        let react_responses = vec![
            r#"TOOL {"name":"memory_search","arguments":{"text":"France"}}"#.to_string(),
            "DONE".to_string(),
        ];
        let mut model = darius_cognitive::MockModel::new(
            plan_response,
            react_responses,
        );

        let (plan, acceptance) = darius_cognitive::run_loop(
            &darius_cognitive::RunMetadata {
                profile: "e2e".into(),
                model: "mock".into(),
                mode: "auto".into(),
            },
            &policy,
            "what is the capital of France?",
            &mut model,
            &mut tools,
            &memory,
        )
        .unwrap();

        assert_eq!(plan.tasks.len(), 1);
        match acceptance {
            darius_cognitive::Acceptance::Accepted => {}
            darius_cognitive::Acceptance::Rejected(reason) => panic!("rejected: {reason}"),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&profile_dir);
    }

    #[test]
    fn ship_gate_spill_on_large_tool_output() {
        let profile_dir =
            std::env::temp_dir().join(format!("darius_ship_spill_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&profile_dir).unwrap();

        let memory = darius_memory::MemoryEngine::open(&profile_dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new(&profile_dir).unwrap();
        darius_tools::register_memory_builtins(&mut tools, &memory);

        // Insert a record with large body (near 32 KiB)
        let large_body = "x".repeat(32_768);
        memory
            .upsert(darius_memory::NewRecord {
                kind: darius_memory::RecordKind::Note,
                title: Some("large record".into()),
                body: large_body,
                tags: vec![],
                importance: 0.5,
                source: None,
            })
            .unwrap();

        // Search should return the large record
        let search_call = darius_tools::ToolCall {
            id: "spill-test".into(),
            name: "memory_search".into(),
            arguments: serde_json::json!({"text": "large record"}),
        };
        let outcome = tools.execute(&search_call);
        match outcome {
            darius_tools::ToolOutcome::Ok { preview, .. } => {
                // Preview should be capped at 1000 chars (from register_memory_builtins)
                assert!(preview.len() <= 1001, "preview too long: {}", preview.len());
            }
            darius_tools::ToolOutcome::Err { message } => panic!("search failed: {message}"),
        }

        // Verify spill directory exists
        let spill_dir = profile_dir.join("tool_results");
        assert!(spill_dir.exists());

        let _ = std::fs::remove_dir_all(&profile_dir);
    }
}
