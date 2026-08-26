//! Darius core types, IDs, and shared utilities.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubagentId {
    pub inner: uuid::Uuid,
}

impl SubagentId {
    pub fn new() -> Self {
        Self { inner: uuid::Uuid::new_v4() }
    }
}

impl Default for SubagentId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error)]
pub enum DariusError {
    #[error("not implemented")]
    NotImplemented,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}
