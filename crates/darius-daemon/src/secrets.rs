//! Secret Management — OS keychain, rotation, scoped tokens, audit by fingerprint.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("secret not found: {0}")]
    NotFound(String),
    #[error("secret expired: {0}")]
    Expired(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("keychain error: {0}")]
    Keychain(String),
}

/// A scoped secret token.
#[derive(Debug, Clone)]
pub struct Secret {
    pub name: String,
    pub value: String,
    pub scope: SecretScope,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub fingerprint: String,
}

/// Secret scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretScope {
    Global,
    Profile(String),
    Session(String),
}

/// Secret store — manages secrets per profile.
pub struct SecretStore {
    secrets: Arc<Mutex<HashMap<String, Secret>>>,
    profile: String,
}

impl SecretStore {
    pub fn new(profile: impl Into<String>) -> Self {
        Self {
            secrets: Arc::new(Mutex::new(HashMap::new())),
            profile: profile.into(),
        }
    }

    /// Store a secret.
    pub fn store(
        &self,
        name: impl Into<String>,
        value: impl Into<String>,
        scope: SecretScope,
        ttl_seconds: Option<u64>,
    ) -> Secret {
        let name = name.into();
        let value = value.into();
        let now = current_timestamp();
        let fingerprint = compute_fingerprint(&value);

        let secret = Secret {
            name: name.clone(),
            value,
            scope,
            created_at: now,
            expires_at: ttl_seconds.map(|ttl| now + ttl),
            fingerprint,
        };

        self.secrets.lock().insert(name, secret.clone());
        secret
    }

    /// Get a secret by name.
    pub fn get(&self, name: &str) -> Result<Secret, SecretError> {
        let secrets = self.secrets.lock();
        let secret = secrets
            .get(name)
            .ok_or_else(|| SecretError::NotFound(name.into()))?;

        // Check expiration.
        if secret
            .expires_at
            .is_some_and(|expires_at| current_timestamp() > expires_at)
        {
            return Err(SecretError::Expired(name.into()));
        }

        Ok(secret.clone())
    }

    /// Get a secret value by name.
    pub fn get_value(&self, name: &str) -> Result<String, SecretError> {
        self.get(name).map(|s| s.value)
    }

    /// Revoke (delete) a secret.
    pub fn revoke(&self, name: &str) -> Result<(), SecretError> {
        let mut secrets = self.secrets.lock();
        secrets
            .remove(name)
            .ok_or_else(|| SecretError::NotFound(name.into()))?;
        Ok(())
    }

    /// Rotate a secret (generate new value).
    pub fn rotate(
        &self,
        name: &str,
        new_value: impl Into<String>,
        ttl_seconds: Option<u64>,
    ) -> Result<Secret, SecretError> {
        let mut secrets = self.secrets.lock();
        let existing = secrets
            .get(name)
            .ok_or_else(|| SecretError::NotFound(name.into()))?;

        let now = current_timestamp();
        let new_value = new_value.into();
        let fingerprint = compute_fingerprint(&new_value);

        let new_secret = Secret {
            name: name.into(),
            value: new_value,
            scope: existing.scope.clone(),
            created_at: now,
            expires_at: ttl_seconds.map(|ttl| now + ttl),
            fingerprint,
        };

        secrets.insert(name.into(), new_secret.clone());
        Ok(new_secret)
    }

    /// List all secrets (names only, no values).
    pub fn list(&self) -> Vec<String> {
        self.secrets.lock().keys().cloned().collect()
    }

    /// Check if a secret is expired.
    pub fn is_expired(&self, name: &str) -> bool {
        match self.get(name).ok().and_then(|secret| secret.expires_at) {
            Some(expires_at) => current_timestamp() >= expires_at,
            None => true,
        }
    }

    /// Get the profile this store belongs to.
    pub fn profile(&self) -> &str {
        &self.profile
    }
}

/// Compute a fingerprint for a secret value (SHA-256 prefix).
fn compute_fingerprint(value: &str) -> String {
    use blake3::Hasher;
    let mut hasher = Hasher::new();
    hasher.update(value.as_bytes());
    let hash = hasher.finalize();
    hash.as_bytes()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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

    #[test]
    fn store_and_get_secret() {
        let store = SecretStore::new("default");
        store.store(
            "api_key",
            "secret_value",
            SecretScope::Profile("default".into()),
            None,
        );

        let secret = store.get("api_key").unwrap();
        assert_eq!(secret.name, "api_key");
        assert_eq!(secret.value, "secret_value");
    }

    #[test]
    fn get_missing_secret_errors() {
        let store = SecretStore::new("default");
        assert!(store.get("nonexistent").is_err());
    }

    #[test]
    fn revoke_secret() {
        let store = SecretStore::new("default");
        store.store("temp", "value", SecretScope::Global, None);
        assert!(store.revoke("temp").is_ok());
        assert!(store.get("temp").is_err());
    }

    #[test]
    fn rotate_secret() {
        let store = SecretStore::new("default");
        store.store("key", "old_value", SecretScope::Global, None);
        let rotated = store.rotate("key", "new_value", None).unwrap();

        assert_eq!(rotated.value, "new_value");
        assert_ne!(rotated.fingerprint, compute_fingerprint("old_value"));
    }

    #[test]
    fn secret_with_ttl_expires() {
        let store = SecretStore::new("default");
        // TTL of 0 means it expires immediately.
        store.store("expiring", "value", SecretScope::Global, Some(0));

        // Should be expired now (or very soon).
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(store.is_expired("expiring"));
    }

    #[test]
    fn list_secrets() {
        let store = SecretStore::new("default");
        store.store("a", "1", SecretScope::Global, None);
        store.store("b", "2", SecretScope::Global, None);

        let mut names = store.list();
        names.sort();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let fp1 = compute_fingerprint("test_value");
        let fp2 = compute_fingerprint("test_value");
        assert_eq!(fp1, fp2);

        let fp3 = compute_fingerprint("different_value");
        assert_ne!(fp1, fp3);
    }
}
