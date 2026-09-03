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
        format!("Provider URL: {provider_url}"),
    ];
    if let RuntimeState::Live(provider) | RuntimeState::MissingKey(provider) = state {
        output.push(key_line(&provider.key_env));
    }
    output
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
