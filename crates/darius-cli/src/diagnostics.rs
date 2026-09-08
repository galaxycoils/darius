use std::path::Path;

use crate::paths::DariusPaths;
use crate::runtime_selection::RuntimeState;
use crate::runtime_selector::nonblank;

pub(crate) fn lines(
    paths: &DariusPaths,
    profile: &str,
    config_path: &Path,
    config_exists: bool,
    state: &RuntimeState,
    memory_open: bool,
) -> Vec<String> {
    let profile_path = paths.profile(profile).expect("validated profile");
    let provider_url = provider_url(state);
    let mut output = vec![
        format!("Version: {}", env!("CARGO_PKG_VERSION")),
        format!("Runtime state: {}", state.label()),
        format!("Home: {}", paths.home.display()),
        format!("Profile path: {}", profile_path.display()),
        format!("Config path: {}", config_path.display()),
        format!(
            "Config parse: {}",
            if config_exists { "valid" } else { "missing" }
        ),
        key_line("DARIUS_API_KEY"),
        key_line("OPENAI_API_KEY"),
        format!(
            "Memory: {}",
            if memory_open { "open" } else { "unavailable" }
        ),
        format!("Workspace: {}", paths.workspace.display()),
        format!("Provider URL: {}", strip_url_secrets(provider_url)),
    ];
    if let RuntimeState::Live(provider) | RuntimeState::MissingKey(provider) = state {
        output.push(key_line(&provider.key_env));
    }
    output
}

/// Strip userinfo (user:pass@) and query (?key=...) from a URL for display.
pub(crate) fn strip_url_secrets(url: &str) -> String {
    if matches!(url, "not configured" | "offline (no network)") {
        return url.to_string();
    }
    match url::Url::parse(url) {
        Ok(mut u) => {
            if u.has_authority() && !u.username().is_empty() {
                let _ = u.set_username("");
            }
            let _ = u.set_password(None);
            u.set_query(None);
            u.set_fragment(None);
            u.to_string()
        }
        Err(_) => "[invalid provider URL]".into(),
    }
}

fn provider_url(state: &RuntimeState) -> &str {
    match state {
        RuntimeState::Live(provider) | RuntimeState::MissingKey(provider) => &provider.base_url,
        RuntimeState::OfflineDemo => "offline (no network)",
        RuntimeState::Setup => "not configured",
    }
}

fn key_line(name: &str) -> String {
    let state = if nonblank(name).is_some() {
        "present"
    } else {
        "missing"
    };
    format!("{name}: {state}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_url_secrets_fails_closed_on_malformed_urls_and_fragments() {
        for raw in [
            "not-a-url?token=fixture-secret",
            "http://[broken?token=fixture-secret",
            "https://example.com/v1#fixture-secret",
        ] {
            assert!(!strip_url_secrets(raw).contains("fixture-secret"));
        }
        assert_eq!(strip_url_secrets("not configured"), "not configured");
        assert_eq!(
            strip_url_secrets("offline (no network)"),
            "offline (no network)"
        );
    }

    #[test]
    fn strip_url_secrets_removes_userinfo_and_query() {
        assert_eq!(
            strip_url_secrets("https://user:pass@example.com/v1"),
            "https://example.com/v1"
        );
        assert_eq!(
            strip_url_secrets("https://example.com/v1?api_key=secret123"),
            "https://example.com/v1"
        );
        assert_eq!(
            strip_url_secrets("https://u:p@host/path?k=v"),
            "https://host/path"
        );
        assert_eq!(
            strip_url_secrets("https://example.com/v1"),
            "https://example.com/v1"
        );
    }
}
