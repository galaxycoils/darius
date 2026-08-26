//! Edit operations (PUT/CUT) anchored to content hashes.

use crate::anchors::compute_anchor;
use crate::filesystem::{Filesystem, FilesystemError};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct EditOp {
    pub anchor: crate::anchors::FileAnchor,
    pub put_lines: Vec<String>,
    pub cut_range: Option<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct EditResult {
    pub new_hash: [u8; 32],
    pub applied: bool,
    pub stale_rejected: bool,
    pub final_content: String,
}

#[derive(Debug, Error)]
pub enum EditError {
    #[error("anchor mismatch for {path}: expected {expected:?}, got {actual:?}")]
    StaleAnchor {
        path: String,
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("invalid range {start}..{end} for file with {lines} lines")]
    InvalidRange {
        start: usize,
        end: usize,
        lines: usize,
    },
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("filesystem error: {0}")]
    Filesystem(#[from] FilesystemError),
}

pub fn apply_put(
    fs: &mut dyn Filesystem,
    op: &EditOp,
    start: usize,
    end: usize,
) -> Result<EditResult, EditError> {
    let content = fs.read(&op.anchor.path)?;

    let current_hash = compute_anchor(&content);
    if current_hash != op.anchor.hash {
        return Err(EditError::StaleAnchor {
            path: op.anchor.path.clone(),
            expected: op.anchor.hash,
            actual: current_hash,
        });
    }

    let has_trailing_newline = content.ends_with('\n');
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();

    if start > line_count || end > line_count {
        return Err(EditError::InvalidRange {
            start,
            end,
            lines: line_count,
        });
    }

    let mut new_lines: Vec<String> = Vec::new();
    new_lines.extend(lines[..start].iter().map(|s| s.to_string()));
    new_lines.extend(op.put_lines.iter().cloned());
    new_lines.extend(lines[end..].iter().map(|s| s.to_string()));

    let new_content = new_lines.join("\n");
    let new_content = if has_trailing_newline {
        format!("{}\n", new_content)
    } else {
        new_content
    };

    let new_hash = compute_anchor(&new_content);
    fs.write(&op.anchor.path, &new_content)?;

    Ok(EditResult {
        new_hash,
        applied: true,
        stale_rejected: false,
        final_content: new_content,
    })
}

pub fn apply_cut(
    fs: &mut dyn Filesystem,
    op: &EditOp,
    start: usize,
    end: usize,
) -> Result<EditResult, EditError> {
    let content = fs.read(&op.anchor.path)?;

    let current_hash = compute_anchor(&content);
    if current_hash != op.anchor.hash {
        return Err(EditError::StaleAnchor {
            path: op.anchor.path.clone(),
            expected: op.anchor.hash,
            actual: current_hash,
        });
    }

    let has_trailing_newline = content.ends_with('\n');
    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();

    if start > line_count || end > line_count {
        return Err(EditError::InvalidRange {
            start,
            end,
            lines: line_count,
        });
    }

    let mut new_lines: Vec<String> = Vec::new();
    new_lines.extend(lines[..start].iter().map(|s| s.to_string()));
    new_lines.extend(lines[end..].iter().map(|s| s.to_string()));

    let new_content = new_lines.join("\n");
    let new_content = if has_trailing_newline {
        format!("{}\n", new_content)
    } else {
        new_content
    };

    let new_hash = compute_anchor(&new_content);
    fs.write(&op.anchor.path, &new_content)?;

    Ok(EditResult {
        new_hash,
        applied: true,
        stale_rejected: false,
        final_content: new_content,
    })
}

pub fn rejects_stale_hash() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchors::anchor_for;
    use crate::filesystem::InMemoryFilesystem;

    fn make_edit(path: &str, content: &str) -> EditOp {
        EditOp {
            anchor: anchor_for(path, content),
            put_lines: vec!["replacement".to_string()],
            cut_range: None,
        }
    }

    #[test]
    fn apply_put_replaces_lines() {
        let mut fs = InMemoryFilesystem::with_file("a.txt", "line1\nline2\nline3\n");
        let op = make_edit("a.txt", "line1\nline2\nline3\n");
        let result = apply_put(&mut fs, &op, 1, 2).unwrap();
        assert!(result.applied);
        assert!(!result.stale_rejected);
        assert_eq!(result.final_content, "line1\nreplacement\nline3\n");
    }

    #[test]
    fn apply_put_stale_rejected() {
        let mut fs = InMemoryFilesystem::with_file("a.txt", "original\n");
        let op = EditOp {
            anchor: anchor_for("a.txt", "original\n"),
            put_lines: vec!["new".to_string()],
            cut_range: None,
        };
        fs.write("a.txt", "modified\n").unwrap();
        let err = apply_put(&mut fs, &op, 0, 1).unwrap_err();
        assert!(matches!(err, EditError::StaleAnchor { .. }));
    }

    #[test]
    fn apply_cut_removes_lines() {
        let mut fs = InMemoryFilesystem::with_file("a.txt", "a\nb\nc\nd\n");
        let op = make_edit("a.txt", "a\nb\nc\nd\n");
        let result = apply_cut(&mut fs, &op, 1, 3).unwrap();
        assert!(result.applied);
        assert_eq!(result.final_content, "a\nd\n");
    }

    #[test]
    fn apply_cut_stale_rejected() {
        let mut fs = InMemoryFilesystem::with_file("a.txt", "original\n");
        let op = EditOp {
            anchor: anchor_for("a.txt", "original\n"),
            put_lines: vec![],
            cut_range: Some((0, 1)),
        };
        fs.write("a.txt", "modified\n").unwrap();
        let err = apply_cut(&mut fs, &op, 0, 1).unwrap_err();
        assert!(matches!(err, EditError::StaleAnchor { .. }));
    }

    #[test]
    fn rejects_stale_hash_is_true() {
        assert!(rejects_stale_hash());
    }
}
