//! Credential resolution order for provider profiles.
//!
//! Order: process environment (which already includes `.env`-loaded values —
//! `runtime_selector::load_dotenv_if_present` merges workspace `.env` then
//! home `.env` into the process without overwriting, so process values
//! structurally win) → `auth.json` provider state (Nous/OAuth only) →
//! explicit error naming the variable and the file paths. Keyless profiles
//! skip every lookup.
//!
//! Pure function over injected inputs so the order is unit-testable without
//! touching the real environment or disk; the CLI loads `auth.json` via
//! `AuthStore` and passes the provider object in.

use crate::providers::AuthType;
use std::path::Path;

/// Where a resolved credential came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// Process environment (direct export or merged from a `.env` file).
    Environment,
    /// `auth.json` provider state in the Darius home directory.
    AuthStore,
}

/// A resolved credential for one provider profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedCredential {
    /// Keyless profile: no credential needed or consulted.
    Keyless,
    /// API key from [`CredentialSource::Environment`].
    ApiKey {
        value: String,
        source: CredentialSource,
    },
    /// OAuth bearer token from [`CredentialSource::AuthStore`].
    OAuthToken {
        token: String,
        source: CredentialSource,
    },
}

/// Resolution failure. The message always names the expected variable and the
/// candidate file paths (`<home>/.env`, `<workspace>/.env`, `<home>/auth.json`)
/// so users know exactly where to put the credential.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CredentialError(pub String);

/// Inputs for [`resolve_credential`].
pub struct CredentialRequest<'a> {
    /// Provider id for messages (e.g. `"nous"`, `"openrouter"`).
    pub provider: &'a str,
    /// Key variable (e.g. `Some("OPENROUTER_API_KEY")`); `None` or `"NONE"`
    /// means keyless.
    pub key_env: Option<&'a str>,
    /// Profile auth type; [`AuthType::Keyless`] skips all lookups and
    /// [`AuthType::NousOAuth`] enables the `auth.json` step.
    pub auth_type: AuthType,
    /// Effective environment lookup (process env after `.env` merge).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// Pre-loaded `auth.json` provider object (`AuthStore::provider`), if any.
    pub auth_state: Option<serde_json::Value>,
    /// Resolved Darius home dir (already honors `DARIUS_HOME`); used for paths
    /// in error messages.
    pub home: &'a Path,
    /// Workspace dir; used for paths in error messages.
    pub workspace: &'a Path,
}

fn is_keyless(auth_type: AuthType, key_env: Option<&str>) -> bool {
    matches!(auth_type, AuthType::Keyless)
        || key_env.is_none_or(|v| v.eq_ignore_ascii_case("NONE") || v.trim().is_empty())
}

fn nonblank(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

/// Stored OAuth state is usable when it carries a non-blank `access_token`,
/// is not quarantined, and has no past `expires_at` (Unix seconds).
fn usable_oauth_token(state: &serde_json::Value) -> Option<String> {
    let obj = state.as_object()?;
    if obj
        .get("quarantined")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    if let Some(exp) = obj.get("expires_at").and_then(|v| v.as_i64()) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(i64::MAX);
        if exp <= now {
            return None;
        }
    }
    obj.get("access_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_owned())
}

/// Resolve one profile's credential in order:
/// key-env in (dotenv-merged) process env → `auth.json` (Nous/OAuth) → error.
/// Keyless profiles return [`ResolvedCredential::Keyless`] without consulting
/// any source.
pub fn resolve_credential(
    req: &CredentialRequest<'_>,
) -> Result<ResolvedCredential, CredentialError> {
    if is_keyless(req.auth_type, req.key_env) {
        return Ok(ResolvedCredential::Keyless);
    }
    let key_env = req.key_env.unwrap_or_default();
    if let Some(value) = nonblank((req.env)(key_env)) {
        return Ok(ResolvedCredential::ApiKey {
            value,
            source: CredentialSource::Environment,
        });
    }
    if matches!(req.auth_type, AuthType::NousOAuth) {
        let token = req.auth_state.as_ref().and_then(usable_oauth_token);
        if let Some(token) = token {
            return Ok(ResolvedCredential::OAuthToken {
                token,
                source: CredentialSource::AuthStore,
            });
        }
    }
    let state_hint = match req.auth_state.as_ref() {
        Some(state) if state.is_object() => {
            if state
                .get("quarantined")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                let reason = state
                    .get("quarantine_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no reason recorded");
                format!(" stored OAuth state is quarantined ({reason}); re-authenticate.")
            } else {
                " stored OAuth state is missing a usable access_token; re-authenticate.".to_owned()
            }
        }
        _ => String::new(),
    };
    Err(CredentialError(format!(
        "missing credential for provider '{provider}': set {key_env} in the process environment, {home}/.env, or {workspace}/.env; OAuth state lives at {home}/auth.json.{state_hint}",
        provider = req.provider,
        home = req.home.display(),
        workspace = req.workspace.display(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Case {
        name: &'static str,
        auth_type: AuthType,
        key_env: Option<&'static str>,
        env_val: Option<&'static str>,
        auth_state: Option<serde_json::Value>,
    }

    /// Table-driven resolution-order test: keyless skips all sources, env
    /// beats auth.json, auth.json covers Nous/OAuth, and failures name the
    /// variable plus every candidate path.
    #[test]
    fn credential_resolution_order() {
        let home = std::path::PathBuf::from("/tmp/darius-wu03-home");
        let workspace = std::path::PathBuf::from("/tmp/darius-wu03-work");
        let live_token = || json!({"access_token": "oauth-token", "expires_at": 2_000_000_000i64, "quarantined": false});
        let cases = [
            Case {
                name: "keyless skips env and auth store",
                auth_type: AuthType::Keyless,
                key_env: None,
                env_val: Some("ignored"),
                auth_state: Some(live_token()),
            },
            Case {
                name: "NONE sentinel skips env and auth store",
                auth_type: AuthType::ApiKey,
                key_env: Some("NONE"),
                env_val: Some("ignored"),
                auth_state: Some(live_token()),
            },
            Case {
                name: "env beats auth store",
                auth_type: AuthType::NousOAuth,
                key_env: Some("NOUS_API_KEY"),
                env_val: Some("env-key"),
                auth_state: Some(live_token()),
            },
            Case {
                name: "auth store covers nous when env is absent",
                auth_type: AuthType::NousOAuth,
                key_env: Some("NOUS_API_KEY"),
                env_val: None,
                auth_state: Some(live_token()),
            },
            Case {
                name: "blank env falls through to auth store",
                auth_type: AuthType::NousOAuth,
                key_env: Some("NOUS_API_KEY"),
                env_val: Some("   "),
                auth_state: Some(live_token()),
            },
            Case {
                name: "api-key profiles ignore auth store",
                auth_type: AuthType::ApiKey,
                key_env: Some("OPENROUTER_API_KEY"),
                env_val: None,
                auth_state: Some(live_token()),
            },
            Case {
                name: "quarantined token is unusable",
                auth_type: AuthType::NousOAuth,
                key_env: Some("NOUS_API_KEY"),
                env_val: None,
                auth_state: Some(
                    json!({"access_token": "bad", "quarantined": true, "quarantine_reason": "rotated"}),
                ),
            },
            Case {
                name: "expired token is unusable",
                auth_type: AuthType::NousOAuth,
                key_env: Some("NOUS_API_KEY"),
                env_val: None,
                auth_state: Some(json!({"access_token": "stale", "expires_at": 1i64})),
            },
            Case {
                name: "nothing anywhere is an error",
                auth_type: AuthType::NousOAuth,
                key_env: Some("NOUS_API_KEY"),
                env_val: None,
                auth_state: None,
            },
        ];
        for case in &cases {
            let lookup = |name: &str| {
                (name == case.key_env.unwrap_or_default())
                    .then(|| case.env_val.map(str::to_owned))
                    .flatten()
            };
            let req = CredentialRequest {
                provider: "nous",
                key_env: case.key_env,
                auth_type: case.auth_type,
                env: &lookup,
                auth_state: case.auth_state.clone(),
                home: &home,
                workspace: &workspace,
            };
            let result = resolve_credential(&req);
            match case.name {
                "keyless skips env and auth store" | "NONE sentinel skips env and auth store" => {
                    assert_eq!(
                        result.unwrap(),
                        ResolvedCredential::Keyless,
                        "{}",
                        case.name
                    );
                }
                "env beats auth store" => assert_eq!(
                    result.unwrap(),
                    ResolvedCredential::ApiKey {
                        value: "env-key".into(),
                        source: CredentialSource::Environment,
                    },
                    "{}",
                    case.name
                ),
                "auth store covers nous when env is absent"
                | "blank env falls through to auth store" => assert_eq!(
                    result.unwrap(),
                    ResolvedCredential::OAuthToken {
                        token: "oauth-token".into(),
                        source: CredentialSource::AuthStore,
                    },
                    "{}",
                    case.name
                ),
                "api-key profiles ignore auth store"
                | "quarantined token is unusable"
                | "expired token is unusable"
                | "nothing anywhere is an error" => {
                    let err = result.unwrap_err();
                    assert!(
                        err.0.contains("NOUS_API_KEY") || err.0.contains("OPENROUTER_API_KEY"),
                        "{}: error must name the variable, got: {err}",
                        case.name
                    );
                    for path in [
                        "/tmp/darius-wu03-home/.env",
                        "/tmp/darius-wu03-work/.env",
                        "/tmp/darius-wu03-home/auth.json",
                    ] {
                        assert!(
                            err.0.contains(path),
                            "{}: error must name {path}, got: {err}",
                            case.name
                        );
                    }
                }
                _ => unreachable!(),
            }
        }
        // Quarantine reason surfaces in the message.
        let lookup = |_: &str| None;
        let req = CredentialRequest {
            provider: "nous",
            key_env: Some("NOUS_API_KEY"),
            auth_type: AuthType::NousOAuth,
            env: &lookup,
            auth_state: Some(
                json!({"access_token": "bad", "quarantined": true, "quarantine_reason": "rotated"}),
            ),
            home: &home,
            workspace: &workspace,
        };
        assert!(
            resolve_credential(&req).unwrap_err().0.contains("rotated"),
            "quarantine reason must surface in the error"
        );
    }
}
