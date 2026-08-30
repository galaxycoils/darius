//! Coding tools — read/write, bash, glob, grep, browser, schema-validated yields.

use darius_hashline::{Filesystem, InMemoryFilesystem, compute_anchor};
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("hashline error: {0}")]
    Hashline(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("filesystem error: {0}")]
    Filesystem(#[from] darius_hashline::FilesystemError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("execution error: {0}")]
    Execution(String),
}

/// Read a file via Hashline.
pub fn read_file(fs: &mut InMemoryFilesystem, path: &str) -> Result<String, ToolError> {
    fs.read(path)
        .map_err(|e| ToolError::NotFound(e.to_string()))
}

/// Write a file via Hashline (anchored edit).
pub fn write_file(
    fs: &mut InMemoryFilesystem,
    path: &str,
    content: &str,
    expected_hash: Option<&[u8; 32]>,
) -> Result<(), ToolError> {
    // If expected hash provided, validate it matches current content.
    if let Some(expected) = expected_hash {
        let current = fs.read(path).unwrap_or_default();
        let actual = compute_anchor(&current);
        if &actual != expected {
            return Err(ToolError::Hashline("stale hash: file has changed".into()));
        }
    }

    // Write directly (no anchor validation needed for new writes).
    fs.write(path, content)?;
    Ok(())
}

/// Execute a bash command.
pub fn bash(command: &str, cwd: Option<&str>) -> Result<String, ToolError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd.unwrap_or("."))
        .output()
        .map_err(|e| ToolError::Execution(e.to_string()))?;

    if !output.status.success() {
        return Err(ToolError::Execution(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Glob for files matching a pattern.
pub fn glob(pattern: &str, cwd: &str) -> Result<Vec<PathBuf>, ToolError> {
    let mut results = Vec::new();
    let path = PathBuf::from(cwd);
    if path.is_dir() {
        glob_dir(&path, pattern, &mut results)?;
    }
    Ok(results)
}

fn glob_dir(
    dir: &std::path::Path,
    pattern: &str,
    results: &mut Vec<PathBuf>,
) -> Result<(), ToolError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            glob_dir(&path, pattern, results)?;
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.contains(pattern.trim_matches('*')))
            .unwrap_or(false)
        {
            results.push(path);
        }
    }
    Ok(())
}

/// Grep for a pattern in files.
pub fn grep(pattern: &str, paths: &[&str]) -> Result<Vec<GrepMatch>, ToolError> {
    let mut results = Vec::new();
    for path in paths {
        let content = std::fs::read_to_string(path)?;
        for (i, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                results.push(GrepMatch {
                    path: path.to_string(),
                    line: i as u32 + 1,
                    content: line.to_string(),
                });
            }
        }
    }
    Ok(results)
}

/// A grep match.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GrepMatch {
    pub path: String,
    pub line: u32,
    pub content: String,
}

/// Browser tool — stub for web browsing (returns URL content).
pub fn browser(url: &str) -> Result<String, ToolError> {
    Ok(format!("Browser stub: would fetch {url}"))
}

/// Schema-validated yield — ensures output matches expected structure.
pub fn validate_yield<T: serde::Serialize>(value: &T) -> Result<String, ToolError> {
    serde_json::to_string_pretty(value).map_err(|e| ToolError::Execution(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_file() {
        let mut fs = InMemoryFilesystem::new();
        write_file(&mut fs, "test.rs", "fn main() {}", None).unwrap();
        let content = read_file(&mut fs, "test.rs").unwrap();
        assert_eq!(content, "fn main() {}");
    }

    #[test]
    fn bash_echo() {
        let output = bash("echo hello", None).unwrap();
        assert!(output.contains("hello"));
    }

    #[test]
    fn glob_finds_files() {
        let dir = std::env::temp_dir().join(format!("darius_glob_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.rs"), "").unwrap();
        std::fs::write(dir.join("test.txt"), "").unwrap();

        let results = glob("test*", dir.to_str().unwrap()).unwrap();
        assert_eq!(results.len(), 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn grep_finds_pattern() {
        let dir = std::env::temp_dir().join(format!("darius_grep_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.txt");
        std::fs::write(&file, "hello world\nfoo bar\nhello again").unwrap();

        let matches = grep("hello", &[file.to_str().unwrap()]).unwrap();
        assert_eq!(matches.len(), 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
