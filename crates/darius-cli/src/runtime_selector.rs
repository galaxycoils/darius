use crate::config::ProfileConfig;
use crate::runtime_selection::{ProviderSelection, RuntimeState};

pub(crate) fn select(
    config: Option<&ProfileConfig>,
    key: impl Fn(&str) -> Option<String>,
) -> RuntimeState {
    let Some(model) = config.and_then(|config| config.model.as_ref()) else {
        return config.map_or_else(|| implicit(key), |_| RuntimeState::Setup);
    };
    let key_env = model
        .api_key_env
        .clone()
        .unwrap_or_else(|| "DARIUS_API_KEY".into());
    let selected = ProviderSelection {
        provider: model.provider.clone(),
        model: model.model.clone(),
        base_url: model.base_url.clone(),
        key_env,
    };
    if key(&selected.key_env).is_some() {
        RuntimeState::Live(selected)
    } else {
        RuntimeState::MissingKey(selected)
    }
}

fn implicit(key: impl Fn(&str) -> Option<String>) -> RuntimeState {
    let key_env = ["DARIUS_API_KEY", "OPENAI_API_KEY"]
        .into_iter()
        .find(|name| key(name).is_some());
    key_env
        .map(|key_env| {
            RuntimeState::Live(ProviderSelection {
                provider: "openai_compatible".into(),
                model: "gpt-4o-mini".into(),
                base_url: "https://api.openai.com/v1".into(),
                key_env: key_env.into(),
            })
        })
        .unwrap_or(RuntimeState::Setup)
}

pub(crate) fn nonblank(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}
