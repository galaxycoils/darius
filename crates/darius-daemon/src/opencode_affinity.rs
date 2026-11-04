//! OpenCode session affinity — `x-opencode-session` header derivation.
//! Port of Hermes `agent/opencode_affinity.py`.
//!
//! Scope: `opencode-go`, `opencode-free`, plus any custom profile whose
//! `base_url` is hosted on `opencode.ai`. The value pins the conversation to
//! one backend relay instance so prompt cache stays warm. Derived from the
//! Darius session ID (no personal data).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Derive a stable, opaque session token from the Darius session ID.
pub fn session_token(session_id: &str) -> String {
    let mut h = DefaultHasher::new();
    session_id.hash(&mut h);
    format!("{:x}", h.finish())
}

/// True when the profile is an OpenCode relay provider (opencode.ai host).
pub fn is_opencode_relay(profile: &darius_core::providers::ProviderProfile) -> bool {
    if profile.id == "opencode-go" || profile.id == "opencode-free" {
        return true;
    }
    if let Ok(u) = reqwest::Url::parse(profile.base_url)
        && let Some(host) = u.host_str()
    {
        return host.ends_with("opencode.ai");
    }
    false
}
