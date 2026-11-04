use super::{LiveModel, Provider};

#[test]
fn claude_on_compatible_provider_uses_chat_completions() {
    for name in ["openrouter", "openai_compatible", "ollama"] {
        let live = LiveModel::for_provider(Provider {
            name: name.into(),
            model: "claude-sonnet-4.6".into(),
            base_url: "http://localhost:1/v1".into(),
            api_key_env: "NONE".into(),
            enabled: true,
        })
        .unwrap();
        assert!(!live.is_anthropic(), "{name} misrouted");
    }
}

#[test]
fn anthropic_url_does_not_select_native_wire() {
    let live = LiveModel::for_provider(Provider {
        name: "custom".into(),
        model: "custom-model".into(),
        base_url: "https://api.anthropic.com/v1".into(),
        api_key_env: "NONE".into(),
        enabled: true,
    })
    .unwrap();
    assert!(!live.is_anthropic());
}

#[test]
fn explicit_anthropic_selects_native_wire() {
    let live = LiveModel::for_provider(Provider {
        name: "anthropic".into(),
        model: "custom-model".into(),
        base_url: "http://localhost:1/v1".into(),
        api_key_env: "NONE".into(),
        enabled: true,
    })
    .unwrap();
    assert!(live.is_anthropic());
}
