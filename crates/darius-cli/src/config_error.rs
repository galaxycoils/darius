use crate::paths::PathError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid profile path: {0}")]
    Path(#[from] PathError),
    #[error("could not read profile configuration")]
    Read(#[source] std::io::Error),
    #[error("profile configuration contains invalid TOML")]
    InvalidToml { path: PathBuf },
    #[error("provider must not be empty")]
    EmptyProvider,
    #[error("model must not be empty")]
    EmptyModel,
    #[error("base URL must use http or https")]
    InvalidUrlScheme,
    #[error("API key environment variable name is invalid")]
    InvalidApiKeyEnvironment,
    #[error("profile configuration already exists")]
    AlreadyExists { path: PathBuf },
    #[error("could not write profile configuration")]
    Write(#[source] std::io::Error),
    #[error("could not serialize profile configuration")]
    Serialize,
}
