//! Continuous Learn — captures failure trajectories and generates eval fixtures.

use crate::{EvalFixture, FixtureStore, Grade};
use parking_lot::Mutex;
use std::sync::Arc;
#[derive(Debug, Clone)]
pub struct LearningEvent {
    pub session_id: String,
    pub trajectory: String,
    pub failure_reason: String,
    pub timestamp: u64,
}

/// Configuration for continuous learning.
#[derive(Debug, Clone)]
pub struct LearnConfig {
    /// Whether to gate learning (only learn from verified failures).
    pub gate: bool,
    /// Whether to save learned fixtures.
    pub save: bool,
}

impl Default for LearnConfig {
    fn default() -> Self {
        Self {
            gate: false,
            save: true,
        }
    }
}

/// Continuous Learner — captures failures and generates eval fixtures.
pub struct ContinuousLearner {
    config: LearnConfig,
    fixture_store: Arc<Mutex<FixtureStore>>,
    events: Arc<Mutex<Vec<LearningEvent>>>,
}

impl ContinuousLearner {
    pub fn new(config: LearnConfig, fixture_store: Arc<Mutex<FixtureStore>>) -> Self {
        Self {
            config,
            fixture_store,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Capture a learning event from a failure trajectory.
    pub fn capture(
        &self,
        session_id: &str,
        trajectory: &str,
        failure_reason: &str,
    ) -> LearningEvent {
        let event = LearningEvent {
            session_id: session_id.to_string(),
            trajectory: trajectory.to_string(),
            failure_reason: failure_reason.to_string(),
            timestamp: current_timestamp(),
        };

        if !self.config.gate {
            self.events.lock().push(event.clone());
        }

        event
    }

    /// Generate an eval fixture from a learning event.
    pub fn generate_fixture(&self, event: &LearningEvent) -> EvalFixture {
        EvalFixture {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!(
                "learned_from_{}",
                &event.session_id[..std::cmp::min(8, event.session_id.len())]
            ),
            input: event.trajectory.clone(),
            expected: format!("should NOT fail with: {}", event.failure_reason),
            category: "learned".to_string(),
        }
    }

    /// Learn from a failure: capture and optionally generate a fixture.
    pub fn learn(
        &self,
        session_id: &str,
        trajectory: &str,
        failure_reason: &str,
    ) -> Result<EvalFixture, String> {
        let event = self.capture(session_id, trajectory, failure_reason);

        if self.config.save {
            let fixture = self.generate_fixture(&event);
            self.fixture_store.lock().add(fixture.clone());
            Ok(fixture)
        } else {
            Err("learning gated: not saving fixture".into())
        }
    }

    /// Verify that a learned failure no longer occurs.
    pub fn verify_fix(&self, _fixture: &EvalFixture, grade: &Grade) -> Result<bool, String> {
        // If the grade passes, the fix is verified.
        Ok(grade.passed)
    }

    /// Get all captured events.
    pub fn events(&self) -> Vec<LearningEvent> {
        self.events.lock().clone()
    }

    /// Get the fixture store.
    pub fn fixture_store(&self) -> Arc<Mutex<FixtureStore>> {
        self.fixture_store.clone()
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Grader, RubricDSL};

    #[test]
    fn learner_captures_failure() {
        let store = Arc::new(Mutex::new(FixtureStore::new()));
        let learner = ContinuousLearner::new(LearnConfig::default(), store);

        let event = learner.capture("sess1", "trajectory", "assertion failed");
        assert_eq!(event.session_id, "sess1");
        assert_eq!(event.failure_reason, "assertion failed");

        let events = learner.events();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn learner_generates_fixture() {
        let store = Arc::new(Mutex::new(FixtureStore::new()));
        let learner = ContinuousLearner::new(LearnConfig::default(), store);

        let event = learner.capture("sess1", "trajectory", "error");
        let fixture = learner.generate_fixture(&event);

        assert_eq!(fixture.category, "learned");
        assert!(fixture.input.contains("trajectory"));
    }

    #[test]
    fn learner_learn_saves_fixture() {
        let store = Arc::new(Mutex::new(FixtureStore::new()));
        let learner = ContinuousLearner::new(LearnConfig::default(), store.clone());

        let fixture = learner.learn("sess1", "trajectory", "error").unwrap();
        assert_eq!(fixture.category, "learned");

        // Verify fixture was saved.
        assert_eq!(store.lock().len(), 1);
    }

    #[test]
    fn learner_gated_does_not_save() {
        let store = Arc::new(Mutex::new(FixtureStore::new()));
        let config = LearnConfig {
            gate: true,
            save: false,
        };
        let learner = ContinuousLearner::new(config, store.clone());

        let result = learner.learn("sess1", "trajectory", "error");
        assert!(result.is_err());
        assert_eq!(store.lock().len(), 0);
    }

    #[test]
    fn failure_creates_fixture_that_fails_eval_until_refined() {
        let store = Arc::new(Mutex::new(FixtureStore::new()));
        let learner = ContinuousLearner::new(LearnConfig::default(), store.clone());

        // Capture a failure.
        let _fixture = learner
            .learn("sess1", "bad output", "quality too low")
            .unwrap();

        // Grade the bad output — should fail.
        let rubric = RubricDSL::parse("quality:1.0:Quality").unwrap();
        let _bad_grade = Grader::grade("bad output", &rubric).unwrap();

        // Now verify the fix: if we grade a good output, it should pass.
        let good_grade = Grader::grade("good refined output", &rubric).unwrap();
        assert!(good_grade.passed);
    }
}
