//! Validated configuration loaded from a Darius profile directory.

use crate::config_error::ConfigError;
use crate::paths::DariusPaths;
use darius_tools::{McpServerEntry, McpTransportConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub auth_type: Option<darius_core::providers::AuthType>,
    #[serde(default)]
    pub default_max_tokens: Option<u32>,
    #[serde(default)]
    pub context_length: Option<u32>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OpenRouterConfig {
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub only: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub order: Vec<String>,
}

impl OpenRouterConfig {
    /// Thread the `[openrouter]` section to the transport (3.6): `None`
    /// when the section carries no prefs, so no `provider_routing` key is
    /// sent at all. The transport still gates on the openrouter profile
    /// id — this value is dropped for any other provider at body-build.
    pub fn provider_routing(&self) -> Option<darius_daemon::ProviderRouting> {
        let routing = darius_daemon::ProviderRouting {
            sort: self.sort.clone(),
            only: self.only.clone(),
            ignore: self.ignore.clone(),
            order: self.order.clone(),
        };
        (!routing.is_empty()).then_some(routing)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NousConfig {
    #[serde(default = "default_keepalive_interval")]
    pub keepalive_interval_seconds: u64,
}

fn default_keepalive_interval() -> u64 {
    900
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProfileConfig {
    pub model: Option<ModelConfig>,
    #[serde(default)]
    pub model_overrides: HashMap<String, String>,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerEntry>,
    #[serde(default)]
    pub openrouter: Option<OpenRouterConfig>,
    #[serde(default)]
    pub nous: NousConfig,
}

impl ProfileConfig {
    pub fn load(paths: &DariusPaths, profile: &str) -> Result<Self, ConfigError> {
        let path = Self::config_path(paths, profile)?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path).map_err(ConfigError::Read)?;
        let mut config: Self =
            toml::from_str(&content).map_err(|_| ConfigError::InvalidToml { path })?;
        if config.mcp_servers.is_empty() && !config.mcp.servers.is_empty() {
            config.mcp_servers = config.mcp.servers.clone();
        } else if !config.mcp_servers.is_empty() && config.mcp.servers.is_empty() {
            config.mcp.servers = config.mcp_servers.clone();
        }
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

    pub fn mcp_servers(&self) -> Vec<McpServerEntry> {
        if !self.mcp.servers.is_empty() {
            self.mcp.servers.clone()
        } else {
            self.mcp_servers.clone()
        }
    }

    pub fn openai_deprecation(&self) -> Option<String> {
        let model = self.model.as_ref()?;
        if model.provider != "openai_compatible" {
            return None;
        }
        let base = url::Url::parse(&model.base_url).ok()?;
        if base.host_str() == Some("api.openai.com") {
            Some(
                "provider \"openai_compatible\" with base_url host api.openai.com is deprecated; migrate to a named provider (see darius config init)"
                    .to_string(),
            )
        } else {
            None
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if let Some(model) = &self.model {
            if model.provider.trim().is_empty() {
                return Err(ConfigError::EmptyProvider);
            }
            if model.model.trim().is_empty() {
                return Err(ConfigError::EmptyModel);
            }
            let url =
                url::Url::parse(&model.base_url).map_err(|_| ConfigError::InvalidUrlScheme)?;
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
        }
        for server in self.mcp_servers() {
            if server.name.trim().is_empty()
                || !server
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(ConfigError::InvalidMcpServerName(server.name.clone()));
            }
            match &server.transport {
                McpTransportConfig::Stdio { command, .. } => {
                    if command.trim().is_empty() {
                        return Err(ConfigError::EmptyMcpServerCommand(server.name.clone()));
                    }
                }
                McpTransportConfig::Sse { url, .. } => {
                    let parsed = url::Url::parse(url)
                        .map_err(|_| ConfigError::InvalidMcpServerUrl(server.name.clone()))?;
                    if !matches!(parsed.scheme(), "http" | "https") {
                        return Err(ConfigError::InvalidMcpServerUrl(server.name.clone()));
                    }
                }
            }
        }
        Ok(())
    }
}

fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_section_converts_to_transport_routing() {
        let cfg = OpenRouterConfig {
            sort: Some("throughput".into()),
            only: vec!["lambda".into()],
            ignore: vec![],
            order: vec!["lambda".into(), "deepinfra".into()],
        };
        let routing = cfg.provider_routing().expect("non-empty section converts");
        assert_eq!(routing.sort.as_deref(), Some("throughput"));
        assert_eq!(routing.only, vec!["lambda"]);
        assert!(routing.ignore.is_empty());
        assert_eq!(routing.order, vec!["lambda", "deepinfra"]);
    }

    #[test]
    fn empty_openrouter_section_converts_to_none() {
        assert!(OpenRouterConfig::default().provider_routing().is_none());
        let blank_sort = OpenRouterConfig {
            sort: Some(String::new()),
            ..OpenRouterConfig::default()
        };
        assert!(blank_sort.provider_routing().is_none());
    }
}
