use crate::config::ModelConfig;

#[derive(Clone, Debug)]
pub(crate) struct ProviderSelection {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub key_env: String,
}

impl ProviderSelection {
    pub fn config(&self) -> ModelConfig {
        ModelConfig {
            provider: self.provider.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            api_key_env: Some(self.key_env.clone()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum RuntimeState {
    OfflineDemo,
    Setup,
    MissingKey(ProviderSelection),
    Live(ProviderSelection),
}

impl RuntimeState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::OfflineDemo => "offline-demo",
            Self::Setup => "setup",
            Self::MissingKey(_) => "missing-key",
            Self::Live(_) => "live",
        }
    }
}
