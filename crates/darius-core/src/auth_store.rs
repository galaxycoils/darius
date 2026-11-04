//! Private per-provider credentials; behavior follows Hermes `hermes_cli/auth.py`.
//! The caller resolves DARIUS_HOME (default ~/.darius) and passes that directory.
//! JSON: {"version":1,"providers":{"nous":{...}}}. Provider objects are extensible:
//! `access_token`, `refresh_token`, `expires_at` (Unix seconds), `scope`,
//! `token_type`, `quarantined` (bool), `quarantined_at` and `quarantine_reason`.
//! Quarantine enforcement and OAuth refresh belong to the credential resolver.
#[path = "auth_store/atomic.rs"]
mod atomic;
#[path = "auth_store/disk.rs"]
mod disk;
#[path = "auth_store/lock.rs"]
mod lock;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[path = "auth_store/error.rs"]
mod error;
pub use error::AuthStoreError;

#[derive(Clone)]
pub struct AuthStore {
    home: PathBuf,
}
impl AuthStore {
    pub fn new(home: impl AsRef<Path>) -> Self {
        Self {
            home: home.as_ref().to_owned(),
        }
    }
    pub fn provider(&self, provider: &str) -> Result<Option<Value>, AuthStoreError> {
        let _guard = lock::acquire(&self.home)?;
        Ok(disk::load(&self.home.join("auth.json"))?["providers"]
            .get(provider)
            .cloned())
    }
    pub fn set_provider(&self, provider: &str, state: Value) -> Result<(), AuthStoreError> {
        self.update_provider(provider, |_| Ok(Some(state)))
    }
    pub fn remove_provider(&self, provider: &str) -> Result<(), AuthStoreError> {
        self.update_provider(provider, |_| Ok(None))
    }
    /// Locked read-modify-write. Return None to remove; Err leaves disk unchanged.
    /// Callback must not reenter this store; the advisory lock covers its entire run.
    pub fn update_provider<F>(&self, provider: &str, update: F) -> Result<(), AuthStoreError>
    where
        F: FnOnce(Option<Value>) -> Result<Option<Value>, AuthStoreError>,
    {
        let _guard = lock::acquire(&self.home)?;
        disk::update(&self.home.join("auth.json"), provider, update)
    }
}
#[cfg(all(test, unix))]
#[path = "auth_store/contention_tests.rs"]
mod contention_tests;
#[cfg(test)]
#[path = "auth_store/tests.rs"]
mod tests;
#[cfg(all(test, unix))]
#[path = "auth_store/unix_tests.rs"]
mod unix_tests;
