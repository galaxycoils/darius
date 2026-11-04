//! Declarative metadata; wire/auth behavior is applied by transports separately.
//! Ported from NousResearch/hermes-agent @ a6102b8d809d6b32824757b547b60f9d22d6a88e:
//! providers/base.py, plugins/model-providers/*, hermes_cli/models_catalog_static.py.
mod kilocode;
mod nous;
mod nvidia;
mod opencode_free;
mod opencode_go;
mod openrouter;
mod profile;
pub use profile::{AuthType, ProviderProfile};

static PROFILES: [ProviderProfile; 6] = [
    kilocode::PROFILE,
    opencode_go::PROFILE,
    opencode_free::PROFILE,
    nous::PROFILE,
    openrouter::PROFILE,
    nvidia::PROFILE,
];

pub fn all_profiles() -> &'static [ProviderProfile] {
    &PROFILES
}

pub fn resolve(id_or_alias: &str) -> Option<&'static ProviderProfile> {
    PROFILES
        .iter()
        .find(|p| p.id == id_or_alias || p.aliases.contains(&id_or_alias))
}
#[cfg(test)]
mod tests;
