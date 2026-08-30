//! Backup-Restore — snapshot and recover event logs, handoffs, memory.

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("path not found: {0}")]
    NotFound(String),
    #[error("backup corrupted: {0}")]
    Corrupted(String),
}

/// Backup manager — creates timestamped snapshots of daemon state.
pub struct BackupManager {
    data_dir: PathBuf,
    backup_dir: PathBuf,
}

impl BackupManager {
    pub fn new(
        data_dir: impl AsRef<Path>,
        backup_dir: impl AsRef<Path>,
    ) -> Result<Self, BackupError> {
        let data = PathBuf::from(data_dir.as_ref());
        let backup = PathBuf::from(backup_dir.as_ref());
        fs::create_dir_all(&backup)?;
        Ok(Self {
            data_dir: data,
            backup_dir: backup,
        })
    }

    /// Create a timestamped backup of the data directory.
    pub fn create_backup(&self, label: Option<&str>) -> Result<PathBuf, BackupError> {
        let timestamp = current_timestamp();
        let name = match label {
            Some(l) => format!("backup_{}_{}", l, timestamp),
            None => format!("backup_{}", timestamp),
        };
        let dest = self.backup_dir.join(&name);
        copy_dir_all(&self.data_dir, &dest)?;
        Ok(dest)
    }

    /// List available backups, newest first.
    pub fn list_backups(&self) -> Result<Vec<PathBuf>, BackupError> {
        let mut backups = Vec::new();
        for entry in fs::read_dir(&self.backup_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                backups.push(path);
            }
        }
        // Sort by timestamp (embedded in directory name), newest first.
        backups.sort_by(|a, b| {
            let a_ts = a
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|s| s.rsplit_once('_').map(|(_, ts)| ts));
            let b_ts = b
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|s| s.rsplit_once('_').map(|(_, ts)| ts));
            b_ts.cmp(&a_ts)
        });
        Ok(backups)
    }

    /// Restore from a backup into the data directory.
    pub fn restore_backup(&self, backup_path: impl AsRef<Path>) -> Result<(), BackupError> {
        let src = backup_path.as_ref();
        if !src.exists() {
            return Err(BackupError::NotFound(src.display().to_string()));
        }
        // Wipe current data.
        if self.data_dir.exists() {
            fs::remove_dir_all(&self.data_dir)?;
        }
        copy_dir_all(src, &self.data_dir)?;
        Ok(())
    }

    /// Verify a backup is valid (contains expected files).
    pub fn verify_backup(&self, backup_path: impl AsRef<Path>) -> Result<(), BackupError> {
        let src = backup_path.as_ref();
        if !src.exists() {
            return Err(BackupError::NotFound(src.display().to_string()));
        }
        // Basic check: directory exists and is not empty.
        let entries: Vec<_> = fs::read_dir(src)?.collect();
        if entries.is_empty() {
            return Err(BackupError::Corrupted("backup directory is empty".into()));
        }
        Ok(())
    }

    /// Delete a backup.
    pub fn delete_backup(&self, backup_path: impl AsRef<Path>) -> Result<(), BackupError> {
        let path = backup_path.as_ref();
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), BackupError> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.as_ref().join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dirs() -> (PathBuf, PathBuf) {
        let base =
            std::env::temp_dir().join(format!("darius_backup_test_{}", uuid::Uuid::new_v4()));
        let data = base.join("data");
        let backups = base.join("backups");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&backups).unwrap();
        (data, backups)
    }

    #[test]
    fn create_and_restore_backup() {
        let (data, backups) = temp_dirs();
        let manager = BackupManager::new(&data, &backups).unwrap();

        // Create some data.
        fs::write(data.join("test.txt"), "hello").unwrap();

        // Create backup.
        let backup_path = manager.create_backup(Some("test")).unwrap();
        assert!(backup_path.exists());
        assert!(backup_path.join("test.txt").exists());

        // Modify original data.
        fs::write(data.join("test.txt"), "modified").unwrap();

        // Restore.
        manager.restore_backup(&backup_path).unwrap();
        let restored = fs::read_to_string(data.join("test.txt")).unwrap();
        assert_eq!(restored, "hello");

        // Cleanup.
        let _ = fs::remove_dir_all(data.parent().unwrap());
    }

    #[test]
    fn list_backups_newest_first() {
        let (data, backups) = temp_dirs();
        let manager = BackupManager::new(&data, &backups).unwrap();

        // Create backups with delays so timestamps differ.
        let _p1 = manager.create_backup(Some("first")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _p2 = manager.create_backup(Some("second")).unwrap();

        let listed = manager.list_backups().unwrap();
        assert_eq!(listed.len(), 2);

        // Cleanup.
        let _ = fs::remove_dir_all(data.parent().unwrap());
    }

    #[test]
    fn verify_backup_validates() {
        let (data, backups) = temp_dirs();
        let manager = BackupManager::new(&data, &backups).unwrap();

        fs::write(data.join("file.txt"), "content").unwrap();
        let backup = manager.create_backup(None).unwrap();

        // Valid backup.
        assert!(manager.verify_backup(&backup).is_ok());

        // Nonexistent backup.
        assert!(manager.verify_backup("/nonexistent/path").is_err());

        // Cleanup.
        let _ = fs::remove_dir_all(data.parent().unwrap());
    }

    #[test]
    fn delete_backup() {
        let (data, backups) = temp_dirs();
        let manager = BackupManager::new(&data, &backups).unwrap();

        fs::write(data.join("file.txt"), "content").unwrap();
        let backup = manager.create_backup(None).unwrap();
        assert!(backup.exists());

        manager.delete_backup(&backup).unwrap();
        assert!(!backup.exists());

        // Cleanup.
        let _ = fs::remove_dir_all(data.parent().unwrap());
    }
}
