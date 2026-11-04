//! Nous Portal OAuth device-code service.
//!
//! Ported from NousResearch/hermes-agent @ `a6102b8d809d6b32824757b547b60f9d22d6a88e`
//! — `hermes_cli/auth_nous.py`, `hermes_cli/auth_constants.py`.
//! Engine only; CLI and request transport wired by the parent.
//!
//! # Public surface
//! - `NousAuthService`: device-code login (start/poll), status, logout, and a
//!   single-flight, refresh-aware token resolver.
//! - `NousCred`: the credential shape stored in `~/.darius/auth.json`.

use darius_core::auth_store::{AuthStore, AuthStoreError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const DEFAULT_PORTAL_BASE: &str = "https://portal.nousresearch.com";
const DEFAULT_INFERENCE_BASE: &str = "https://inference-api.nousresearch.com/v1";
const DEFAULT_SCOPE: &str = "inference:invoke";
#[allow(dead_code)]
const DEVICE_CODE_URL: &str = "/device/code";
const TOKEN_URL: &str = "/oauth/token";

#[derive(Debug, thiserror::Error)]
pub enum NousAuthError {
    #[error("auth store I/O failed")]
    Store(#[from] AuthStoreError),
    #[error("nous credential not found")]
    NotFound,
    #[error("nous token quarantined")]
    Quarantined,
    #[error("no refresh token")]
    NoRefreshToken,
    #[error("refresh token revoked")]
    Revoked,
    #[error("refresh request failed")]
    Network(#[from] reqwest::Error),
    #[error("refresh response invalid")]
    BadResponse,
    #[error("device flow failed: {0}")]
    DeviceFlow(String),
    #[error("device flow timed out")]
    DeviceTimeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NousCred {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub scope: Option<String>,
    pub token_type: Option<String>,
    pub quarantined: bool,
    pub quarantined_at: Option<i64>,
    pub quarantine_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NousAuthConfig {
    pub portal_base: String,
    pub inference_base: String,
    pub scope: String,
    pub client_id: String,
}

impl Default for NousAuthConfig {
    fn default() -> Self {
        Self {
            portal_base: std::env::var("NOUS_PORTAL_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_PORTAL_BASE.to_string()),
            inference_base: std::env::var("NOUS_INFERENCE_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_INFERENCE_BASE.to_string()),
            scope: DEFAULT_SCOPE.to_string(),
            client_id: "darius".to_string(),
        }
    }
}

pub struct NousAuthService {
    config: NousAuthConfig,
    store: Arc<AuthStore>,
    /// Single-flight guard: last successful refresh time; concurrent
    /// requests share one refresh.
    refresh_lock: Arc<Mutex<Option<Instant>>>,
}

impl NousAuthService {
    pub fn new(home: impl AsRef<std::path::Path>) -> Result<Self, AuthStoreError> {
        Ok(Self {
            config: NousAuthConfig::default(),
            store: Arc::new(AuthStore::new(home)),
            refresh_lock: Arc::new(Mutex::new(None)),
        })
    }

    pub fn with_config(mut self, config: NousAuthConfig) -> Self {
        self.config = config;
        self
    }

    pub fn config(&self) -> &NousAuthConfig {
        &self.config
    }

    fn http(&self) -> Result<reqwest::Client, NousAuthError> {
        Ok(reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?)
    }

    pub fn store(&self) -> &AuthStore {
        &self.store
    }

    pub fn load(&self) -> Result<Option<NousCred>, NousAuthError> {
        let raw = self.store.provider("nous")?;
        Ok(raw.and_then(|v| serde_json::from_value::<NousCred>(v).ok()))
    }

    pub fn save(&self, cred: &NousCred) -> Result<(), NousAuthError> {
        let v = serde_json::to_value(cred).map_err(|_| NousAuthError::BadResponse)?;
        self.store.set_provider("nous", v)?;
        Ok(())
    }

    pub fn delete(&self) -> Result<(), NousAuthError> {
        self.store.remove_provider("nous")?;
        Ok(())
    }

    pub fn status(&self) -> Result<Option<AuthStatus>, NousAuthError> {
        let cred = self.load()?;
        Ok(cred.map(|c| AuthStatus {
            provider: "nous".into(),
            token_type: c.token_type.unwrap_or_else(|| "opaque".into()),
            expires_at: c.expires_at,
            quarantined: c.quarantined,
        }))
    }

    /// Resolve the bearer token for the current credential.
    /// On 401, callers should re-invoke this after a forced refresh.
    pub async fn resolve_token(&self) -> Result<String, NousAuthError> {
        let cred = self.load()?;
        let Some(cred) = cred else {
            return Err(NousAuthError::NotFound);
        };
        if cred.quarantined {
            return Err(NousAuthError::Quarantined);
        }
        if let Some(exp) = cred.expires_at {
            let now = chrono::Utc::now().timestamp();
            if now + 120 >= exp {
                return self.refresh().await;
            }
        }
        Ok(cred.access_token)
    }

    /// Single-flight refresh: concurrent requests share one refresh.
    pub async fn refresh(&self) -> Result<String, NousAuthError> {
        let lock = self.refresh_lock.clone();
        let mut guard = lock.lock().await;
        // Re-check after acquiring lock.
        if let Some(t) = guard.as_ref()
            && t.elapsed() < Duration::from_secs(5)
        {
            // Another request just refreshed; re-read directly
            // (no resolve_token call: breaks async recursion).
            drop(guard);
            let cred = self.load()?;
            return cred.map(|c| c.access_token).ok_or(NousAuthError::NotFound);
        }
        let cred = self.load()?;
        let Some(mut cred) = cred else {
            return Err(NousAuthError::NotFound);
        };
        let refresh_token = cred
            .refresh_token
            .clone()
            .ok_or(NousAuthError::NoRefreshToken)?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        let body = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": self.config.client_id,
        });
        let resp = client
            .post(format!("{}{}", self.config.inference_base, TOKEN_URL))
            .json(&body)
            .send()
            .await?;
        if resp.status().as_u16() == 400 || resp.status().as_u16() == 401 {
            cred.quarantined = true;
            cred.quarantined_at = Some(chrono::Utc::now().timestamp());
            cred.quarantine_reason = Some("refresh rejected".into());
            let _ = self.save(&cred);
            return Err(NousAuthError::Revoked);
        }
        let json: serde_json::Value = resp.json().await.map_err(|_| NousAuthError::BadResponse)?;
        cred.access_token = json["access_token"]
            .as_str()
            .ok_or(NousAuthError::BadResponse)?
            .to_string();
        cred.refresh_token = json["refresh_token"].as_str().map(str::to_owned);
        cred.expires_at = json["expires_in"]
            .as_u64()
            .map(|secs| chrono::Utc::now().timestamp() + secs as i64);
        cred.token_type = json["token_type"].as_str().map(str::to_owned);
        cred.scope = json["scope"].as_str().map(str::to_owned);
        cred.quarantined = false;
        cred.quarantined_at = None;
        cred.quarantine_reason = None;
        self.save(&cred)?;
        *guard = Some(Instant::now());
        Ok(cred.access_token)
    }

    /// Device-code login, step 1: request a user code from the Portal.
    /// Prints nothing; the caller shows `verification_uri` + `user_code`.
    pub async fn start_device_code(&self) -> Result<DeviceCode, NousAuthError> {
        let body = serde_json::json!({
            "client_id": self.config.client_id,
            "scope": self.config.scope,
        });
        let resp = self
            .http()?
            .post(format!("{}{}", self.config.portal_base, DEVICE_CODE_URL))
            .json(&body)
            .send()
            .await?;
        let json: serde_json::Value = resp.json().await.map_err(|_| NousAuthError::BadResponse)?;
        Ok(DeviceCode {
            device_code: json["device_code"]
                .as_str()
                .ok_or(NousAuthError::BadResponse)?
                .to_string(),
            user_code: json["user_code"]
                .as_str()
                .ok_or(NousAuthError::BadResponse)?
                .to_string(),
            verification_uri: json["verification_uri"]
                .as_str()
                .ok_or(NousAuthError::BadResponse)?
                .to_string(),
            expires_in: json["expires_in"].as_u64().unwrap_or(600),
            interval: json["interval"].as_u64().unwrap_or(5).clamp(1, 30),
        })
    }

    /// Device-code login, step 2: poll until the user approves (or times out).
    /// On success the credential is stored and returned.
    pub async fn poll_device_token(&self, started: &DeviceCode) -> Result<NousCred, NousAuthError> {
        let deadline = Instant::now() + Duration::from_secs(started.expires_in.min(600));
        loop {
            if Instant::now() >= deadline {
                return Err(NousAuthError::DeviceTimeout);
            }
            tokio::time::sleep(Duration::from_secs(started.interval)).await;
            let body = serde_json::json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": started.device_code,
                "client_id": self.config.client_id,
            });
            let resp = self
                .http()?
                .post(format!("{}{}", self.config.portal_base, TOKEN_URL))
                .json(&body)
                .send()
                .await?;
            let json: serde_json::Value =
                resp.json().await.map_err(|_| NousAuthError::BadResponse)?;
            if json.get("access_token").and_then(|v| v.as_str()).is_some() {
                let cred = NousCred {
                    access_token: json["access_token"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    refresh_token: json["refresh_token"].as_str().map(str::to_owned),
                    expires_at: json["expires_in"]
                        .as_u64()
                        .map(|s| chrono::Utc::now().timestamp() + s as i64),
                    scope: json["scope"].as_str().map(str::to_owned),
                    token_type: json["token_type"].as_str().map(str::to_owned),
                    quarantined: false,
                    quarantined_at: None,
                    quarantine_reason: None,
                };
                self.save(&cred)?;
                return Ok(cred);
            }
            match json.get("error").and_then(|v| v.as_str()) {
                Some("authorization_pending") | Some("slow_down") | None => continue,
                Some(other) => return Err(NousAuthError::DeviceFlow(other.to_string())),
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub provider: String,
    pub token_type: String,
    pub expires_at: Option<i64>,
    pub quarantined: bool,
}

/// Pending device-code authorization (Portal response, step 1 of login).
#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}
