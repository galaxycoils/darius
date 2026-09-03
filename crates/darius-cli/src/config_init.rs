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
