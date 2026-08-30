//! SKILL.md parser for the Skill Curator lifecycle.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Parsed skill manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: Option<String>,
    pub version: String,
    pub body: String,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// Errors that can occur when parsing a SKILL.md manifest.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("empty SKILL.md input")]
    EmptyInput,
    #[error("missing required field: name")]
    MissingName,
    #[error("invalid frontmatter structure")]
    InvalidFrontmatter,
}

/// Parse a SKILL.md manifest into a [`Skill`].
pub fn parse(skill_md: &str) -> Result<Skill, ParseError> {
    let trimmed = skill_md.trim();
    if trimmed.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    let mut lines = trimmed.lines();
    let first = lines.next().unwrap();

    if first == "---" {
        let mut fm_lines = Vec::new();
        for line in lines {
            if line == "---" {
                let body_start = trimmed
                    .find("\n---\n")
                    .map(|p| p + 4)
                    .unwrap_or(trimmed.len());
                let body = trimmed[body_start..].trim();
                return parse_frontmatter(&fm_lines, body);
            }
            fm_lines.push(line);
        }
        return Err(ParseError::InvalidFrontmatter);
    }

    parse_simple_markdown(trimmed)
}

fn parse_frontmatter(fm_lines: &[&str], body: &str) -> Result<Skill, ParseError> {
    let mut name = None;
    let mut description = None;
    let mut version = "0.1.0".to_string();
    let mut metadata = std::collections::HashMap::new();

    for line in fm_lines {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key.as_str() {
                "name" => name = Some(value.to_string()),
                "description" => description = Some(value.to_string()),
                "version" => version = value.to_string(),
                _ => {
                    metadata.insert(key, serde_json::Value::String(value.to_string()));
                }
            }
        }
    }

    let name = name.ok_or(ParseError::MissingName)?;

    Ok(Skill {
        name,
        description,
        version,
        body: body.to_string(),
        metadata,
    })
}

fn parse_simple_markdown(content: &str) -> Result<Skill, ParseError> {
    for line in content.lines() {
        if let Some(stripped) = line.strip_prefix("# ") {
            let name = stripped.trim().to_string();
            if !name.is_empty() {
                let body_start = content.find(line).unwrap() + line.len();
                let body = content[body_start..].trim().to_string();
                return Ok(Skill {
                    name,
                    description: None,
                    version: "0.1.0".to_string(),
                    body,
                    metadata: std::collections::HashMap::new(),
                });
            }
        }
    }

    Err(ParseError::MissingName)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_frontmatter() {
        let input = "---\nname: test-skill\ndescription: A test skill\nversion: 1.0.0\n---\n# Test Skill\n\nThis is the body.\n";
        let skill = parse(input).unwrap();
        assert_eq!(skill.name, "test-skill");
        assert_eq!(skill.description, Some("A test skill".to_string()));
        assert_eq!(skill.version, "1.0.0");
        assert!(skill.body.contains("This is the body"));
    }

    #[test]
    fn parse_simple_markdown_heading() {
        let input = "# My Skill\n\nThis is the body content.\n";
        let skill = parse(input).unwrap();
        assert_eq!(skill.name, "My Skill");
        assert!(skill.body.contains("body content"));
    }

    #[test]
    fn parse_empty_input() {
        assert_eq!(parse(""), Err(ParseError::EmptyInput));
        assert_eq!(parse("   "), Err(ParseError::EmptyInput));
    }

    #[test]
    fn parse_missing_name() {
        let input = "---\ndescription: no name\n---\nbody\n";
        assert_eq!(parse(input), Err(ParseError::MissingName));
    }

    #[test]
    fn parse_invalid_frontmatter() {
        let input = "---\nname: test\nno closing delimiter\n";
        assert_eq!(parse(input), Err(ParseError::InvalidFrontmatter));
    }

    #[test]
    fn parse_metadata_fields() {
        let input = "---\nname: test\nauthor: alice\n---\nbody\n";
        let skill = parse(input).unwrap();
        assert_eq!(
            skill.metadata.get("author").unwrap().as_str().unwrap(),
            "alice"
        );
    }
}
