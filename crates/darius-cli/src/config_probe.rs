//! Live connectivity and authentication probe for configured model providers.
use crate::config::ProfileConfig;
use crate::paths::DariusPaths;
use darius_cognitive::{AsyncModel, Message, TurnContext};
use darius_daemon::{LiveModel, Provider};
use std::time::Instant;

pub async fn run_config_probe(paths: &DariusPaths, profile: &str) -> Result<(), String> {
    let config_path =
        ProfileConfig::config_path(paths, profile).map_err(|e| format!("path error: {e}"))?;
    if !config_path.exists() {
        return Err(format!(
            "No model configured for profile '{profile}'. Run `darius config init` first."
        ));
    }
    let profile_config =
        ProfileConfig::load(paths, profile).map_err(|e| format!("failed to load config: {e}"))?;
    let model_cfg = match profile_config.model {
        Some(m) => m,
        None => {
            return Err(format!(
                "No model configured for profile '{profile}'. Run `darius config init` first."
            ));
        }
    };

    let key_env = model_cfg
        .api_key_env
        .clone()
        .unwrap_or_else(|| "DARIUS_API_KEY".into());

    let profile = darius_core::providers::resolve(&model_cfg.provider);
    let is_keyless = profile.is_some_and(|p| {
        p.auth_type == darius_core::providers::AuthType::Keyless || p.key_env.is_none()
    }) || key_env.eq_ignore_ascii_case("none");
    let is_nous =
        profile.is_some_and(|p| p.auth_type == darius_core::providers::AuthType::NousOAuth);

    // Credential source for reporting (never the secret itself).
    let mut source = if is_keyless {
        "keyless (no credential required)".to_string()
    } else if is_nous {
        format!("oauth store ({}/auth.json)", paths.home.display())
    } else {
        format!("environment variable '{key_env}'")
    };

    if is_nous {
        // Resolve (refreshing if expired) so failures name the source here,
        // not as a bare transport error below.
        let svc = darius_daemon::NousAuthService::new(&paths.home)
            .map_err(|e| format!("failed to open oauth store: {e}"))?;
        if let Err(e) = svc.resolve_token().await {
            println!("Provider: {}", model_cfg.provider);
            println!("Endpoint: {}", model_cfg.base_url);
            println!("Model: {}", model_cfg.model);
            println!("Credential: {source}");
            println!("Status: FAILED (nous token unavailable: {e})");
            return Err(format!("nous token unavailable via {source}: {e}"));
        }
        source = format!("oauth store ({}/auth.json, resolved)", paths.home.display());
    }

    let is_local = model_cfg.base_url.contains("localhost")
        || model_cfg.base_url.contains("127.0.0.1")
        || model_cfg.base_url.contains("0.0.0.0")
        || is_keyless;

    if !is_local
        && std::env::var(&key_env)
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
    {
        println!("Provider: {}", model_cfg.provider);
        println!("Endpoint: {}", model_cfg.base_url);
        println!("Model: {}", model_cfg.model);
        println!("Credential: {source}");
        println!(
            "Status: FAILED (authentication failed: API key environment variable '{}' is not set)",
            key_env
        );
        return Err(format!(
            "API key environment variable '{key_env}' is not set"
        ));
    }

    let provider = Provider {
        name: model_cfg.provider.clone(),
        model: model_cfg.model.clone(),
        base_url: model_cfg.base_url.clone(),
        enabled: true,
        api_key_env: key_env.clone(),
    };

    let mut model = LiveModel::for_provider(provider)
        .map_err(|e| format!("failed to initialize provider client: {e}"))?;

    let probe_messages = vec![Message::User {
        content: "ping".into(),
    }];

    let start = Instant::now();
    let ctx = TurnContext::new();

    match model.complete(&probe_messages, &[], &ctx).await {
        Ok(_) => {
            let latency_ms = start.elapsed().as_millis();
            println!("Provider: {}", model_cfg.provider);
            println!("Endpoint: {}", model_cfg.base_url);
            println!("Model: {}", model_cfg.model);
            println!("Credential: {source}");
            println!("Status: OK ({}ms)", latency_ms);
            Ok(())
        }
        Err(e) => {
            let err_str = e.to_string();
            println!("Provider: {}", model_cfg.provider);
            println!("Endpoint: {}", model_cfg.base_url);
            println!("Model: {}", model_cfg.model);
            if err_str.contains("401") || err_str.contains("authentication failed") {
                println!("Status: FAILED (Invalid API key in {key_env})");
            } else if err_str.contains("404") || err_str.contains("not found") {
                println!(
                    "Status: FAILED (Model '{}' not found on endpoint '{}')",
                    model_cfg.model, model_cfg.base_url
                );
            } else if err_str.contains("429") || err_str.contains("rate limited") {
                println!("Status: FAILED (Quota exceeded or rate limited)");
            } else if err_str.contains("timed out") || err_str.contains("connectivity") {
                println!("Status: FAILED (Network timeout / DNS resolution failure)");
            } else {
                println!("Status: FAILED ({err_str})");
            }
            Err(err_str)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_paths(temp: &TempDir) -> DariusPaths {
        // Direct construction: resolve() would read the real $HOME/.darius.
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        DariusPaths {
            home,
            workspace: temp.path().to_path_buf(),
        }
    }

    #[tokio::test]
    async fn probe_missing_config_returns_error() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let res = run_config_probe(&paths, "nonexistent").await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("No model configured"));
    }

    #[tokio::test]
    async fn probe_missing_key_reports_auth_failure() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let profile = paths.profile("unauth").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let toml_str = r#"[model]
provider = "openai"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "DARIUS_PROBE_UNSET_KEY_9999"
"#;
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();
        let res = run_config_probe(&paths, "unauth").await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("is not set"));
    }

    #[tokio::test]
    async fn probe_live_endpoint_success() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "probe_id",
                "choices": [{"message": {"role": "assistant", "content": "pong"}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let profile = paths.profile("test_ok").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let toml_str = format!(
            r#"[model]
provider = "openai"
base_url = "{}/v1"
model = "gpt-4o-mini"
api_key_env = "NONE"
"#,
            server.uri()
        );
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();
        let res = run_config_probe(&paths, "test_ok").await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn probe_live_endpoint_401() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let profile = paths.profile("test_401").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let toml_str = format!(
            r#"[model]
provider = "openai"
base_url = "{}/v1"
model = "gpt-4o-mini"
api_key_env = "NONE"
"#,
            server.uri()
        );
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();
        let res = run_config_probe(&paths, "test_401").await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("authentication failed"));
    }

    #[tokio::test]
    async fn probe_live_endpoint_429() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&server)
            .await;

        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let profile = paths.profile("test_429").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let toml_str = format!(
            r#"[model]
provider = "openai"
base_url = "{}/v1"
model = "gpt-4o-mini"
api_key_env = "NONE"
"#,
            server.uri()
        );
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();
        let res = run_config_probe(&paths, "test_429").await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("rate limited"));
    }

    #[tokio::test]
    async fn probe_keyless_profile_needs_no_key() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "probe_id",
                "choices": [{"message": {"role": "assistant", "content": "pong"}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let profile = paths.profile("keyless").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        // No api_key_env at all: opencode-free is keyless by profile.
        let toml_str = format!(
            r#"[model]
provider = "opencode-free"
base_url = "{}/v1"
model = "laguna-s-2.1-free"
"#,
            server.uri()
        );
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();
        assert!(run_config_probe(&paths, "keyless").await.is_ok());
    }

    #[tokio::test]
    async fn probe_nous_without_credential_names_oauth_source() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let profile = paths.profile("nous_nokey").unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        let toml_str = r#"[model]
provider = "nous"
base_url = "https://inference-api.nousresearch.com/v1"
model = "hermes-3-405b"
api_key_env = "DARIUS_PROBE_NOUS_UNSET_9999"
"#;
        std::fs::write(profile.join("config.toml"), toml_str).unwrap();
        let err = run_config_probe(&paths, "nous_nokey").await.unwrap_err();
        assert!(err.contains("nous"), "{err}");
        assert!(err.contains("auth.json"), "{err}");
    }
}
