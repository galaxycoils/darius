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
    let is_local = selected.base_url.contains("localhost")
        || selected.base_url.contains("127.0.0.1")
        || selected.base_url.contains("0.0.0.0")
        || selected.key_env.eq_ignore_ascii_case("NONE");
    if is_local || key(&selected.key_env).is_some() {
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

/// Discovers and loads variables from `.env` in the specified directory or current working directory.
/// Existing environment variables are never overwritten.
pub fn load_dotenv_if_present(dir: Option<&std::path::Path>) {
    let mut search_dirs = Vec::new();
    if let Some(dir) = dir {
        search_dirs.push(dir.to_path_buf());
    }
    if let Ok(current) = std::env::current_dir()
        && !search_dirs.contains(&current)
    {
        search_dirs.push(current);
    }
    for dir in search_dirs {
        let env_path = dir.join(".env");
        if let Ok(content) = std::fs::read_to_string(&env_path) {
            parse_and_apply_dotenv(&content);
            break;
        }
    }
}

pub fn parse_and_apply_dotenv(content: &str) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, val)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut val = val.trim();
        if val.len() >= 2
            && ((val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\'')))
        {
            val = &val[1..val.len() - 1];
        }
        if std::env::var(key).is_err() && !val.is_empty() {
            unsafe {
                std::env::set_var(key, val);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dotenv_sets_unassigned_variables() {
        let content = r#"
        # Sample comment
        DARIUS_DOTENV_TEST_VAR_1=secret_token_123
        DARIUS_DOTENV_TEST_VAR_2="quoted_value"
        DARIUS_DOTENV_TEST_VAR_3='single_quoted'
        "#;
        parse_and_apply_dotenv(content);
        assert_eq!(
            std::env::var("DARIUS_DOTENV_TEST_VAR_1").unwrap(),
            "secret_token_123"
        );
        assert_eq!(
            std::env::var("DARIUS_DOTENV_TEST_VAR_2").unwrap(),
            "quoted_value"
        );
        assert_eq!(
            std::env::var("DARIUS_DOTENV_TEST_VAR_3").unwrap(),
            "single_quoted"
        );
    }
}
