//! Darius evaluation flywheel.
//!
//! Produces artifacts that an independent AutoRater (role `rater`) grades
//! against a structured rubric. Results feed the Continual Harness.

pub mod fixtures {}
pub mod grader {}
pub mod rater {}
pub mod rubric {}
