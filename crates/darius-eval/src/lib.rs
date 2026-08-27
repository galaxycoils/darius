//! Darius evaluation flywheel.
//!
//! Produces artifacts that an independent AutoRater (role `rater`) grades
//! against a structured rubric. Results feed the Continual Harness.

pub mod fixtures;
pub mod grader;
pub mod rater;
pub mod rubric;

pub use fixtures::{EvalFixture, FixtureStore};
pub use grader::{Grade, GradingResult};
pub use rater::{AutoRater, RaterConfig};
pub use rubric::{Rubric, RubricScore, RubricDSL};
