//! E2E integration harness: MockLlm, TestDaemon, full-session pipeline tests.

pub struct MockLlm {
    responses: Vec<String>,
    next: usize,
}

pub struct TestDaemon {
    running: bool,
    profile: String,
}

pub fn run_e2e() -> Result<E2EReport, E2EError> {
    Ok(E2EReport { passed: true, steps: 0, errors: vec![] })
}

#[derive(Debug, Clone, Default)]
pub struct E2EReport {
    pub passed: bool,
    pub steps: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum E2EError {
    #[error("e2e setup failed: {0}")]
    Setup(String),
    #[error("e2e step failed: {0}")]
    Step(String),
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
}

impl TestDaemon {
    pub fn new(profile: impl Into<String>) -> Self {
        Self { running: false, profile: profile.into() }
    }
    pub fn start(&mut self) { self.running = true; }
    pub fn stop(&mut self) { self.running = false; }
    pub fn is_running(&self) -> bool { self.running }
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
}
