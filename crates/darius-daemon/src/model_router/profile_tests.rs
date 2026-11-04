use super::{LiveModel, Provider, live_factory::LiveBackend};
use darius_core::providers::{all_profiles, resolve};

#[test]
fn configured_provider_delegates_and_attaches_registry_metadata() {
    for profile in all_profiles() {
        for name in std::iter::once(&profile.id).chain(profile.aliases) {
            let mut provider = Provider::from_profile(name).unwrap();
            assert!(std::ptr::eq(provider.profile().unwrap(), profile));
            assert_eq!(provider.base_url, profile.base_url);
            assert_eq!(provider.api_key_env, profile.key_env.unwrap_or("NONE"));
            assert_eq!(provider.model, profile.default_model);
            provider.base_url = "http://localhost:1/v1".into();
            provider.model = "claude-custom".into();
            let live = LiveModel::for_provider(provider).unwrap();
            let LiveBackend::ChatCompletions(model) = live.backend else {
                panic!("compatible profile misrouted");
            };
            assert!(std::ptr::eq(model.profile.unwrap(), resolve(name).unwrap()));
            assert_eq!(model.model, "claude-custom");
            assert_eq!(model.base_url, "http://localhost:1/v1");
        }
    }
}
