//! Profile Isolation — multi-tenant data separation.

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("profile not found: {0}")]
    NotFound(String),
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
        let name = name.into();
        let path = Self::base_dir().join(&name);
        std::fs::create_dir_all(&path)?;
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
        let base = Self::base_dir();
        if !base.exists() {
            return Ok(Vec::new());
        }
        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(&base)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    profiles.push(name.to_string());
                }
            }
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
        let dir = std::env::temp_dir().join(format!("darius_profile_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn with_temp_dir<F: FnOnce()>(f: F) {
        let dir = temp_profile_dir();
        unsafe {
            std::env::set_var("DARIUS_PROFILE_DIR", &dir);
        }
        f();
        unsafe {
            std::env::remove_var("DARIUS_PROFILE_DIR");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    fn profile_creates_isolated_directories() {
        with_temp_dir(|| {
            let profile = Profile::new("test_profile").unwrap();
            assert!(profile.path.exists());
            assert!(profile.memory_dir().exists());
            assert!(profile.skills_dir().exists());
            assert!(profile.sessions_dir().exists());
            assert!(profile.db_path().parent().unwrap().exists());
        });
    }

    #[test]
    fn two_profiles_do_not_share_data() {
        with_temp_dir(|| {
            let p1 = Profile::new("tenant_a").unwrap();
            let p2 = Profile::new("tenant_b").unwrap();

            // Write to p1.
            std::fs::write(p1.memory_dir().join("data.txt"), "tenant_a_data").unwrap();

            // Verify p2 cannot see it.
            let p2_data = p2.memory_dir().join("data.txt");
            assert!(!p2_data.exists());

            // Verify p1 path != p2 path.
            assert_ne!(p1.path, p2.path);
        });
    }

    #[test]
    fn profile_list() {
        with_temp_dir(|| {
            let _p1 = Profile::new("listed_profile").unwrap();
            let profiles = Profile::list().unwrap();
            assert!(profiles.contains(&"listed_profile".to_string()));
        });
    }
}
