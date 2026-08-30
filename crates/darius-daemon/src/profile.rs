//! Profile Isolation — multi-tenant data separation.

use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("profile not found: {0}")]
    NotFound(String),
    #[error("invalid profile name: {0}")]
    InvalidName(String),
}

/// A Darius profile (tenant).
pub struct Profile {
    pub name: String,
    pub path: PathBuf,
}

impl Profile {
    /// Get the base directory for all profiles.
    pub fn base_dir() -> PathBuf {
        if let Ok(profile) = std::env::var("DARIUS_PROFILE_DIR") {
            PathBuf::from(profile)
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".darius")
                .join("profiles")
        }
    }

    /// Create or load a profile.
    pub fn new(name: impl Into<String>) -> Result<Self, ProfileError> {
        Self::with_base_dir(name, Self::base_dir())
    }

    /// Create or load a profile with a specific base directory.
    pub fn with_base_dir(
        name: impl Into<String>,
        base_dir: impl AsRef<Path>,
    ) -> Result<Self, ProfileError> {
        let name = name.into();
        let mut components = Path::new(&name).components();
        let valid_name =
            matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
        if !valid_name {
            return Err(ProfileError::InvalidName(name));
        }

        std::fs::create_dir_all(base_dir.as_ref())?;
        let base = base_dir.as_ref().canonicalize()?;
        let candidate = base.join(&name);
        std::fs::create_dir_all(&candidate)?;
        let path = candidate.canonicalize()?;
        if !path.starts_with(&base) {
            return Err(ProfileError::InvalidName(name));
        }

        std::fs::create_dir_all(path.join("memory"))?;
        std::fs::create_dir_all(path.join("skills"))?;
        std::fs::create_dir_all(path.join("sessions"))?;
        Ok(Self { name, path })
    }

    /// Get the path to a profile subdirectory.
    pub fn subdir(&self, sub: &str) -> PathBuf {
        self.path.join(sub)
    }

    /// Get the database path for this profile.
    pub fn db_path(&self) -> PathBuf {
        self.path.join("state.db")
    }

    /// Get the memory directory for this profile.
    pub fn memory_dir(&self) -> PathBuf {
        self.path.join("memory")
    }

    /// Get the skills directory for this profile.
    pub fn skills_dir(&self) -> PathBuf {
        self.path.join("skills")
    }

    /// Get the sessions directory for this profile.
    pub fn sessions_dir(&self) -> PathBuf {
        self.path.join("sessions")
    }

    /// List all existing profiles.
    pub fn list() -> Result<Vec<String>, ProfileError> {
        Self::list_in_dir(Self::base_dir())
    }

    /// List profiles in a specific directory.
    pub fn list_in_dir(base_dir: impl AsRef<Path>) -> Result<Vec<String>, ProfileError> {
        let base = base_dir.as_ref();
        if !base.exists() {
            return Ok(Vec::new());
        }
        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(base)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            profiles.push(name.to_string());
        }
        Ok(profiles)
    }

    /// Delete a profile and all its data.
    pub fn delete(&self) -> Result<(), ProfileError> {
        if self.path.exists() {
            std::fs::remove_dir_all(&self.path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_profile_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("darius_profile_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn profile_creates_isolated_directories() {
        let dir = temp_profile_dir();
        let profile = Profile::with_base_dir("test_profile", &dir).unwrap();
        assert!(profile.path.exists());
        assert!(profile.memory_dir().exists());
        assert!(profile.skills_dir().exists());
        assert!(profile.sessions_dir().exists());
        assert!(profile.db_path().parent().unwrap().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn profile_rejects_path_traversal() {
        let dir = temp_profile_dir();
        for invalid in ["../escaped", "nested/profile", ".", ""] {
            assert!(matches!(
                Profile::with_base_dir(invalid, &dir),
                Err(ProfileError::InvalidName(_))
            ));
        }
        assert!(!dir.parent().unwrap().join("escaped").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_profiles_do_not_share_data() {
        let dir = temp_profile_dir();
        let p1 = Profile::with_base_dir("tenant_a", &dir).unwrap();
        let p2 = Profile::with_base_dir("tenant_b", &dir).unwrap();

        // Write to p1.
        std::fs::write(p1.memory_dir().join("data.txt"), "tenant_a_data").unwrap();

        // Verify p2 cannot see it.
        let p2_data = p2.memory_dir().join("data.txt");
        assert!(!p2_data.exists());

        // Verify p1 path != p2 path.
        assert_ne!(p1.path, p2.path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn profile_list() {
        let dir = temp_profile_dir();
        let _p1 = Profile::with_base_dir("listed_profile", &dir).unwrap();
        let profiles = Profile::list_in_dir(&dir).unwrap();
        assert!(profiles.contains(&"listed_profile".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
