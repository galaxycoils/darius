//! Model configuration persistence and catalog definitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

use crate::providers::AuthType;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: String,
    #[serde(default)]
    pub auth_type: Option<AuthType>,
    #[serde(default)]
    pub default_max_tokens: Option<u32>,
    #[serde(default)]
    pub context_length: Option<u32>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCatalogEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    pub model: &'static str,
    pub base_url: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProfileConfigFile {
    model: ModelConfig,
}

/// Returns the standard catalog of model options for the interactive picker.
///
/// Built from the six Hermes-parity provider profiles: one entry per registry
/// fallback model, so the picker tracks the registry with no second hardcoded
/// list. OpenAI/OpenAI-compatible entries are gone from the default story;
/// explicit custom `openai_compatible` configs still load (see 6.2).
pub fn default_model_catalog() -> Vec<ModelCatalogEntry> {
    crate::providers::all_profiles()
        .iter()
        .flat_map(|p| {
            p.fallback_models.iter().map(move |m| ModelCatalogEntry {
                id: m,
                label: m,
                provider: p.id,
                model: m,
                base_url: p.base_url,
            })
        })
        .collect()
}

/// Converts a catalog entry into an active `ModelConfig`.
pub fn catalog_entry_to_config(entry: &ModelCatalogEntry) -> ModelConfig {
    let profile = crate::providers::resolve(entry.provider);
    let api_key_env = profile
        .and_then(|p| p.key_env)
        .unwrap_or("NONE")
        .to_string();
    let (auth_type, default_max_tokens) = profile
        .map(|p| (Some(p.auth_type), p.default_max_tokens))
        .unwrap_or((None, None));
    ModelConfig {
        provider: entry.provider.to_string(),
        base_url: entry.base_url.to_string(),
        model: entry.model.to_string(),
        api_key_env,
        auth_type,
        default_max_tokens,
        context_length: None,
        headers: None,
    }
}

/// Saves a `ModelConfig` into `<profile_dir>/config.toml`.
pub fn save_model_config(profile_dir: &Path, cfg: &ModelConfig) -> Result<(), ConfigError> {
    std::fs::create_dir_all(profile_dir)?;
    let file = ProfileConfigFile { model: cfg.clone() };
    let content = toml::to_string_pretty(&file)?;
    let path = profile_dir.join("config.toml");
    std::fs::write(path, content)?;
    Ok(())
}

/// Loads `ModelConfig` from `<profile_dir>/config.toml`, returning a default if missing.
pub fn load_model_config(profile_dir: &Path) -> Result<ModelConfig, ConfigError> {
    let path = profile_dir.join("config.toml");
    if !path.exists() {
        return Ok(catalog_entry_to_config(&default_model_catalog()[0]));
    }
    let content = std::fs::read_to_string(path)?;
    let file: ProfileConfigFile = toml::from_str(&content)?;
    Ok(file.model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_config_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = ModelConfig {
            provider: "openai_compatible".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            api_key_env: "DARIUS_API_KEY".into(),
            auth_type: None,
            default_max_tokens: None,
            context_length: None,
            headers: None,
        };
        save_model_config(dir.path(), &cfg).unwrap();
        let loaded = load_model_config(dir.path()).unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn model_config_optional_fields_default_and_extensions_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut headers = HashMap::new();
        headers.insert("X-Test".into(), "value".into());
        let cfg = ModelConfig {
            provider: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            model: "test".into(),
            api_key_env: "NONE".into(),
            auth_type: Some(AuthType::ApiKey),
            default_max_tokens: Some(16384),
            context_length: Some(131072),
            headers: Some(headers.clone()),
        };
        save_model_config(dir.path(), &cfg).unwrap();
        let loaded = load_model_config(dir.path()).unwrap();
        assert_eq!(loaded.default_max_tokens, Some(16384));
        assert_eq!(loaded.context_length, Some(131072));
        assert_eq!(loaded.auth_type, Some(AuthType::ApiKey));
        assert_eq!(loaded.headers.as_ref().unwrap()["X-Test"], "value");

        let legacy = ModelConfig {
            provider: "openai_compatible".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "old".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            auth_type: None,
            default_max_tokens: None,
            context_length: None,
            headers: None,
        };
        let text = toml::to_string(&legacy).unwrap();
        let parsed: ModelConfig = toml::from_str(&text).unwrap();
        assert!(parsed.auth_type.is_none());
        assert!(parsed.headers.is_none());
    }

    #[test]
    fn default_model_catalog_has_no_mock_default() {
        let catalog = default_model_catalog();
        assert!(!catalog.is_empty());
        assert!(catalog.iter().all(|e| e.id != "mock"));
        // Six Hermes-parity providers, no OpenAI-default story.
        for id in [
            "kilocode",
            "opencode-go",
            "opencode-free",
            "nous",
            "openrouter",
            "nvidia",
        ] {
            assert!(
                catalog.iter().any(|e| e.provider == id),
                "missing provider {id}"
            );
        }
        assert!(
            catalog
                .iter()
                .all(|e| !e.base_url.contains("api.openai.com")),
            "no default entry may resolve to api.openai.com"
        );
        // Spot-checks against the ported Hermes fallback lists.
        for model in [
            "nvidia/llama-3.1-nemotron-70b-instruct",
            "nvidia/llama-3.3-70b-instruct",
            "hermes-3-405b",
            "hermes-3-70b",
        ] {
            assert!(catalog.iter().any(|e| e.model == model), "missing {model}");
        }
    }
}
