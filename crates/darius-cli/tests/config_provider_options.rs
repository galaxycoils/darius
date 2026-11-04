use darius_cli::ProfileConfig;
use darius_core::config::ModelConfig;

#[test]
fn provider_options_round_trip_both_schemas() {
    let text = r#"[model]
provider = "openrouter"
base_url = "https://openrouter.ai/api/v1"
model = "test"
api_key_env = "NONE"
auth_type = "api_key"
default_max_tokens = 16384
context_length = 131072
[model.headers]
X-Test = "value"
[openrouter]
sort = "latency"
only = ["one"]
ignore = ["two"]
order = ["one", "three"]
[nous]
keepalive_interval_seconds = 90
"#;
    let cli: ProfileConfig = toml::from_str(text).unwrap();
    let model = cli.model.as_ref().unwrap();
    assert_eq!(model.default_max_tokens, Some(16384));
    assert_eq!(model.context_length, Some(131072));
    let core: ModelConfig = toml::from_str(&toml::to_string(model).unwrap()).unwrap();
    assert_eq!(core.headers.as_ref().unwrap()["X-Test"], "value");
    assert_eq!(cli.nous.keepalive_interval_seconds, 90);
    assert_eq!(cli.openrouter.unwrap().only, ["one"]);
}

#[test]
fn legacy_model_defaults_are_optional() {
    let core: ModelConfig = toml::from_str(
        "provider='openai_compatible'\nbase_url='https://api.openai.com/v1'\nmodel='old'",
    )
    .unwrap();
    assert_eq!(core.api_key_env, "");
    assert!(core.auth_type.is_none());
    assert!(core.headers.is_none());
}
