/// Model provider.
#[derive(Debug, Clone)]
pub struct Provider {
    pub name: String,
    pub model: String,
    pub base_url: String,
    pub enabled: bool,
    /// Environment variable name holding the API key. Never store the key itself.
    pub api_key_env: String,
}

impl Provider {
    pub(super) fn validate(&self) -> Result<(), super::RouterError> {
        if [&self.name, &self.model, &self.base_url, &self.api_key_env]
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(super::RouterError::Provider(
                "provider config incomplete".into(),
            ));
        }
        Ok(())
    }

    /// Known provider metadata is owned exclusively by the core registry.
    /// Fields above are explicit runtime overrides, not another profile table.
    pub fn profile(&self) -> Option<&'static darius_core::providers::ProviderProfile> {
        darius_core::providers::resolve(&self.name)
    }

    /// Start from registry defaults; legacy struct literals remain supported.
    pub fn from_profile(id_or_alias: &str) -> Option<Self> {
        let p = darius_core::providers::resolve(id_or_alias)?;
        Some(Self {
            name: p.id.into(),
            model: p.default_model.into(),
            base_url: p.base_url.into(),
            enabled: true,
            api_key_env: p.key_env.unwrap_or("NONE").into(),
        })
    }
}
