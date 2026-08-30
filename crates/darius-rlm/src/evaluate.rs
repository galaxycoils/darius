//! Generator–evaluator: structured grading of artifacts against rubrics.

use crate::{DariusError, Grade, RubricScore};

/// Evaluate an artifact against a rubric using a structured rubric parser.
/// This is a stub until ModelRouter rater is wired.
pub fn rlm_evaluate(target: &str, _rubric: &str) -> Result<Grade, DariusError> {
    Ok(Grade {
        passed: true,
        scores: vec![RubricScore {
            criterion: "quality".into(),
            value: 0.8,
            max_value: 1.0,
        }],
        notes: format!("evaluated target: {target}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_returns_grade_with_scores() {
        let grade = rlm_evaluate("test target", "quality rubric").unwrap();
        assert!(grade.passed);
        assert!(!grade.scores.is_empty());
        assert!(grade.notes.contains("test target"));
    }
}
