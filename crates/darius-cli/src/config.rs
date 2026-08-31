//! Profile config — loads TOML from ~/.darius/profiles/<name>/config.toml

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProfileConfig {
    pub model: Option<ModelConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_env: Option<String>,
}

impl ProfileConfig {
    pub fn load(profile: &str) -> Self {
        let path = Self::config_path(profile);
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn config_path(profile: &str) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".darius")
            .join("profiles")
            .join(profile)
            .join("config.toml")
    }

    pub fn profile_dir(profile: &str) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".darius")
            .join("profiles")
            .join(profile)
    }

    pub fn api_key(&self) -> Option<String> {
        let env_name = self.model.as_ref()?.api_key_env.as_deref()?;
        std::env::var(env_name).ok()
    }

    pub fn is_configured(&self) -> bool {
        if self.model.is_none() {
            return false;
        }
        // Check if API key is available
        self.api_key().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_not_configured() {
        let config = ProfileConfig::default();
        assert!(!config.is_configured());
    }

    #[test]
    fn load_missing_file_returns_default() {
        let config = ProfileConfig::load("nonexistent_profile_12345");
        assert!(!config.is_configured());
    }

    #[test]
    fn parse_config_with_model() {
        let toml_str = r#"
[model]
provider = "openai_compatible"
base_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "DARIUS_API_KEY"
"#;
        let config: ProfileConfig = toml::from_str(toml_str).unwrap();
        assert!(config.model.is_some());
        let model = config.model.unwrap();
        assert_eq!(model.provider, "openai_compatible");
        assert_eq!(model.model, "gpt-4o-mini");
    }
}
