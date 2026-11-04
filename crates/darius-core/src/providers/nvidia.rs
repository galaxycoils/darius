use super::{AuthType, ProviderProfile};

// Profile plugin metadata; catalog fallback where the plugin has no list.
pub(super) const PROFILE: ProviderProfile = ProviderProfile {
    id: "nvidia",
    aliases: &["nvidia-nim"],
    display_name: "NVIDIA NIM",
    base_url: "https://integrate.api.nvidia.com/v1",
    models_url: None,
    key_env: Some("NVIDIA_API_KEY"),
    auth_type: AuthType::ApiKey,
    static_headers: &[],
    fallback_models: &[
        "nvidia/llama-3.1-nemotron-70b-instruct",
        "nvidia/llama-3.3-70b-instruct",
    ],
    default_model: "nvidia/llama-3.1-nemotron-70b-instruct",
    default_max_tokens: Some(16384),
    strict_string_tool_content: false,
    strip_tool_message_names: true,
    omit_authorization: false,
};
