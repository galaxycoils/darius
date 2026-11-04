//! Generic chat-completions backend; profile metadata drives request-time auth.
use super::wire;
use super::{BudgetEnforcer, BudgetScope};
use crate::auth::NousAuthService;
use crate::opencode_affinity::{is_opencode_relay, session_token};
use darius_cognitive::CognitiveError;
use darius_core::providers::{AuthType, ProviderProfile};

pub struct ChatCompletionsModel {
    pub profile: Option<&'static ProviderProfile>,
    pub model: String,
    pub base_url: String,
    pub key_env: String,
    pub client: reqwest::Client,
    pub budget: BudgetEnforcer,
    pub scope: BudgetScope,
    /// Explicit per-config output cap. `None` (absent) falls back to the
    /// profile default (3.5), then the transport default below.
    pub max_output_tokens: Option<u64>,
    /// OpenRouter `[openrouter]` routing prefs (3.6). Applied to the body
    /// ONLY when the active profile is openrouter; dropped at body-build
    /// for every other profile. `None` (default) sends no key anywhere.
    pub provider_routing: Option<wire::ProviderRouting>,
}

/// Transport default output cap when neither config nor profile sets one.
pub(crate) const DEFAULT_OUTPUT_TOKENS: u64 = 4096;

/// Resolved request-time authorization for one chat-completions call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthHeader {
    /// Keyless profile: explicit empty `Authorization` header (present, not absent).
    Empty,
    /// Bearer credential: API key, local fallback, or resolved Nous token.
    Bearer(String),
}

/// Resolved auth plus whether a 401 may trigger one OAuth refresh-and-retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAuth {
    pub header: AuthHeader,
    /// True only when the bearer came from the Nous OAuth store.
    pub nous_managed: bool,
}

fn auth_failed() -> CognitiveError {
    CognitiveError::Loop("authentication failed".into())
}

fn transport_error(error: reqwest::Error) -> CognitiveError {
    if error.is_timeout() {
        CognitiveError::Loop(
            "provider request timed out: check network connectivity, \
             then retry or try again"
                .into(),
        )
    } else {
        CognitiveError::Loop(
            "provider request failed: check network connectivity and base_url, \
             then retry or try again"
                .into(),
        )
    }
}

/// Darius home dir for the Nous OAuth store: `DARIUS_HOME` or `~/.darius`.
fn darius_home() -> std::path::PathBuf {
    match std::env::var("DARIUS_HOME") {
        Ok(home) if !home.trim().is_empty() => std::path::PathBuf::from(home),
        _ => dirs::home_dir()
            .map(|home| home.join(".darius"))
            .unwrap_or_else(|| std::path::PathBuf::from(".darius")),
    }
}

impl ChatCompletionsModel {
    /// Effective output cap for the budget reservation and `max_tokens` body
    /// field (3.5): explicit per-config value wins, else the profile default
    /// (e.g. NVIDIA 16384) when the config value is absent, else the
    /// transport default.
    pub fn output_budget(&self) -> u64 {
        self.max_output_tokens
            .or_else(|| {
                self.profile
                    .and_then(|profile| profile.default_max_tokens)
                    .map(u64::from)
            })
            .unwrap_or(DEFAULT_OUTPUT_TOKENS)
    }

    /// Keyless profiles skip every credential lookup and send an explicit
    /// empty `Authorization` header. `None` (legacy callers) never counts as
    /// keyless, preserving current Bearer behavior.
    pub fn is_keyless(&self) -> bool {
        match self.profile {
            None => false,
            Some(profile) => {
                profile.omit_authorization
                    || matches!(profile.auth_type, AuthType::Keyless)
                    || self.key_env.eq_ignore_ascii_case("NONE")
            }
        }
    }

    /// Nous profiles resolve their bearer from the OAuth store (or the
    /// configured key variable when set).
    pub fn is_nous(&self) -> bool {
        matches!(
            self.profile.map(|profile| profile.auth_type),
            Some(AuthType::NousOAuth)
        )
    }

    fn is_local(&self) -> bool {
        self.base_url.contains("localhost")
            || self.base_url.contains("127.0.0.1")
            || self.base_url.contains("0.0.0.0")
    }

    fn env_key(&self) -> Option<String> {
        match std::env::var(&self.key_env) {
            Ok(key) if !key.trim().is_empty() => Some(key),
            _ => None,
        }
    }

    /// Profile-driven `Authorization` resolution: keyless skips all lookups;
    /// Nous prefers the configured key variable, else the OAuth-store token
    /// (refresh-eligible on 401); api-key uses the variable with the
    /// local/ollama fallback; legacy `None` profiles keep Bearer behavior.
    pub async fn resolve_auth(&self) -> Result<ResolvedAuth, CognitiveError> {
        if self.is_keyless() {
            return Ok(ResolvedAuth {
                header: AuthHeader::Empty,
                nous_managed: false,
            });
        }
        if self.is_nous() {
            if let Some(key) = self.env_key() {
                return Ok(ResolvedAuth {
                    header: AuthHeader::Bearer(key),
                    nous_managed: false,
                });
            }
            let service = NousAuthService::new(darius_home()).map_err(|_| auth_failed())?;
            let token = service.resolve_token().await.map_err(|_| auth_failed())?;
            return Ok(ResolvedAuth {
                header: AuthHeader::Bearer(token),
                nous_managed: true,
            });
        }
        match self.env_key() {
            Some(key) => Ok(ResolvedAuth {
                header: AuthHeader::Bearer(key),
                nous_managed: false,
            }),
            None if self.is_local() || self.key_env.eq_ignore_ascii_case("NONE") => {
                Ok(ResolvedAuth {
                    header: AuthHeader::Bearer("ollama".to_string()),
                    nous_managed: false,
                })
            }
            None => Err(auth_failed()),
        }
    }

    /// Apply a resolved [`AuthHeader`] to a request builder: keyless sends an
    /// explicit empty header, never an absent or `Bearer` one.
    pub fn apply_auth(
        builder: reqwest::RequestBuilder,
        header: &AuthHeader,
    ) -> reqwest::RequestBuilder {
        match header {
            AuthHeader::Empty => builder.header("authorization", ""),
            AuthHeader::Bearer(token) => builder.bearer_auth(token),
        }
    }

    /// Apply profile-driven extra headers: attribution (`request_headers`)
    /// only for profiles with non-empty `static_headers`; `x-opencode-session`
    /// only for OpenCode relay providers. `None` (legacy) sends neither.
    pub fn apply_extra_headers(
        builder: reqwest::RequestBuilder,
        profile: &Option<&'static ProviderProfile>,
        session_id: &str,
    ) -> reqwest::RequestBuilder {
        let Some(profile) = profile else {
            return builder;
        };
        let mut builder = builder;
        if !profile.static_headers.is_empty() {
            for (key, value) in profile.request_headers() {
                builder = builder.header(key, value);
            }
        }
        if is_opencode_relay(profile) {
            builder = builder.header("x-opencode-session", session_token(session_id));
        }
        builder
    }

    /// POST one chat-completions body with profile-driven auth; Nous
    /// OAuth-store tokens get a single refresh-and-retry on 401.
    /// Shared by `complete` and `complete_stream` (streaming parity).
    /// Profile-gated body extras (3.6 routing, 3.7 Nous tags + sticky
    /// `session_id`) are applied here so both paths stay identical.
    pub async fn post_chat(
        &self,
        url: String,
        mut body: serde_json::Value,
        session_id: &str,
    ) -> Result<reqwest::Response, CognitiveError> {
        wire::apply_body_extras(
            self.profile,
            &mut body,
            self.provider_routing.as_ref(),
            session_id,
        );
        let auth = self.resolve_auth().await?;
        let first = Self::apply_extra_headers(
            Self::apply_auth(self.client.post(url.clone()), &auth.header),
            &self.profile,
            session_id,
        )
        .json(&body)
        .send()
        .await
        .map_err(transport_error)?;
        if first.status().as_u16() == 401 && auth.nous_managed {
            let service = NousAuthService::new(darius_home()).map_err(|_| auth_failed())?;
            let fresh = service.refresh().await.map_err(|_| auth_failed())?;
            let retry = Self::apply_extra_headers(
                Self::apply_auth(self.client.post(url), &AuthHeader::Bearer(fresh)),
                &self.profile,
                session_id,
            )
            .json(&body)
            .send()
            .await
            .map_err(transport_error)?;
            return Ok(retry);
        }
        Ok(first)
    }
}
