//! SKILL.md parser for the Skill Curator lifecycle.
//!
//! Parses agent skill manifests (SKILL.md) into structured `Skill` objects
//! for registration, curation, and lifecycle management by the Darius daemon.

use thiserror::Error;

/// Parsed skill manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// Skill name from frontmatter.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Raw body content after frontmatter.
    pub body: String,
}

/// Errors that can occur when parsing a SKILL.md manifest.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Frontmatter delimiters (`---`) missing or malformed.
    #[error("missing or malformed frontmatter delimiters")]
    InvalidFrontmatter,
    /// Required `name` field missing from frontmatter.
    #[error("missing required field: name")]
    MissingName,
}

/// Parse a SKILL.md manifest into a [`Skill`].
///
/// # Errors
///
/// Returns [`ParseError`] if the frontmatter is malformed or required fields
pub fn parse(_skill_md: &str) -> Result<Skill, ParseError> {
    Err(ParseError::InvalidFrontmatter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stub_returns_unimplemented() {
        let result = parse("---\nname: test\n---\nbody");
        assert!(result.is_err());
    }
}
