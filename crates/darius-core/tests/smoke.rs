//! Smoke test: Darius public API contract at bootstrap.
//! Validates core, RLM, and harness_e2e surfaces compile and behave minimally.

use darius_core::{DariusError, SubagentId};
use darius_rlm::{rlm, rlm_evaluate, Grade, RlmHandle, RlmOptions, RlmStatus};
use harness_e2e::{MockLlm, TestDaemon};

#[test]
fn subagent_id_is_unique_and_debugable() {
    let a = SubagentId::new();
    let b = SubagentId::new();
    assert_ne!(a, b);
    let debug_a = format!("{:?}", a);
    assert!(!debug_a.is_empty());
}

#[test]
fn darius_error_impls_std_error() {
    let err: DariusError = DariusError::NotImplemented;
    assert!(err.to_string().contains("not implemented"));
}

#[test]
fn rlm_returns_handle_with_running_status() {
    let handle: RlmHandle =
        rlm("ping", RlmOptions::default()).expect("rlm should not fail at bootstrap");
    assert_eq!(handle.status(), RlmStatus::Running);
}

#[test]
fn rlm_evaluate_returns_structured_grade() {
    let grade: Grade =
        rlm_evaluate("target", "rubric").expect("rlm_evaluate should not fail at bootstrap");
    assert!(grade.passed);
    assert!(grade.notes.contains("target"));
}

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
fn laptop_compiles_cleanly_after_bootstrap() {
    // Sanity: all imported symbols resolve and types are inhabited.
    let _id = SubagentId::new();
    let _status = RlmStatus::Idle;
    let _grade = Grade {
        passed: true,
        scores: vec![],
        notes: String::new(),
    };
}
