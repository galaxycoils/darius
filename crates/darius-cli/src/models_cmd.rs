//! `darius models [--provider]` — list models with source annotation.
//!
//! The provider resolves from the explicit `--provider` flag (via
//! `darius_core::providers::resolve`) or, when absent, from the active
//! profile's configured provider. The endpoint is the profile's `models_url`
//! or `{base_url}/models`; the bearer comes from the existing credential
//! paths (environment key after dotenv merge; keyless profiles send none).
//! Every printed line carries a source annotation: live, cached, or fallback.

use crate::config::ProfileConfig;
use crate::paths::DariusPaths;
use darius_core::providers::ProviderProfile;
use darius_daemon::{CatalogRequest, CatalogResponse, CatalogSource, ModelCatalog};

/// Valid provider ids for error messages, in registry order.
pub fn valid_providers() -> String {
    darius_core::providers::all_profiles()
        .iter()
        .map(|p| p.id)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolve the target profile: explicit `--provider` wins, otherwise the
/// active profile's configured provider. Never invents a default.
fn resolve_target_provider(
    config: &ProfileConfig,
    provider_override: Option<&str>,
) -> Result<&'static ProviderProfile, String> {
    if let Some(id) = provider_override {
        return darius_core::providers::resolve(id).ok_or_else(|| {
            format!(
                "Unknown provider '{id}'. Valid providers: {}",
                valid_providers()
            )
        });
    }
    match config.model.as_ref() {
        Some(m) => darius_core::providers::resolve(&m.provider).ok_or_else(|| {
            format!(
                "Configured provider '{}' is not a known provider. Valid providers: {}",
                m.provider,
                valid_providers()
            )
        }),
        None => Err(
            "No model configured for this profile. Run `darius config setup` first.".to_string(),
        ),
    }
}

/// Endpoint for a listing: the profile's `models_url`, else `{base_url}/models`
/// where the base prefers the stored config when it matches the provider.
fn endpoint_for(
    profile: &ProviderProfile,
    config: &ProfileConfig,
    provider_override: Option<&str>,
) -> String {
    if let Some(url) = profile.models_url {
        return url.to_string();
    }
    let base = match (&config.model, provider_override) {
        (Some(m), None) => m.base_url.clone(),
        (Some(m), Some(_)) if m.provider == profile.id => m.base_url.clone(),
        _ => profile.base_url.to_string(),
    };
    format!("{}/models", base.trim_end_matches('/'))
}

/// Bearer for a listing: `None` for keyless profiles, otherwise the
/// non-blank environment value (dotenv files are merged beforehand).
fn bearer_for(profile: &ProviderProfile, config: &ProfileConfig) -> Option<String> {
    if profile.auth_type == darius_core::providers::AuthType::Keyless || profile.key_env.is_none() {
        return None;
    }
    let key_env = config
        .model
        .as_ref()
        .and_then(|m| m.api_key_env.clone())
        .filter(|v| !v.eq_ignore_ascii_case("NONE") && !v.trim().is_empty())
        .or_else(|| profile.key_env.map(str::to_string))?;
    std::env::var(&key_env)
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// Fetch one catalog page. Pure async core so tests can drive it directly.
async fn fetch_catalog(
    home: &std::path::Path,
    profile: &ProviderProfile,
    endpoint: &str,
    bearer: Option<&str>,
    headers: &[(&str, &str)],
) -> CatalogResponse {
    let catalog = match ModelCatalog::new(home) {
        Ok(c) => c,
        Err(_) => return CatalogResponse::default(),
    };
    let req = CatalogRequest {
        profile,
        endpoint,
        bearer_token: bearer,
        headers,
        context_length: None,
    };
    catalog.fetch_models(&req).await
}

/// One printed model line: id + context length + source annotation.
pub fn format_model_line(id: &str, context_length: Option<u32>, source: CatalogSource) -> String {
    let ctx = context_length
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let source = match source {
        CatalogSource::Live => "live",
        CatalogSource::Cached => "cached",
        CatalogSource::Fallback => "fallback",
    };
    format!("{id}  context_length={ctx}  [source: {source}]")
}

/// Empty live/cached results degrade to the registry fallback list so the
/// command always shows something selectable.
fn entries_or_fallback(
    profile: &'static ProviderProfile,
    resp: &CatalogResponse,
) -> (Vec<(String, Option<u32>)>, CatalogSource) {
    if resp.models.is_empty() {
        let entries = profile
            .fallback_models
            .iter()
            .map(|m| (m.to_string(), None))
            .collect();
        (entries, CatalogSource::Fallback)
    } else {
        let entries = resp
            .models
            .iter()
            .map(|m| (m.id.clone(), m.context_length))
            .collect();
        (entries, resp.source)
    }
}

pub fn run_models(
    paths: &DariusPaths,
    profile_name: &str,
    provider_override: Option<&str>,
) -> Result<(), String> {
    crate::runtime_selector::load_dotenv_if_present(paths);
    let config = ProfileConfig::load(paths, profile_name)
        .map_err(|e| format!("failed to load config: {e}"))?;
    let target = resolve_target_provider(&config, provider_override)?;
    let endpoint = endpoint_for(target, &config, provider_override);
    let bearer = bearer_for(target, &config);
    let owned_headers: Vec<(String, String)> = target.request_headers().into_iter().collect();
    let header_refs: Vec<(&str, &str)> = owned_headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let resp = crate::runtime::block_on_turn(fetch_catalog(
        &paths.home,
        target,
        &endpoint,
        bearer.as_deref(),
        &header_refs,
    ));
    let (entries, source) = entries_or_fallback(target, &resp);
    println!("Provider: {} ({})", target.id, target.display_name);
    println!("Endpoint: {endpoint}");
    for (id, ctx) in &entries {
        println!("{}", format_model_line(id, *ctx, source));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn models_live_lists_with_source_annotation() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "demo-1", "context_length": 8192},
                    {"id": "demo-2"}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let profile = darius_core::providers::resolve("nous").unwrap();
        let endpoint = format!("{}/models", server.uri());
        let home = tempfile::TempDir::new().unwrap();
        let resp = fetch_catalog(home.path(), profile, &endpoint, Some("k"), &[]).await;
        assert_eq!(resp.source, CatalogSource::Live);
        assert_eq!(resp.models.len(), 2);
        let line = format_model_line(
            &resp.models[0].id,
            resp.models[0].context_length,
            resp.source,
        );
        assert!(line.contains("demo-1"), "{line}");
        assert!(line.contains("8192"), "{line}");
        assert!(line.contains("[source: live]"), "{line}");
        let line2 = format_model_line(
            &resp.models[1].id,
            resp.models[1].context_length,
            resp.source,
        );
        assert!(line2.contains("unknown"), "{line2}");
        assert!(line2.contains("[source: live]"), "{line2}");
        server.verify().await;
    }

    #[tokio::test]
    async fn models_fallback_lists_registry_entries() {
        let profile = darius_core::providers::resolve("nous").unwrap();
        let home = tempfile::TempDir::new().unwrap();
        let resp = fetch_catalog(
            home.path(),
            profile,
            "http://127.0.0.1:1/invalid/models",
            None,
            &[],
        )
        .await;
        assert_eq!(resp.source, CatalogSource::Fallback);
        assert!(resp.models.is_empty());
        let (entries, source) = entries_or_fallback(profile, &resp);
        assert!(!entries.is_empty());
        assert_eq!(entries.len(), profile.fallback_models.len());
        let line = format_model_line(&entries[0].0, entries[0].1, source);
        assert!(line.contains("[source: fallback]"), "{line}");
    }

    #[test]
    fn models_unknown_provider_names_valid_choices() {
        let config = ProfileConfig::default();
        let err = resolve_target_provider(&config, Some("bogus")).unwrap_err();
        assert!(err.contains("Unknown provider 'bogus'"), "{err}");
        assert!(err.contains("opencode-free"), "{err}");
    }

    #[test]
    fn models_without_config_points_at_setup() {
        let config = ProfileConfig::default();
        let err = resolve_target_provider(&config, None).unwrap_err();
        assert!(err.contains("darius config setup"), "{err}");
    }

    #[test]
    fn models_prefers_models_url_when_present() {
        let config = ProfileConfig::default();
        let profile = darius_core::providers::resolve("openrouter").unwrap();
        assert_eq!(
            endpoint_for(profile, &config, Some("openrouter")),
            "https://openrouter.ai/api/v1/models"
        );
    }

    #[test]
    fn models_keyless_bearer_is_none() {
        let config = ProfileConfig::default();
        let profile = darius_core::providers::resolve("opencode-free").unwrap();
        assert_eq!(bearer_for(profile, &config), None);
    }
}
