use crate::paths::{DariusPaths, PathError};
use std::path::Path;

/// Ensure a profile directory exists with a safe workspace root.
#[allow(dead_code)]
pub fn ensure_profile(paths: &DariusPaths, profile: &str) -> Result<std::path::PathBuf, PathError> {
    let directory = paths.profile(profile)?;
    std::fs::create_dir_all(&directory).map_err(PathError::Io)?;
    std::fs::create_dir_all(directory.join("tool_results")).map_err(PathError::Io)?;
    Ok(directory)
}

/// Check if a path is safe to write to (under a profile directory).
#[allow(dead_code)]
pub fn is_safe_write_path(
    paths: &DariusPaths,
    profile: &str,
    path: &str,
) -> Result<bool, PathError> {
    let profile_dir = paths.profile(profile)?;
    let target = Path::new(path);
    let canonical_profile = profile_dir.canonicalize().unwrap_or(profile_dir);
    Ok(match target.canonicalize() {
        Ok(canonical_target) => canonical_target.starts_with(&canonical_profile),
        Err(_) => target
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .is_some_and(|parent| parent.starts_with(&canonical_profile)),
    })
}

/// Get the workspace root for tools (profile subdirectory).
#[allow(dead_code)]
pub fn tool_workspace(paths: &DariusPaths, profile: &str) -> Result<std::path::PathBuf, PathError> {
    let directory = paths.profile(profile)?.join("workspace");
    std::fs::create_dir_all(&directory).map_err(PathError::Io)?;
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths(temp: &TempDir) -> DariusPaths {
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        DariusPaths { home, workspace }
    }

    #[test]
    fn ensure_profile_creates_directories() {
        let temp = TempDir::new().unwrap();
        let dir = ensure_profile(&paths(&temp), "test").unwrap();
        assert!(dir.join("tool_results").exists());
    }

    #[test]
    fn is_safe_write_path_accepts_paths_under_profile() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let dir = ensure_profile(&paths, "test").unwrap();
        assert!(is_safe_write_path(&paths, "test", &dir.join("out").to_string_lossy()).unwrap());
    }

    #[test]
    fn is_safe_write_path_rejects_paths_outside_profile() {
        let temp = TempDir::new().unwrap();
        let paths = paths(&temp);
        let _ = ensure_profile(&paths, "test").unwrap();
        assert!(!is_safe_write_path(&paths, "test", "/tmp/evil.txt").unwrap());
    }

    #[test]
    fn check_approval_tool_risk() {
        let (required, risk, _) = crate::check_approval("read_file", &serde_json::json!({}));
        assert!(!required);
        assert_eq!(risk, "ReadOnly");
    }
}
