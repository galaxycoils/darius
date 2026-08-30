//! AutoRater — independent evaluation via the `rater` model role.

use crate::grader::{Grade, Grader};
use crate::rubric::Rubric;
use darius_daemon::model_router::ModelRole;

/// Configuration for the AutoRater.
#[derive(Debug, Clone)]
pub struct RaterConfig {
    pub model_role: ModelRole,
    pub model_id: String,
    pub optimizer_model_id: String,
    pub rubric: Rubric,
    pub threshold: f32,
}

impl Default for RaterConfig {
    fn default() -> Self {
        Self {
            model_role: ModelRole::Rater,
            model_id: "claude-3".into(),
            optimizer_model_id: "gpt-4".into(),
            rubric: Rubric {
                criteria: Vec::new(),
            },
            threshold: 0.7,
        }
    }
}

/// AutoRater — uses the rater role to independently evaluate artifacts.
pub struct AutoRater {
    config: RaterConfig,
}

impl AutoRater {
    pub fn new(config: RaterConfig) -> Self {
        Self { config }
    }

    /// Grade an artifact through the independent rater model path.
    pub fn grade(&self, target: &str) -> Result<Grade, String> {
        if self.config.model_role != ModelRole::Rater {
            return Err("AutoRater requires the rater model role".into());
        }
        if self.config.model_id == self.config.optimizer_model_id {
            return Err("rater model must differ from optimizer model".into());
        }
        Grader::grade_with_model(target, &self.config.rubric, &self.config.model_id)
    }

    /// Verify the rater path differs from the optimizer path.
    pub fn uses_rater_role(&self) -> bool {
        self.config.model_role == ModelRole::Rater
            && self.config.model_id != self.config.optimizer_model_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rubric::RubricDSL;

    #[test]
    fn rater_uses_rater_role() {
        let config = RaterConfig::default();
        let rater = AutoRater::new(config);
        assert!(rater.uses_rater_role());
    }

    #[test]
    fn rater_returns_grade() {
        let rubric = RubricDSL::parse("quality:1.0:Quality").unwrap();
        let config = RaterConfig {
            model_role: ModelRole::Rater,
            model_id: "claude-3".into(),
            optimizer_model_id: "gpt-4".into(),
            rubric,
            threshold: 0.7,
        };
        let rater = AutoRater::new(config);
        let grade = rater.grade("test artifact").unwrap();
        assert!(grade.passed);
        assert!(grade.notes.contains("rater model claude-3"));
    }

    #[test]
    fn rater_rejects_optimizer_identity() {
        let config = RaterConfig {
            model_role: ModelRole::Rater,
            model_id: "same-model".into(),
            optimizer_model_id: "same-model".into(),
            ..RaterConfig::default()
        };
        let rater = AutoRater::new(config);
        assert!(rater.grade("test artifact").is_err());
    }

    #[test]
    fn rater_path_differs_from_optimizer() {
        let config = RaterConfig::default();
        let rater = AutoRater::new(config);
        // The rater uses ModelRole::Rater, not ModelRole::Default (optimizer).
        assert_ne!(rater.config.model_role, ModelRole::Default);
        assert_ne!(rater.config.model_id, rater.config.optimizer_model_id);
    }
}
