/// Errors deliberately exclude paths, input, OS messages, and credentials.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthStoreError {
    #[error("auth store I/O failed")]
    Io,
    #[error("invalid auth store state")]
    InvalidState,
    #[error("timed out waiting for auth store lock")]
    LockTimeout,
    #[error("private auth storage is unsupported on this platform")]
    UnsupportedPlatform,
}
