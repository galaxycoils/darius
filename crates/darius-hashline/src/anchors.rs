//! Anchor types for content-hash edit operations.

use blake3::Hasher;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAnchor {
    pub path: String,
    pub hash: [u8; 32],
    pub line_count: usize,
    pub ast_boundary: Option<AstBoundary>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AstBoundary {
    pub enclosing_scope: Option<String>,
    pub in_function_body: bool,
}

pub fn compute_anchor(content: &str) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(bytes);
    arr
}

pub fn anchor_for(path: &str, content: &str) -> FileAnchor {
    let hash = compute_anchor(content);
    let line_count = content.lines().count();
    FileAnchor {
        path: path.to_string(),
        hash,
        line_count,
        ast_boundary: None,
    }
}

pub fn anchors_match(a: &FileAnchor, b: &FileAnchor) -> bool {
    a.hash == b.hash && a.path == b.path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_content_same_hash() {
        let a = compute_anchor("hello world");
        let b = compute_anchor("hello world");
        assert_eq!(a, b);
    }

    #[test]
    fn different_content_different_hash() {
        let a = compute_anchor("foo");
        let b = compute_anchor("bar");
        assert_ne!(a, b);
    }

    #[test]
    fn anchor_matches_same_content() {
        let f1 = anchor_for("a.txt", "x");
        let f2 = anchor_for("a.txt", "x");
        assert!(anchors_match(&f1, &f2));
    }

    #[test]
    fn anchor_does_not_match_different_path() {
        let f1 = anchor_for("a.txt", "x");
        let f2 = anchor_for("b.txt", "x");
        assert!(!anchors_match(&f1, &f2));
    }

    #[test]
    fn anchor_does_not_match_different_content() {
        let f1 = anchor_for("a.txt", "x");
        let f2 = anchor_for("a.txt", "y");
        assert!(!anchors_match(&f1, &f2));
    }
}
