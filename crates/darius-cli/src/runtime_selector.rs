use crate::config::ProfileConfig;
use crate::paths::DariusPaths;
use crate::runtime_selection::{ProviderSelection, RuntimeState};

pub(crate) fn select(
    config: Option<&ProfileConfig>,
    key: impl Fn(&str) -> Option<String>,
) -> RuntimeState {
    let Some(model) = config.and_then(|config| config.model.as_ref()) else {
        // Cutover (1.5): no stored config always enters setup guidance.
        // Bare environment keys never auto-select a provider.
        return RuntimeState::Setup;
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
        auth_type: model.auth_type,
        default_max_tokens: model.default_max_tokens,
        context_length: model.context_length,
        headers: model.headers.clone(),
        openrouter: config.and_then(|c| c.openrouter.clone()),
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

pub(crate) fn nonblank(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Loads dotenv files in priority order (later sources override earlier):
/// workspace `.env`, then resolved Darius home `.env`.
/// Process environment always wins; existing variables are never overwritten.
pub fn load_dotenv_if_present(paths: &DariusPaths) {
    if let Ok(content) = std::fs::read_to_string(paths.workspace.join(".env")) {
        parse_and_apply_dotenv(&content);
    }
    if let Ok(content) = std::fs::read_to_string(paths.home.join(".env")) {
        parse_and_apply_dotenv(&content);
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
    use crate::config::ModelConfig;

    fn profile_with_model(model: ModelConfig) -> ProfileConfig {
        ProfileConfig {
            model: Some(model),
            ..Default::default()
        }
    }

    #[test]
    fn runtime_selection_no_config_or_usable_key_enters_setup() {
        // No stored config: setup, even with bare keys present.
        for key in [
            |_: &str| None,
            |name: &str| {
                (name == "DARIUS_API_KEY" || name == "OPENAI_API_KEY")
                    .then(|| "bare-key".to_string())
            },
        ] {
            let state = select(None, key);
            assert!(
                matches!(state, RuntimeState::Setup),
                "no config must enter setup, got {}",
                state.label()
            );
        }
        // Stored config without a model section: setup as well.
        let state = select(Some(&ProfileConfig::default()), |_| None);
        assert!(matches!(state, RuntimeState::Setup));
    }

    #[test]
    fn runtime_selection_never_resolves_implicit_openai() {
        let key = |name: &str| {
            (name == "DARIUS_API_KEY" || name == "OPENAI_API_KEY").then(|| "bare-key".to_string())
        };
        match select(None, key) {
            RuntimeState::Live(sel) | RuntimeState::MissingKey(sel) => {
                panic!(
                    "bare keys must not select a provider, got {}/{}",
                    sel.base_url, sel.model
                );
            }
            RuntimeState::Setup | RuntimeState::OfflineDemo => {}
        }
    }

    #[test]
    fn runtime_selection_explicit_config_still_resolves() {
        let config = profile_with_model(ModelConfig {
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            model: "z-ai/glm-5.2".into(),
            api_key_env: Some("DARIUS_WU02_TEST_KEY".into()),
            auth_type: None,
            default_max_tokens: None,
            context_length: None,
            headers: None,
        });
        assert!(matches!(
            select(Some(&config), |_| Some("k".into())),
            RuntimeState::Live(_)
        ));
        assert!(matches!(
            select(Some(&config), |_| None),
            RuntimeState::MissingKey(_)
        ));
    }

    #[test]
    fn runtime_selection_carries_openrouter_section() {
        use crate::config::OpenRouterConfig;
        let mut config = profile_with_model(ModelConfig {
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            model: "z-ai/glm-5.2".into(),
            api_key_env: Some("DARIUS_WU07_TEST_KEY".into()),
            auth_type: None,
            default_max_tokens: None,
            context_length: None,
            headers: None,
        });
        config.openrouter = Some(OpenRouterConfig {
            sort: Some("price".into()),
            only: vec![],
            ignore: vec![],
            order: vec!["a".into()],
        });
        match select(Some(&config), |_| Some("k".into())) {
            RuntimeState::Live(sel) => {
                let routing = sel
                    .openrouter
                    .as_ref()
                    .and_then(|o| o.provider_routing())
                    .expect("section survives select");
                assert_eq!(routing.sort.as_deref(), Some("price"));
                // Profile-id gate lives in the transport (3.6); selection just carries.
                assert!(routing.only.is_empty());
            }
            other => panic!("expected live, got {}", other.label()),
        }
        // Absent section -> None (transport sends no key at all).
        let bare = profile_with_model(ModelConfig {
            provider: "nous".into(),
            base_url: "https://inference-api.nousresearch.com/v1".into(),
            model: "hermes-3-405b".into(),
            api_key_env: Some("NOUS_API_KEY".into()),
            auth_type: None,
            default_max_tokens: None,
            context_length: None,
            headers: None,
        });
        match select(Some(&bare), |_| Some("k".into())) {
            RuntimeState::Live(sel) => assert!(sel.openrouter.is_none()),
            other => panic!("expected live, got {}", other.label()),
        }
    }

    #[test]
    fn dotenv_home_env_resolves_under_relocated_home() {
        // DARIUS_HOME relocation: home .env is read from paths.home, and the
        // merged value is visible to every path that funnels through
        // resolve_profile (run/tui/serve all call load_dotenv_if_present).
        let home = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join(".env"),
            "DARIUS_WU03_HOME_KEY=home-secret\n",
        )
        .unwrap();
        let var = "DARIUS_WU03_HOME_KEY";
        assert!(std::env::var(var).is_err());
        let paths = DariusPaths {
            home: home.path().to_owned(),
            workspace: workspace.path().to_owned(),
        };
        load_dotenv_if_present(&paths);
        assert_eq!(std::env::var(var).unwrap(), "home-secret");
        unsafe { std::env::remove_var(var) };
    }

    #[test]
    fn dotenv_process_env_wins_over_dotenv_files() {
        let home = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".env"), "DARIUS_WU03_WIN_KEY=file-value\n").unwrap();
        std::fs::write(
            workspace.path().join(".env"),
            "DARIUS_WU03_WIN_KEY=workspace-value\n",
        )
        .unwrap();
        let var = "DARIUS_WU03_WIN_KEY";
        unsafe { std::env::set_var(var, "process-value") };
        let paths = DariusPaths {
            home: home.path().to_owned(),
            workspace: workspace.path().to_owned(),
        };
        load_dotenv_if_present(&paths);
        assert_eq!(std::env::var(var).unwrap(), "process-value");
        unsafe { std::env::remove_var(var) };
    }

    #[test]
    fn dotenv_workspace_env_keeps_back_compat() {
        // Workspace .env alone (no home .env) still resolves: back-compat.
        let home = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join(".env"),
            "DARIUS_WU03_WS_KEY=workspace-secret\n",
        )
        .unwrap();
        let var = "DARIUS_WU03_WS_KEY";
        assert!(std::env::var(var).is_err());
        let paths = DariusPaths {
            home: home.path().to_owned(),
            workspace: workspace.path().to_owned(),
        };
        load_dotenv_if_present(&paths);
        assert_eq!(std::env::var(var).unwrap(), "workspace-secret");
        unsafe { std::env::remove_var(var) };
    }

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
