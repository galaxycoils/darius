//! Rubric DSL — structured evaluation criteria.

use serde::{Deserialize, Serialize};

/// A single rubric criterion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricCriterion {
    pub name: String,
    pub weight: f32,
    pub description: String,
}

/// A complete rubric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rubric {
    pub criteria: Vec<RubricCriterion>,
}

/// A score for a single criterion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RubricScore {
    pub criterion: String,
    pub value: f32,
    #[serde(rename = "max")]
    pub max_value: f32,
}

/// DSL for parsing rubric definitions.
pub struct RubricDSL;

impl RubricDSL {
    /// Parse a rubric from a simple text definition.
    ///
    /// Format: one criterion per line, `name:weight:description`.
    pub fn parse(source: &str) -> Result<Rubric, String> {
        if source.trim().is_empty() {
            return Ok(Rubric { criteria: Vec::new() });
        }

        let mut criteria = Vec::new();
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() < 2 {
                return Err(format!("invalid criterion line: {line}"));
            }
            let name = parts[0].trim().to_string();
            let weight: f32 = parts[1].trim().parse().map_err(|_| format!("invalid weight: {}", parts[1]))?;
            let description = parts.get(2).unwrap_or(&"").trim().to_string();
            criteria.push(RubricCriterion { name, weight, description });
        }

        Ok(Rubric { criteria })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_rubric() {
        let r = RubricDSL::parse("").unwrap();
        assert!(r.criteria.is_empty());
    }

    #[test]
    fn parse_single_criterion() {
        let r = RubricDSL::parse("quality:1.0:Overall quality").unwrap();
        assert_eq!(r.criteria.len(), 1);
        assert_eq!(r.criteria[0].name, "quality");
        assert_eq!(r.criteria[0].weight, 1.0);
    }

    #[test]
    fn parse_multiple_criteria() {
        let r = RubricDSL::parse("quality:0.5:Quality\nspeed:0.5:Speed").unwrap();
        assert_eq!(r.criteria.len(), 2);
    }

    #[test]
    fn parse_invalid_weight() {
        assert!(RubricDSL::parse("quality:abc:desc").is_err());
    }
}
