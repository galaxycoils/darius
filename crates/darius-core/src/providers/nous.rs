use super::{AuthType, ProviderProfile};

// Profile plugin metadata; catalog fallback where the plugin has no list.
pub(super) const PROFILE: ProviderProfile = ProviderProfile {
    id: "nous",
    aliases: &["nous-portal", "nousresearch"],
    display_name: "Nous Research",
    base_url: "https://inference-api.nousresearch.com/v1",
    models_url: None,
    key_env: Some("NOUS_API_KEY"),
    auth_type: AuthType::NousOAuth,
    static_headers: &[],
    fallback_models: &["hermes-3-405b", "hermes-3-70b"],
    default_model: "z-ai/glm-5.2",
    default_max_tokens: None,
    strict_string_tool_content: false,
    strip_tool_message_names: false,
    omit_authorization: false,
};
