use crate::config::ProfileConfig;
use std::path::Path;

/// Ensure the default profile directory exists with safe workspace root.
/// Called on first run of any command that uses a profile.
#[allow(dead_code)]
pub fn ensure_profile(profile: &str) -> std::path::PathBuf {
    let dir = ProfileConfig::profile_dir(profile);
    std::fs::create_dir_all(&dir).ok();

    // Create tool_results directory for spill
    let tool_results = dir.join("tool_results");
    std::fs::create_dir_all(&tool_results).ok();

    dir
}

/// Check if a path is safe to write to (under profile directory).
#[allow(dead_code)]
pub fn is_safe_write_path(profile: &str, path: &str) -> bool {
    let profile_dir = ProfileConfig::profile_dir(profile);
    let target = Path::new(path);
    let canonical_profile = profile_dir.canonicalize().unwrap_or(profile_dir);

    match target.canonicalize() {
        Ok(canonical_target) => canonical_target.starts_with(&canonical_profile),
        Err(_) => {
            if let Some(parent) = target.parent() {
                match parent.canonicalize() {
                    Ok(canonical_parent) => canonical_parent.starts_with(&canonical_profile),
                    Err(_) => false,
                }
            } else {
                false
            }
        }
    }
}

/// Get the workspace root for tools (profile subdirectory).
#[allow(dead_code)]
pub fn tool_workspace(profile: &str) -> std::path::PathBuf {
    let dir = ProfileConfig::profile_dir(profile).join("workspace");
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_profile_creates_directories() {
        let profile = format!("test_{}", uuid::Uuid::new_v4());
        let dir = ensure_profile(&profile);
        assert!(dir.exists());
        assert!(dir.join("tool_results").exists());
        let _ = std::fs::remove_dir_all(ProfileConfig::profile_dir(&profile));
    }

    #[test]
    fn is_safe_write_path_accepts_paths_under_profile() {
        let profile = format!("test_{}", uuid::Uuid::new_v4());
        let dir = ensure_profile(&profile);
        let test_file = dir.join("output.txt");
        assert!(is_safe_write_path(&profile, &test_file.to_string_lossy()));
        let _ = std::fs::remove_dir_all(ProfileConfig::profile_dir(&profile));
    }

    #[test]
    fn is_safe_write_path_rejects_paths_outside_profile() {
        let profile = format!("test_{}", uuid::Uuid::new_v4());
        let _ = ensure_profile(&profile);
        assert!(!is_safe_write_path(&profile, "/etc/passwd"));
        assert!(!is_safe_write_path(&profile, "/tmp/evil.txt"));
        let _ = std::fs::remove_dir_all(ProfileConfig::profile_dir(&profile));
    }

    #[test]
    fn test_check_approval_tool_risk() {
        // Read-only inspection -> no approval required
        let (req1, risk1, _) =
            crate::check_approval("read_file", &serde_json::json!({"path": "src/main.rs"}));
        assert!(!req1);
        assert_eq!(risk1, "ReadOnly");

        // Shell -> approval required
        let (req2, risk2, _) =
            crate::check_approval("shell", &serde_json::json!({"command": "ls"}));
        assert!(req2);
        assert_eq!(risk2, "Mutating");

        // write_file to AGENTS.md -> approval required for protected path
        let (req3, risk3, reason3) =
            crate::check_approval("write_file", &serde_json::json!({"path": "AGENTS.md"}));
        assert!(req3);
        assert_eq!(risk3, "Mutating");
        assert!(reason3.contains("protected instruction file"));
    }
}
