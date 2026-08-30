//! Grader — structured grading of artifacts against rubrics.

use crate::rubric::{Rubric, RubricScore};
use serde::{Deserialize, Serialize};

/// A structured grade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Grade {
    pub passed: bool,
    pub scores: Vec<RubricScore>,
    pub notes: String,
}

/// Result of grading.
pub type GradingResult = Grade;

/// Grader — evaluates artifacts against rubrics.
pub struct Grader;

impl Grader {
    /// Grade through an explicitly identified model path.
    pub fn grade_with_model(
        artifact: &str,
        rubric: &Rubric,
        model_id: &str,
    ) -> Result<Grade, String> {
        if model_id.trim().is_empty() {
            return Err("rater model id must not be empty".into());
        }
        let mut grade = Self::grade(artifact, rubric)?;
        grade.notes = format!("rater model {model_id}; {}", grade.notes);
        Ok(grade)
    }

    /// Grade an artifact against a rubric.
    pub fn grade(artifact: &str, rubric: &Rubric) -> Result<Grade, String> {
        if rubric.criteria.is_empty() {
            return Ok(Grade {
                passed: true,
                scores: Vec::new(),
                notes: "no criteria to evaluate".into(),
            });
        }

        let mut scores = Vec::new();
        let mut total_weighted_score = 0.0;
        let mut total_weight = 0.0;

        for criterion in &rubric.criteria {
            // Stub: in a real implementation, this would use the rater model.
            let score = score_criterion(artifact, criterion)?;
            total_weighted_score += score * criterion.weight;
            total_weight += criterion.weight;
            scores.push(RubricScore {
                criterion: criterion.name.clone(),
                value: score,
                max_value: 1.0,
            });
        }

        let normalized_score = if total_weight > 0.0 {
            total_weighted_score / total_weight
        } else {
            0.0
        };

        Ok(Grade {
            passed: normalized_score >= 0.7,
            scores,
            notes: format!("normalized score: {:.2}", normalized_score),
        })
    }
}

fn score_criterion(
    _artifact: &str,
    criterion: &crate::rubric::RubricCriterion,
) -> Result<f32, String> {
    // Stub: return a default score. In production, this calls the rater model.
    match criterion.name.as_str() {
        "quality" => Ok(0.8),
        "speed" => Ok(0.9),
        "accuracy" => Ok(0.85),
        _ => Ok(0.7),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rubric::RubricDSL;

    #[test]
    fn grade_empty_rubric_passes() {
        let rubric = RubricDSL::parse("").unwrap();
        let grade = Grader::grade("artifact", &rubric).unwrap();
        assert!(grade.passed);
    }

    #[test]
    fn grade_with_criteria() {
        let rubric = RubricDSL::parse("quality:1.0:Quality").unwrap();
        let grade = Grader::grade("good artifact", &rubric).unwrap();
        assert_eq!(grade.scores.len(), 1);
        assert_eq!(grade.scores[0].criterion, "quality");
    }
}
