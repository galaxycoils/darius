//! Atomic same-directory file write bound to the workspace.
use crate::{PathPolicy, ToolError, ToolOutcome};
use similar::TextDiff;
use std::io::Write as _;

/// Max bytes accepted for a single write (1 MiB).
pub const MAX_WRITE_BYTES: usize = 1024 * 1024;

/// Convert diff to preview format with + and - lines
fn format_diff_preview(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();
    for change in diff.iter_all_changes() {
        let prefix = match change.tag() {
            similar::ChangeTag::Equal => " ",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Delete => "-",
        };
        lines.push(format!("{prefix}{}", change.value()));
    }
    lines.join("\n")
}

/// Write `content` to `raw` atomically: temp file in the same
/// directory, then rename. Rejects NUL bytes.
pub fn write_atomic(
    policy: &PathPolicy,
    raw: &str,
    content: &str,
) -> Result<ToolOutcome, ToolError> {
    if content.contains('\0') {
        return Err(ToolError::InvalidArgs("binary content rejected".into()));
    }
    if content.len() > MAX_WRITE_BYTES {
        return Err(ToolError::InvalidArgs("content exceeds 1 MiB".into()));
    }
    crate::ensure_dirs::ensure_parent_dirs(policy.root(), raw)?;
    let path = policy.resolve(raw, true)?;
    let parent = path
        .parent()
        .ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
    std::fs::create_dir_all(parent)?;

    // Read old content if file exists for diff
    let old_content = std::fs::read_to_string(&path).unwrap_or_default();

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(&path).map_err(|e| ToolError::Io(e.error))?;

    // Compute diff if file existed before
    let preview = if !old_content.is_empty() && old_content != content {
        format!(
            "wrote {} bytes to {}\n{}",
            content.len(),
            path.display(),
            format_diff_preview(&old_content, content)
        )
    } else {
        format!("wrote {} bytes to {}", content.len(), path.display())
    };

    Ok(ToolOutcome::Ok {
        preview,
        spilled_path: None,
    })
}
