//! Model configuration persistence and catalog definitions.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

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
    pub api_key_env: String,
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
pub fn default_model_catalog() -> Vec<ModelCatalogEntry> {
    vec![
        ModelCatalogEntry {
            id: "mock",
            label: "Mock (offline demo)",
            provider: "mock",
            model: "mock",
            base_url: "http://localhost:8080/v1",
        },
        ModelCatalogEntry {
            id: "gpt-4o-mini",
            label: "OpenAI gpt-4o-mini (fast & lean)",
            provider: "openai",
            model: "gpt-4o-mini",
            base_url: "https://api.openai.com/v1",
        },
        ModelCatalogEntry {
            id: "gpt-4o",
            label: "OpenAI gpt-4o (high capability)",
            provider: "openai",
            model: "gpt-4o",
            base_url: "https://api.openai.com/v1",
        },
        ModelCatalogEntry {
            id: "ollama",
            label: "Ollama (local custom model)",
            provider: "ollama",
            model: "llama3.2",
            base_url: "http://localhost:11434/v1",
        },
    ]
}

/// Converts a catalog entry into an active `ModelConfig`.
pub fn catalog_entry_to_config(entry: &ModelCatalogEntry) -> ModelConfig {
    let api_key_env = match entry.provider {
        "mock" | "ollama" => "NONE".into(),
        _ => "OPENAI_API_KEY".into(),
    };
    ModelConfig {
        provider: entry.provider.to_string(),
        base_url: entry.base_url.to_string(),
        model: entry.model.to_string(),
        api_key_env,
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
        };
        save_model_config(dir.path(), &cfg).unwrap();
        let loaded = load_model_config(dir.path()).unwrap();
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn catalog_contains_mock_and_gpt4o_mini() {
        let catalog = default_model_catalog();
        assert!(catalog.iter().any(|e| e.id == "mock"));
        assert!(catalog.iter().any(|e| e.id == "gpt-4o-mini"));
        assert!(catalog.iter().any(|e| e.id == "gpt-4o"));
    }
}
