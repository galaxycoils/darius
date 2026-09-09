#![allow(dead_code, unused_imports)]
//! E2E integration harness — MockLlm, TestDaemon, full-session pipeline and Hermes ports matrix.

use darius_daemon::Daemon;
use darius_rlm::{IsolationTier, RlmKernel, RlmOptions, RlmStatus, rlm, rlm_evaluate};
use std::sync::Arc;

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
        self.next = 0;
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
    use darius_cognitive::SubagentRuntime;

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
        let mut tools = darius_tools::ToolRegistry::new_with_roots(
            &profile_dir,
            &profile_dir.join("tool_results"),
        )
        .unwrap();
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
            darius_tools::ToolOutcome::Interrupted | darius_tools::ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        // Step 3: AgentLoop with scripted AsyncModel
        let policy = darius_cognitive::LoopPolicy::default();
        let mut model = darius_cognitive::MockModel::new(vec![
            darius_cognitive::ModelOutput {
                content: None,
                tool_calls: vec![darius_tools::ToolCall {
                    id: "s1".into(),
                    name: "memory_search".into(),
                    arguments: serde_json::json!({"text": "France"}),
                }],
            },
            darius_cognitive::ModelOutput {
                content: Some("Paris is the capital of France.".into()),
                tool_calls: vec![],
            },
        ]);
        let (tx, _rx) = std::sync::mpsc::channel();
        let sink = std::sync::Arc::new(darius_cognitive::ChannelEventSink::new(tx));
        let control = std::sync::Arc::new(darius_cognitive::NoopRunControl);
        let loopt = darius_cognitive::AgentLoop::new(sink, control);
        let mut convo = darius_cognitive::Conversation::from_messages(vec![]).unwrap();
        let ws = profile_dir.to_string_lossy().to_string();
        let meta = darius_cognitive::RunMetadata {
            profile: "e2e".into(),
            model: "mock".into(),
            mode: "auto".into(),
        };
        let text = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(loopt.run_turn(
                &meta,
                &policy,
                "what is the capital of France?",
                &mut convo,
                &mut model,
                &tools,
                &memory,
                &ws,
            ))
            .unwrap();

        assert!(text.contains("Paris"));
        assert_eq!(convo.messages().len(), 4);

        let _ = std::fs::remove_dir_all(&profile_dir);
    }

    #[test]
    fn ship_gate_spill_on_large_tool_output() {
        let profile_dir =
            std::env::temp_dir().join(format!("darius_ship_spill_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&profile_dir).unwrap();

        let memory = darius_memory::MemoryEngine::open(&profile_dir).unwrap();
        let mut tools = darius_tools::ToolRegistry::new_with_roots(
            &profile_dir,
            &profile_dir.join("tool_results"),
        )
        .unwrap();
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
                assert!(preview.len() <= 1001, "preview too long: {}", preview.len());
            }
            darius_tools::ToolOutcome::Err { message } => panic!("search failed: {message}"),
            darius_tools::ToolOutcome::Interrupted | darius_tools::ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        // Verify spill directory exists
        let spill_dir = profile_dir.join("tool_results");
        assert!(spill_dir.exists());

        let _ = std::fs::remove_dir_all(&profile_dir);
    }

    #[test]
    fn e2e_lean_tail_compress_retention() {
        let mut messages = Vec::new();
        messages.push(darius_cognitive::ChatMessage {
            role: "system".into(),
            content: "SYSTEM_HEAD: persistent security policy".into(),
        });
        for i in 0..50 {
            messages.push(darius_cognitive::ChatMessage {
                role: "user".into(),
                content: format!("MIDDLE_STEP_{i}: verbose build log {}", "a".repeat(500)),
            });
        }
        messages.push(darius_cognitive::ChatMessage {
            role: "assistant".into(),
            content: "TAIL_OUTPUT: final verification success".into(),
        });

        let opts = darius_cognitive::CompressOpts {
            max_chars: 5_000,
            head_chars: 1_000,
            tail_chars: 2_000,
        };

        let compressed = darius_cognitive::lean_tail_compress(&messages, opts);
        let total_len: usize = compressed.iter().map(|m| m.content.len()).sum();
        assert!(total_len <= 5_000);
        assert!(compressed.first().unwrap().content.contains("SYSTEM_HEAD"));
        assert!(compressed.last().unwrap().content.contains("TAIL_OUTPUT"));
    }

    #[test]
    fn e2e_tool_spill_recall_workflow() {
        let dir = std::env::temp_dir().join(format!("darius_e2e_spill_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry =
            darius_tools::ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        darius_tools::register_coding_builtins(&mut registry);

        let large_src = dir.join("large_file.txt");
        let content = "START_LINE\n".to_string()
            + &("padding ".to_string() + &"x".repeat(64) + "\n").repeat(999);
        std::fs::write(&large_src, &content).unwrap();

        // 1. read_file spills to tool_results
        let read_call = darius_tools::ToolCall {
            id: "call-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({
                "path": large_src.to_str().unwrap(),
                "limit": 1000
            }),
        };
        let outcome = registry.execute(&read_call);
        let spilled_path = match outcome {
            darius_tools::ToolOutcome::Ok { spilled_path, .. } => {
                spilled_path.expect("spilled path expected")
            }
            darius_tools::ToolOutcome::Err { message } => panic!("read failed: {message}"),
            darius_tools::ToolOutcome::Interrupted | darius_tools::ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        };

        // 2. spill_read recalls from tool_results
        let recall_call = darius_tools::ToolCall {
            id: "call-2".into(),
            name: "spill_read".into(),
            arguments: serde_json::json!({
                "path": spilled_path.to_str().unwrap(),
                "offset": 0,
                "limit": 100
            }),
        };
        let recall_outcome = registry.execute(&recall_call);
        match recall_outcome {
            darius_tools::ToolOutcome::Ok { preview, .. } => {
                assert!(preview.starts_with("START_LINE"));
            }
            darius_tools::ToolOutcome::Err { message } => panic!("recall failed: {message}"),
            darius_tools::ToolOutcome::Interrupted | darius_tools::ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn e2e_subagent_steer_and_schema_validation() {
        let runtime = Arc::new(darius_cognitive::LocalSubagentRuntime::new());
        let schema = serde_json::json!({
            "type": "object",
            "required": ["report", "confidence"],
            "properties": {
                "report": { "type": "string" },
                "confidence": { "type": "number" }
            }
        });

        let id = runtime
            .spawn(
                "perform threat analysis",
                darius_cognitive::SpawnOpts {
                    max_iters: 10,
                    output_schema: Some(schema.clone()),
                },
            )
            .unwrap();

        assert_eq!(runtime.list_running().len(), 1);
        runtime.steer(&id, "narrow down to ssh anomalies").unwrap();

        let valid_json = r#"{"report": "found 1 bruteforce attempt", "confidence": 0.95}"#;
        assert!(darius_cognitive::validate_json_schema(valid_json, &schema).is_ok());

        let invalid_json = r#"{"report": "missing confidence"}"#;
        assert!(darius_cognitive::validate_json_schema(invalid_json, &schema).is_err());

        let partial = runtime.stop(&id).unwrap();
        assert!(partial.terminated);
        assert!(runtime.list_running().is_empty());
    }

    #[test]
    fn e2e_cron_memory_continuity_across_runs() {
        let scheduler = darius_daemon::CronScheduler::new();
        let job = darius_daemon::CronJob::new("dep-audit", "0 2 * * *", "cargo audit");
        scheduler.add_job(job).unwrap();

        // Run 1
        scheduler
            .append_notepad(
                "dep-audit",
                "Vulnerability found: CVE-2026-001 in old_crate",
            )
            .unwrap();

        // Run 2: context loaded with prior notepad
        let ctx = scheduler.build_job_context("dep-audit", None).unwrap();
        assert!(ctx.contains("Vulnerability found: CVE-2026-001"));
    }

    #[test]
    fn e2e_instruction_write_protection() {
        let dir = std::env::temp_dir().join(format!("darius_e2e_prot_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry =
            darius_tools::ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        darius_tools::register_coding_builtins(&mut registry);

        let skill_file = dir.join("SKILL.md");

        // Unapproved write denied
        let call_unapproved = darius_tools::ToolCall {
            id: "c1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": skill_file.to_str().unwrap(),
                "content": "unauthorized instructions"
            }),
        };
        assert!(matches!(
            registry.execute(&call_unapproved),
            darius_tools::ToolOutcome::Err { .. }
        ));

        // Model-supplied approved:true no longer bypasses protection (2.4):
        // protected writes are hard-denied; approval flows via RunControl.
        let call_approved = darius_tools::ToolCall {
            id: "c2".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": skill_file.to_str().unwrap(),
                "content": "authorized instructions",
                "approved": true
            }),
        };
        assert!(matches!(
            registry.execute(&call_approved),
            darius_tools::ToolOutcome::Err { .. }
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn e2e_mcp_discovery_and_step_gating() {
        let dir = std::env::temp_dir().join(format!("darius_e2e_mcp_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry =
            darius_tools::ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();

        let client = Arc::new(darius_tools::LocalMcpClient::new());
        client.add_tool(darius_tools::McpToolDef {
            name: "k8s_scale".into(),
            description: "Scale Kubernetes deployment".into(),
            input_schema: serde_json::json!({}),
            requires_prior_success: true,
        });

        darius_tools::register_mcp_tools(&mut registry, client.clone()).unwrap();

        let call = darius_tools::ToolCall {
            id: "mcp-call".into(),
            name: "k8s_scale".into(),
            arguments: serde_json::json!({"replicas": 3}),
        };

        // 1. Prior step failed -> denied
        client.set_prior_step_succeeded(false);
        assert!(matches!(
            registry.execute(&call),
            darius_tools::ToolOutcome::Err { .. }
        ));

        // 2. Prior step succeeded -> allowed
        client.set_prior_step_succeeded(true);
        assert!(matches!(
            registry.execute(&call),
            darius_tools::ToolOutcome::Ok { .. }
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn e2e_peer_a2a_messaging_matrix() {
        // Without an injected executor no execution capability is advertised.
        let (bare_state, _) = darius_web::ServerState::new();
        let _router = darius_web::create_router(bare_state);
        let bare_card = darius_web::agent_card();
        assert!(bare_card.capabilities.is_empty());

        // With an executor the card advertises exactly the transport capabilities.
        let executor: darius_web::GoalExecutor =
            std::sync::Arc::new(|goal, _sink| Ok(format!("executed: {goal}")));
        let exec_state = darius_web::ServerState::with_executor(executor);
        let router = darius_web::create_router(exec_state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let body = http_get(address, "/a2a/card").await;
        let card: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            card["capabilities"],
            serde_json::json!(["goal_execution", "task_lookup", "task_sse"])
        );
    }

    async fn http_get(address: std::net::SocketAddr, path: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let wire = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        socket.write_all(wire.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8(bytes).unwrap();
        text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned()
    }
}
