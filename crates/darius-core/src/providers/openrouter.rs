use super::profile::ATTRIBUTION;
use super::{AuthType, ProviderProfile};

// Profile plugin metadata; catalog fallback where the plugin has no list.
pub(super) const PROFILE: ProviderProfile = ProviderProfile {
    id: "openrouter",
    aliases: &["or"],
    display_name: "OpenRouter",
    base_url: "https://openrouter.ai/api/v1",
    models_url: Some("https://openrouter.ai/api/v1/models"),
    key_env: Some("OPENROUTER_API_KEY"),
    auth_type: AuthType::ApiKey,
    static_headers: ATTRIBUTION,
    fallback_models: &[
        "anthropic/claude-sonnet-4.6",
        "openai/gpt-5.4",
        "deepseek/deepseek-chat",
        "google/gemini-3.8-flash",
        "google/gemini-3.7-flash",
        "qwen/qwen3-plus",
    ],
    default_model: "z-ai/glm-5.2",
    default_max_tokens: None,
    strict_string_tool_content: false,
    strip_tool_message_names: false,
    omit_authorization: false,
};
