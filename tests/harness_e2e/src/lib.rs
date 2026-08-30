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
}
