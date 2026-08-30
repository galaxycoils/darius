//! Darius evaluation flywheel.
//!
//! Produces artifacts that an independent AutoRater (role `rater`) grades
//! against a structured rubric. Results feed the Continual Harness.

pub mod fixtures;
pub mod grader;
pub mod learn;
pub mod rater;
pub mod rubric;

pub use fixtures::{EvalFixture, FixtureStore};
pub use grader::{Grade, Grader, GradingResult};
pub use learn::{ContinuousLearner, LearnConfig, LearningEvent};
pub use rater::{AutoRater, RaterConfig};
pub use rubric::{Rubric, RubricDSL, RubricScore};
