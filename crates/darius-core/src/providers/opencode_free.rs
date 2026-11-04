use super::profile::ATTRIBUTION;
use super::{AuthType, ProviderProfile};

// Profile plugin metadata; catalog fallback where the plugin has no list.
pub(super) const PROFILE: ProviderProfile = ProviderProfile {
    id: "opencode-free",
    aliases: &["free", "opencode_free"],
    display_name: "OpenCode Free",
    base_url: "https://opencode.ai/zen/v1",
    models_url: None,
    key_env: None,
    auth_type: AuthType::Keyless,
    static_headers: ATTRIBUTION,
    fallback_models: &[
        "deepseek-v4-flash-free",
        "hy3-free",
        "mimo-v2.5-free",
        "laguna-s-2.1-free",
        "nemotron-3-ultra-free",
        "nemotron-3.5-lightning-free",
        "muse-spark-1.2-contributor-free",
        "muse-spark-1.3-contributor-free",
    ],
    default_model: "laguna-s-2.1-free",
    default_max_tokens: None,
    strict_string_tool_content: false,
    strip_tool_message_names: false,
    omit_authorization: true,
};
