use crate::config::{ModelConfig, ProfileConfig};
use crate::config_error::ConfigError;
use crate::config_publish::{publish, write_temp};
use crate::paths::DariusPaths;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Preset the first-run wizard recommends: keyless, no signup friction.
/// Presets are never auto-selected — `select()` enters setup without config.
pub const RECOMMENDED_PRESET: &str = "opencode-free";

#[derive(Debug, Clone)]
pub struct ProviderMetadata {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub auth_type: Option<darius_core::providers::AuthType>,
    pub default_max_tokens: Option<u32>,
}

impl ProviderMetadata {
    pub fn from_preset_or_fields(
        preset: Option<&str>,
        provider: Option<String>,
        base_url: Option<url::Url>,
        model: Option<String>,
        key_env: Option<String>,
    ) -> Result<Self, ConfigError> {
        let preset = preset.map(|name| name.to_lowercase());
        // Exact Hermes base URLs / key envs from `darius-core/src/providers/*`.
        let (p_provider, p_base, p_model, p_key, p_auth, p_max): (
            &str,
            &str,
            &str,
            Option<&str>,
            darius_core::providers::AuthType,
            Option<u32>,
        ) = match preset.as_deref() {
            Some("kilocode") => (
                "kilocode",
                "https://api.kilo.ai/api/gateway",
                "google/gemini-3.6-flash",
                Some("KILOCODE_API_KEY"),
                darius_core::providers::AuthType::ApiKey,
                None,
            ),
            Some("opencode-go") => (
                "opencode-go",
                "https://opencode.ai/zen/go/v1",
                "glm-5",
                Some("OPENCODE_GO_API_KEY"),
                darius_core::providers::AuthType::ApiKey,
                None,
            ),
            Some("opencode-free") => (
                "opencode-free",
                "https://opencode.ai/zen/v1",
                "laguna-s-2.1-free",
                Some("NONE"),
                darius_core::providers::AuthType::Keyless,
                None,
            ),
            Some("nous") => (
                "nous",
                "https://inference-api.nousresearch.com/v1",
                "z-ai/glm-5.2",
                Some("NOUS_API_KEY"),
                darius_core::providers::AuthType::NousOAuth,
                None,
            ),
            Some("openrouter") => (
                "openrouter",
                "https://openrouter.ai/api/v1",
                "z-ai/glm-5.2",
                Some("OPENROUTER_API_KEY"),
                darius_core::providers::AuthType::ApiKey,
                None,
            ),
            Some("nvidia") => (
                "nvidia",
                "https://integrate.api.nvidia.com/v1",
                "nvidia/llama-3.1-nemotron-70b-instruct",
                Some("NVIDIA_API_KEY"),
                darius_core::providers::AuthType::ApiKey,
                Some(16384),
            ),
            Some(unknown) => {
                return Err(ConfigError::UnknownPreset {
                    name: unknown.to_string(),
                });
            }
            None => {
                // No preset: explicit fields only, never an implicit default.
                let (Some(provider), Some(base_url), Some(model)) = (provider, base_url, model)
                else {
                    return Err(ConfigError::UnknownPreset {
                        name: format!("<none> (try '{RECOMMENDED_PRESET}' for a keyless start)"),
                    });
                };
                return Ok(Self {
                    provider,
                    base_url: base_url.to_string(),
                    model,
                    api_key_env: key_env,
                    auth_type: None,
                    default_max_tokens: None,
                });
            }
        };

        let provider = provider.unwrap_or_else(|| p_provider.to_string());
        let base_url = base_url
            .map(|u| u.to_string())
            .unwrap_or_else(|| p_base.to_string());
        let model = model.unwrap_or_else(|| p_model.to_string());
        let api_key_env = key_env.or_else(|| p_key.map(str::to_string));

        Ok(Self {
            provider,
            base_url,
            model,
            api_key_env,
            auth_type: Some(p_auth),
            default_max_tokens: p_max,
        })
    }
}

pub fn initialize_profile(
    paths: &DariusPaths,
    profile: &str,
    metadata: &ProviderMetadata,
    force: bool,
) -> Result<PathBuf, ConfigError> {
    let directory = paths.profile(profile)?;
    fs::create_dir_all(&directory).map_err(ConfigError::Write)?;
    let target = directory.join("config.toml");
    if target.exists() && !force {
        return Err(ConfigError::AlreadyExists { path: target });
    }
    let config = ProfileConfig {
        model: Some(ModelConfig {
            provider: metadata.provider.clone(),
            base_url: metadata.base_url.clone(),
            model: metadata.model.clone(),
            api_key_env: metadata.api_key_env.clone(),
            auth_type: metadata.auth_type,
            default_max_tokens: metadata.default_max_tokens,
            context_length: None,
            headers: None,
        }),
        model_overrides: HashMap::new(),
        ..Default::default()
    };
    config.validate()?;
    let content = toml::to_string(&config).map_err(|_| ConfigError::Serialize)?;
    let temporary = directory.join(format!(".config.toml.{}.tmp", uuid::Uuid::new_v4()));
    let result =
        write_temp(&temporary, &content).and_then(|()| publish(&temporary, &target, force));
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(target)
}

/// One wizard choice: a registry provider in menu order.
pub struct SetupChoice {
    pub id: &'static str,
    pub display: &'static str,
    pub recommended: bool,
}

/// Six-provider wizard choices in registry order; exactly one recommendation.
pub fn setup_provider_choices() -> Vec<SetupChoice> {
    darius_core::providers::all_profiles()
        .iter()
        .map(|p| SetupChoice {
            id: p.id,
            display: p.display_name,
            recommended: p.id == RECOMMENDED_PRESET,
        })
        .collect()
}

/// Pure menu text for `darius config setup` (interactive path).
pub fn render_setup_menu() -> String {
    let mut out = String::from("Choose a model provider:\n");
    for (i, choice) in setup_provider_choices().iter().enumerate() {
        let marker = if choice.recommended {
            " (recommended)"
        } else {
            ""
        };
        out.push_str(&format!(
            "  {}. {} — {}{marker}\n",
            i + 1,
            choice.id,
            choice.display
        ));
    }
    out.push_str(&format!(
        "Recommended: {RECOMMENDED_PRESET} (keyless, no signup). \
         The wizard recommends but never auto-selects.\n\
         Choose provider [1-6, name] (default: {RECOMMENDED_PRESET}): "
    ));
    out
}

/// Parse one wizard menu answer: empty defaults to [`RECOMMENDED_PRESET`],
/// otherwise a 1-based index or provider id/alias.
pub fn parse_setup_selection(input: &str) -> Result<&'static str, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(RECOMMENDED_PRESET);
    }
    let choices = setup_provider_choices();
    if let Ok(n) = trimmed.parse::<usize>()
        && (1..=choices.len()).contains(&n)
    {
        return Ok(choices[n - 1].id);
    }
    darius_core::providers::resolve(trimmed)
        .map(|p| p.id)
        .ok_or_else(|| {
            let valid: Vec<_> = choices.iter().map(|c| c.id).collect();
            format!("Unknown provider '{trimmed}'. Valid: {}", valid.join(", "))
        })
}

/// Append (or replace) `KEY=value` in `<home>/.env` with mode 0600.
/// Secrets never touch `config.toml`.
pub(crate) fn append_home_env_key(
    home: &std::path::Path,
    key: &str,
    value: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(home)
        .map_err(|e| format!("failed to create {}: {e}", home.display()))?;
    let path = home.join(".env");
    let mut lines: Vec<String> = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|line| {
            line.split_once('=')
                .map(|(k, _)| k.trim() != key)
                .unwrap_or(true)
        })
        .map(str::to_string)
        .collect();
    lines.push(format!("{key}={value}"));
    let mut content = lines.join("\n");
    content.push('\n');
    std::fs::write(&path, &content)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    #[cfg(unix)]
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(|e| format!("failed to chmod {}: {e}", path.display()))?;
    // Current process sees the merged value, matching dotenv semantics.
    unsafe {
        std::env::set_var(key, value);
    }
    Ok(())
}

/// Best-effort live model list for the wizard; falls back to the registry
/// list when the endpoint is unreachable. Never fails setup.
fn print_wizard_model_list(
    home: &std::path::Path,
    profile: &'static darius_core::providers::ProviderProfile,
    base_url: &str,
    bearer: Option<&str>,
) {
    let endpoint = match profile.models_url {
        Some(url) => url.to_string(),
        None => format!("{}/models", base_url.trim_end_matches('/')),
    };
    let owned: Vec<(String, String)> = profile.request_headers();
    let refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let req = darius_daemon::CatalogRequest {
        profile,
        endpoint: &endpoint,
        bearer_token: bearer,
        headers: &refs,
        context_length: None,
    };
    let resp = match darius_daemon::ModelCatalog::new(home) {
        Ok(catalog) => crate::runtime::block_on_turn(catalog.fetch_models(&req)),
        Err(_) => darius_daemon::CatalogResponse::default(),
    };
    if resp.models.is_empty() {
        println!("Available models (fallback list):");
        for m in profile.fallback_models {
            println!("  {m}  [source: fallback]");
        }
    } else {
        let source = match resp.source {
            darius_daemon::CatalogSource::Live => "live",
            darius_daemon::CatalogSource::Cached => "cached",
            darius_daemon::CatalogSource::Fallback => "fallback",
        };
        println!("Available models (source: {source}):");
        for m in resp.models.iter().take(20) {
            let ctx = m
                .context_length
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            println!("  {}  context_length={ctx}  [source: {source}]", m.id);
        }
    }
}

pub fn run_setup(
    paths: &DariusPaths,
    profile: &str,
    provider_override: Option<&str>,
) -> Result<(), String> {
    let stdin = std::io::stdin();
    run_setup_impl(paths, profile, provider_override, &mut stdin.lock(), true)
}

pub(crate) fn run_setup_impl(
    paths: &DariusPaths,
    profile: &str,
    provider_override: Option<&str>,
    reader: &mut impl std::io::BufRead,
    live_fetch: bool,
) -> Result<(), String> {
    crate::runtime_selector::load_dotenv_if_present(paths);
    // Non-interactive `--provider` path never consults stdin.
    let provider_id: &'static str = match provider_override {
        Some(id) => darius_core::providers::resolve(id)
            .map(|p| p.id)
            .ok_or_else(|| {
                let valid: Vec<_> = setup_provider_choices().iter().map(|c| c.id).collect();
                format!("Unknown provider '{id}'. Valid: {}", valid.join(", "))
            })?,
        None => {
            print!("{}", render_setup_menu());
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
            let mut answer = String::new();
            reader
                .read_line(&mut answer)
                .map_err(|e| format!("failed to read choice: {e}"))?;
            parse_setup_selection(&answer)?
        }
    };
    let target = darius_core::providers::resolve(provider_id)
        .ok_or_else(|| format!("Unknown provider '{provider_id}'"))?;
    let metadata =
        ProviderMetadata::from_preset_or_fields(Some(provider_id), None, None, None, None)
            .map_err(|e| e.to_string())?;

    match target.auth_type {
        darius_core::providers::AuthType::Keyless => {
            println!("{provider_id} is keyless — no API key needed.");
        }
        darius_core::providers::AuthType::NousOAuth => {
            // OAuth is never run inline: persist first, then instruct.
            println!("After setup, run `darius auth login nous` to connect your Nous account.");
        }
        darius_core::providers::AuthType::ApiKey => {
            let key_env = target.key_env.unwrap_or("DARIUS_API_KEY");
            println!(
                "Enter {key_env} (stored in {}/.env with mode 0600; never written to config.toml).",
                paths.home.display()
            );
            println!("Note: input will be visible while typing (no hidden prompt available).");
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
            let mut secret = String::new();
            reader
                .read_line(&mut secret)
                .map_err(|e| format!("failed to read key: {e}"))?;
            let secret = secret.trim();
            if secret.is_empty() {
                println!(
                    "Empty value — setup continues, but `darius config probe` will fail until {key_env} is set."
                );
            } else {
                append_home_env_key(&paths.home, key_env, secret)?;
                println!(
                    "Stored {key_env} in {}/.env (mode 0600).",
                    paths.home.display()
                );
            }
        }
    }

    if live_fetch {
        let bearer = match target.auth_type {
            darius_core::providers::AuthType::Keyless => None,
            _ => target
                .key_env
                .and_then(|k| std::env::var(k).ok())
                .filter(|v| !v.trim().is_empty()),
        };
        // Never borrow a temporary: endpoint base lives in `metadata`.
        let bearer_owned = bearer;
        print_wizard_model_list(
            &paths.home,
            target,
            &metadata.base_url,
            bearer_owned.as_deref(),
        );
    }

    match initialize_profile(paths, profile, &metadata, false) {
        Ok(path) => {
            println!(
                "Initialized profile '{profile}' with provider '{provider_id}' at {}",
                path.display()
            );
            if target.auth_type == darius_core::providers::AuthType::NousOAuth {
                println!("Next: run `darius auth login nous` to authenticate.");
            }
            Ok(())
        }
        Err(ConfigError::AlreadyExists { path }) => Err(format!(
            "Profile '{profile}' already has a config at {}. Delete it or run `darius config init --force` to overwrite.",
            path.display()
        )),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str) -> ProviderMetadata {
        ProviderMetadata::from_preset_or_fields(Some(name), None, None, None, None).unwrap()
    }

    #[test]
    fn preset_kilocode_matches_registry() {
        let meta = preset("kilocode");
        assert_eq!(meta.provider, "kilocode");
        assert_eq!(meta.base_url, "https://api.kilo.ai/api/gateway");
        assert_eq!(meta.model, "google/gemini-3.6-flash");
        assert_eq!(meta.api_key_env.as_deref(), Some("KILOCODE_API_KEY"));
        assert!(!meta.base_url.contains("api.openai.com"));
    }

    #[test]
    fn preset_opencode_go_matches_registry() {
        let meta = preset("opencode-go");
        assert_eq!(meta.provider, "opencode-go");
        assert_eq!(meta.base_url, "https://opencode.ai/zen/go/v1");
        assert_eq!(meta.model, "glm-5");
        assert_eq!(meta.api_key_env.as_deref(), Some("OPENCODE_GO_API_KEY"));
        assert!(!meta.base_url.contains("api.openai.com"));
    }

    #[test]
    fn preset_opencode_free_is_keyless() {
        let meta = preset("opencode-free");
        assert_eq!(meta.provider, "opencode-free");
        assert_eq!(meta.base_url, "https://opencode.ai/zen/v1");
        assert_eq!(meta.api_key_env.as_deref(), Some("NONE"));
        assert_eq!(
            meta.auth_type,
            Some(darius_core::providers::AuthType::Keyless)
        );
        assert!(!meta.base_url.contains("api.openai.com"));
    }

    #[test]
    fn preset_nous_matches_registry() {
        let meta = preset("nous");
        assert_eq!(meta.provider, "nous");
        assert_eq!(meta.base_url, "https://inference-api.nousresearch.com/v1");
        assert_eq!(meta.api_key_env.as_deref(), Some("NOUS_API_KEY"));
        assert!(!meta.base_url.contains("api.openai.com"));
    }

    #[test]
    fn preset_openrouter_matches_registry() {
        let meta = preset("openrouter");
        assert_eq!(meta.provider, "openrouter");
        assert_eq!(meta.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(meta.api_key_env.as_deref(), Some("OPENROUTER_API_KEY"));
        assert!(!meta.base_url.contains("api.openai.com"));
    }

    #[test]
    fn preset_nvidia_matches_registry_with_max_tokens() {
        let meta = preset("nvidia");
        assert_eq!(meta.provider, "nvidia");
        assert_eq!(meta.base_url, "https://integrate.api.nvidia.com/v1");
        assert_eq!(meta.model, "nvidia/llama-3.1-nemotron-70b-instruct");
        assert_eq!(meta.api_key_env.as_deref(), Some("NVIDIA_API_KEY"));
        assert_eq!(meta.default_max_tokens, Some(16384));
        assert!(!meta.base_url.contains("api.openai.com"));
    }

    #[test]
    fn preset_names_are_case_insensitive() {
        assert_eq!(preset("OpenRouter").base_url, preset("openrouter").base_url);
        assert_eq!(
            preset("OPEnCODE-Free").api_key_env,
            preset("opencode-free").api_key_env
        );
    }

    #[test]
    fn unknown_preset_is_an_error_not_openai() {
        for name in ["openai", "ollama", "groq", "bogus"] {
            let err = ProviderMetadata::from_preset_or_fields(Some(name), None, None, None, None)
                .unwrap_err();
            assert!(
                matches!(err, ConfigError::UnknownPreset { .. }),
                "{name}: got {err}"
            );
        }
    }

    #[test]
    fn no_preset_resolves_to_api_openai_com() {
        for name in [
            "kilocode",
            "opencode-go",
            "opencode-free",
            "nous",
            "openrouter",
            "nvidia",
        ] {
            let meta = preset(name);
            assert!(
                !meta.base_url.contains("api.openai.com"),
                "{name}: {}",
                meta.base_url
            );
        }
    }

    #[test]
    fn explicit_fields_override_preset() {
        let meta = ProviderMetadata::from_preset_or_fields(
            Some("nvidia"),
            None,
            None,
            Some("custom-model".into()),
            None,
        )
        .unwrap();
        assert_eq!(meta.model, "custom-model");
        assert_eq!(meta.base_url, "https://integrate.api.nvidia.com/v1");
        assert_eq!(meta.default_max_tokens, Some(16384));
    }

    #[test]
    fn recommended_preset_is_keyless_opencode_free() {
        assert_eq!(RECOMMENDED_PRESET, "opencode-free");
        assert_eq!(
            preset(RECOMMENDED_PRESET).api_key_env.as_deref(),
            Some("NONE")
        );
    }

    #[test]
    fn setup_menu_recommends_without_autoselect() {
        let choices = setup_provider_choices();
        assert_eq!(choices.len(), 6);
        let recommended: Vec<_> = choices.iter().filter(|c| c.recommended).collect();
        assert_eq!(recommended.len(), 1);
        assert_eq!(recommended[0].id, RECOMMENDED_PRESET);
        let menu = render_setup_menu();
        assert!(menu.contains("Recommended: opencode-free"), "{menu}");
        assert!(menu.contains("never auto-selects"), "{menu}");
        // Empty answer defaults to the recommendation; names/indices resolve.
        assert_eq!(parse_setup_selection("").unwrap(), "opencode-free");
        assert_eq!(parse_setup_selection("  \n").unwrap(), "opencode-free");
        assert_eq!(parse_setup_selection("nous").unwrap(), "nous");
        assert_eq!(parse_setup_selection("3").unwrap(), choices[2].id);
        assert!(parse_setup_selection("bogus").is_err());
    }

    fn setup_test_paths() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        crate::paths::DariusPaths,
    ) {
        let home = tempfile::TempDir::new().unwrap();
        let workspace = tempfile::TempDir::new().unwrap();
        let paths = crate::paths::DariusPaths {
            home: home.path().to_owned(),
            workspace: workspace.path().to_owned(),
        };
        (home, workspace, paths)
    }

    #[test]
    fn setup_noninteractive_opencode_free_writes_config() {
        let (_home, _ws, paths) = setup_test_paths();
        let mut stdin: &[u8] = b"";
        run_setup_impl(&paths, "default", Some("opencode-free"), &mut stdin, false).unwrap();
        let raw =
            std::fs::read_to_string(paths.profile("default").unwrap().join("config.toml")).unwrap();
        assert!(raw.contains("opencode-free"), "{raw}");
        // Keyless: no secret file created.
        assert!(!paths.home.join(".env").exists());
    }

    #[test]
    fn setup_key_written_to_home_env_not_config() {
        let (_home, _ws, paths) = setup_test_paths();
        let mut stdin: &[u8] = b"test-openrouter-secret\n";
        run_setup_impl(&paths, "default", Some("openrouter"), &mut stdin, false).unwrap();
        let env = std::fs::read_to_string(paths.home.join(".env")).unwrap();
        assert!(
            env.contains("OPENROUTER_API_KEY=test-openrouter-secret"),
            "{env}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(paths.home.join(".env"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        }
        let raw =
            std::fs::read_to_string(paths.profile("default").unwrap().join("config.toml")).unwrap();
        assert!(!raw.contains("test-openrouter-secret"), "{raw}");
        unsafe {
            std::env::remove_var("OPENROUTER_API_KEY");
        }
    }

    #[test]
    fn setup_nous_persists_without_running_oauth() {
        let (_home, _ws, paths) = setup_test_paths();
        let mut stdin: &[u8] = b"";
        run_setup_impl(&paths, "default", Some("nous"), &mut stdin, false).unwrap();
        let raw =
            std::fs::read_to_string(paths.profile("default").unwrap().join("config.toml")).unwrap();
        assert!(raw.contains("provider = \"nous\""), "{raw}");
        // OAuth never runs inline: no credential store is created.
        assert!(!paths.home.join("auth.json").exists());
    }

    #[test]
    fn setup_interactive_empty_answer_selects_recommended() {
        let (_home, _ws, paths) = setup_test_paths();
        let mut stdin: &[u8] = b"\n";
        run_setup_impl(&paths, "default", None, &mut stdin, false).unwrap();
        let raw =
            std::fs::read_to_string(paths.profile("default").unwrap().join("config.toml")).unwrap();
        assert!(raw.contains("opencode-free"), "{raw}");
    }
}
