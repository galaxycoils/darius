//! Filesystem abstraction for hashline edits.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilesystemError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub trait Filesystem {
    fn read(&mut self, path: &str) -> Result<String, FilesystemError>;
    fn write(&mut self, path: &str, content: &str) -> Result<(), FilesystemError>;
    fn list_files(&self) -> Vec<PathBuf>;
    fn exists(&self, path: &str) -> bool;
}

#[derive(Debug, Default)]
pub struct InMemoryFilesystem {
    files: HashMap<PathBuf, String>,
}

impl InMemoryFilesystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file(path: &str, content: &str) -> Self {
        let mut fs = Self::new();
        fs.files.insert(PathBuf::from(path), content.to_string());
        fs
    }
}

impl Filesystem for InMemoryFilesystem {
    fn read(&mut self, path: &str) -> Result<String, FilesystemError> {
        self.files
            .get(&PathBuf::from(path))
            .cloned()
            .ok_or_else(|| FilesystemError::NotFound(path.into()))
    }

    fn write(&mut self, path: &str, content: &str) -> Result<(), FilesystemError> {
        self.files.insert(PathBuf::from(path), content.to_string());
        Ok(())
    }

    fn list_files(&self) -> Vec<PathBuf> {
        self.files.keys().cloned().collect()
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(&PathBuf::from(path))
    }
}

#[derive(Debug, Default)]
pub struct DiskFilesystem {
    root: Option<PathBuf>,
}

impl DiskFilesystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn in_directory(dir: &str) -> Self {
        Self {
            root: Some(PathBuf::from(dir)),
        }
    }
}

fn resolve_path(path: &str, root: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else if let Some(root) = root {
        root.join(&p)
    } else {
        p
    }
}

impl Filesystem for DiskFilesystem {
    fn read(&mut self, path: &str) -> Result<String, FilesystemError> {
        let target = resolve_path(path, self.root.as_deref());
        fs::read_to_string(&target).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                FilesystemError::NotFound(path.into())
            } else {
                FilesystemError::Io(e)
            }
        })
    }

    fn write(&mut self, path: &str, content: &str) -> Result<(), FilesystemError> {
        let target = resolve_path(path, self.root.as_deref());
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(FilesystemError::Io)?;
        }
        fs::write(&target, content).map_err(FilesystemError::Io)
    }

    fn list_files(&self) -> Vec<PathBuf> {
        let root = match &self.root {
            Some(r) => r.clone(),
            None => return Vec::new(),
        };
        walk_dir(&root)
    }

    fn exists(&self, path: &str) -> bool {
        let target = resolve_path(path, self.root.as_deref());
        target.exists()
    }
}

fn walk_dir(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.is_dir() {
        return out;
    }
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            } else if path.is_dir() {
                out.extend(walk_dir(&path));
            }
        }
    }
    out
}
