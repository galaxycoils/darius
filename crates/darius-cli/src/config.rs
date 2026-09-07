//! Validated configuration loaded from a Darius profile directory.

use crate::config_error::ConfigError;
use crate::paths::DariusPaths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProfileConfig {
    pub model: Option<ModelConfig>,
    #[serde(default)]
    pub model_overrides: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_env: Option<String>,
}

impl ProfileConfig {
    pub fn load(paths: &DariusPaths, profile: &str) -> Result<Self, ConfigError> {
        let path = Self::config_path(paths, profile)?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path).map_err(ConfigError::Read)?;
        let config: Self =
            toml::from_str(&content).map_err(|_| ConfigError::InvalidToml { path })?;
        config.validate()?;
        Ok(config)
    }

    pub fn config_path(paths: &DariusPaths, profile: &str) -> Result<PathBuf, ConfigError> {
        Ok(paths.profile(profile)?.join("config.toml"))
    }

    pub fn api_key(&self) -> Option<String> {
        let env_name = self.model.as_ref()?.api_key_env.as_deref()?;
        std::env::var(env_name).ok()
    }

    pub fn is_configured(&self) -> bool {
        if self.is_local() {
            return self.model.is_some();
        }
        self.model.is_some() && self.api_key().is_some()
    }

    pub fn is_local(&self) -> bool {
        let Some(model) = &self.model else {
            return false;
        };
        model.base_url.contains("localhost")
            || model.base_url.contains("127.0.0.1")
            || model.base_url.contains("0.0.0.0")
            || model
                .api_key_env
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case("NONE"))
    }

    pub fn get_role_model(&self, role: &str) -> Option<String> {
        self.model_overrides
            .get(role)
            .cloned()
            .or_else(|| self.model.as_ref().map(|model| model.model.clone()))
    }

    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        let Some(model) = &self.model else {
            return Ok(());
        };
        if model.provider.trim().is_empty() {
            return Err(ConfigError::EmptyProvider);
        }
        if model.model.trim().is_empty() {
            return Err(ConfigError::EmptyModel);
        }
        let url = url::Url::parse(&model.base_url).map_err(|_| ConfigError::InvalidUrlScheme)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ConfigError::InvalidUrlScheme);
        }
        if model
            .api_key_env
            .as_deref()
            .is_some_and(|name| !valid_env_name(name))
        {
            return Err(ConfigError::InvalidApiKeyEnvironment);
        }
        Ok(())
    }
}

fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
