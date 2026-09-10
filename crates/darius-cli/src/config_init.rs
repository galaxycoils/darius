use crate::config::{ModelConfig, ProfileConfig};
use crate::config_error::ConfigError;
use crate::config_publish::{publish, write_temp};
use crate::paths::DariusPaths;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProviderMetadata {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_env: Option<String>,
}

impl ProviderMetadata {
    pub fn from_preset_or_fields(
        preset: Option<&str>,
        provider: Option<String>,
        base_url: Option<url::Url>,
        model: Option<String>,
        key_env: Option<String>,
    ) -> Result<Self, ConfigError> {
        let (p_provider, p_base, p_model, p_key) = match preset.map(str::to_lowercase).as_deref() {
            Some("openrouter") => (
                "openai_compatible",
                "https://openrouter.ai/api/v1",
                "anthropic/claude-3.5-sonnet",
                "OPENROUTER_API_KEY",
            ),
            Some("ollama") => ("ollama", "http://localhost:11434/v1", "llama3.2", "NONE"),
            Some("groq") => (
                "openai_compatible",
                "https://api.groq.com/openai/v1",
                "llama-3.3-70b-versatile",
                "GROQ_API_KEY",
            ),
            _ => (
                "openai_compatible",
                "https://api.openai.com/v1",
                "gpt-4o-mini",
                "OPENAI_API_KEY",
            ),
        };

        let provider = provider.unwrap_or_else(|| p_provider.to_string());
        let base_url = base_url
            .map(|u| u.to_string())
            .unwrap_or_else(|| p_base.to_string());
        let model = model.unwrap_or_else(|| p_model.to_string());
        let api_key_env = key_env.or_else(|| Some(p_key.to_string()));

        Ok(Self {
            provider,
            base_url,
            model,
            api_key_env,
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
