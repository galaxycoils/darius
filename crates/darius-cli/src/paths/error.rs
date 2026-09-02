use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("could not determine user home directory")]
    HomeUnavailable,
    #[error("Darius home is not an existing directory: {0}")]
    InvalidHome(PathBuf),
    #[error("workspace is not an existing directory: {0}")]
    InvalidWorkspace(PathBuf),
    #[error("invalid profile name: {0}")]
    InvalidProfile(String),
    #[error("path I/O error: {0}")]
    Io(#[from] std::io::Error),
}
