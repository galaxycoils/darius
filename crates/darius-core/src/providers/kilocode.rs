use super::{AuthType, ProviderProfile};

// Profile plugin metadata; catalog fallback where the plugin has no list.
pub(super) const PROFILE: ProviderProfile = ProviderProfile {
    id: "kilocode",
    aliases: &["kilo-code", "kilo", "kilo-gateway"],
    display_name: "Kilo Code",
    base_url: "https://api.kilo.ai/api/gateway",
    models_url: None,
    key_env: Some("KILOCODE_API_KEY"),
    auth_type: AuthType::ApiKey,
    static_headers: &[],
    fallback_models: &[
        "anthropic/claude-opus-4.6",
        "anthropic/claude-sonnet-4.6",
        "openai/gpt-5.4",
        "google/gemini-3-pro-preview",
        "google/gemini-3-flash-preview",
    ],
    default_model: "google/gemini-3.6-flash",
    default_max_tokens: None,
    strict_string_tool_content: false,
    strip_tool_message_names: false,
    omit_authorization: false,
};
