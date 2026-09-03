use darius_cli::paths::DariusPaths;
use darius_cli::{initialize_profile, ConfigError, ProfileConfig, ProviderMetadata};
use std::fs;
use tempfile::TempDir;

fn paths(temp: &TempDir) -> DariusPaths {
    let home = temp.path().join("isolated-darius-home");
    let workspace = temp.path().join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    DariusPaths { home, workspace }
}

fn metadata() -> ProviderMetadata {
    ProviderMetadata {
        provider: "openai_compatible".into(),
        base_url: "https://api.example.test/v1".into(),
        model: "test-model".into(),
        api_key_env: Some("DARIUS_TEST_API_KEY".into()),
    }
}

#[test]
fn config_missing_file_returns_default_setup_state() {
    let temp = TempDir::new().unwrap();
    let config = ProfileConfig::load(&paths(&temp), "default").unwrap();
    assert!(config.model.is_none());
    assert!(!config.is_configured());
}

#[test]
fn config_malformed_toml_is_visible_error() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let profile = paths.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("config.toml"), "[model\nprovider = \"x\"").unwrap();

    assert!(matches!(
        ProfileConfig::load(&paths, "default"),
        Err(ConfigError::InvalidToml { .. })
    ));
}

#[test]
fn config_invalid_provider_is_visible_error() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let profile = paths.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"   \"\nbase_url = \"https://api.example.test\"\nmodel = \"test\"",
    )
    .unwrap();

    assert!(matches!(
        ProfileConfig::load(&paths, "default"),
        Err(ConfigError::EmptyProvider)
    ));
}

#[test]
fn config_invalid_model_is_visible_error() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let profile = paths.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"provider\"\nbase_url = \"https://api.example.test\"\nmodel = \"\"",
    )
    .unwrap();

    assert!(matches!(
        ProfileConfig::load(&paths, "default"),
        Err(ConfigError::EmptyModel)
    ));
}

#[test]
fn config_invalid_url_scheme_is_visible_error() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let profile = paths.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"provider\"\nbase_url = \"file:///tmp/provider\"\nmodel = \"test\"",
    )
    .unwrap();

    assert!(matches!(
        ProfileConfig::load(&paths, "default"),
        Err(ConfigError::InvalidUrlScheme)
    ));
}

#[test]
fn config_invalid_api_key_environment_variable_is_visible_error() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let profile = paths.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"provider\"\nbase_url = \"https://api.example.test\"\nmodel = \"test\"\napi_key_env = \"NOT-VALID\"",
    )
    .unwrap();

    assert!(matches!(
        ProfileConfig::load(&paths, "default"),
        Err(ConfigError::InvalidApiKeyEnvironment)
    ));
}

#[test]
fn config_invalid_profile_is_path_error() {
    let temp = TempDir::new().unwrap();
    assert!(matches!(
        ProfileConfig::load(&paths(&temp), "../default"),
        Err(ConfigError::Path(_))
    ));
}

#[test]
fn config_init_writes_metadata_without_secret() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let path = initialize_profile(&paths, "default", &metadata(), false).unwrap();
    let content = fs::read_to_string(path).unwrap();

    assert!(content.contains("api_key_env = \"DARIUS_TEST_API_KEY\""));
    assert!(!content.contains("api_key ="));
    assert!(!content.contains("super-secret-value"));
}

#[test]
fn config_init_rejects_overwrite_without_force() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    initialize_profile(&paths, "default", &metadata(), false).unwrap();

    assert!(matches!(
        initialize_profile(&paths, "default", &metadata(), false),
        Err(ConfigError::AlreadyExists { .. })
    ));
}

#[cfg(unix)]
#[test]
fn config_init_writes_mode_0600() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let path = initialize_profile(&paths(&temp), "default", &metadata(), false).unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn config_init_is_atomic_and_leaves_no_temporary_file() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let path = initialize_profile(&paths, "default", &metadata(), false).unwrap();
    let entries: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();

    assert_eq!(entries, vec!["config.toml"]);
}

#[test]
fn config_init_uses_isolated_home_and_profile() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let path = initialize_profile(&paths, "isolated-profile", &metadata(), false).unwrap();

    assert_eq!(
        path,
        temp.path()
            .join("isolated-darius-home/profiles/isolated-profile/config.toml")
    );
    assert!(path.exists());
}

#[test]
fn config_read_failure_is_visible_and_sanitized() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let profile = paths.profile("default").unwrap();
    fs::create_dir_all(profile.join("config.toml")).unwrap();

    let error = ProfileConfig::load(&paths, "default").unwrap_err();
    assert!(matches!(error, ConfigError::Read(_)));
    assert!(!error.to_string().contains("DARIUS_TEST_SECRET_VALUE"));
}

#[test]
fn config_parse_error_does_not_disclose_file_content() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let profile = paths.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "super-secret-value = \"DARIUS_TEST_SECRET_VALUE\"\n[model",
    )
    .unwrap();

    let error = ProfileConfig::load(&paths, "default").unwrap_err();
    assert!(matches!(error, ConfigError::InvalidToml { .. }));
    assert!(!error.to_string().contains("DARIUS_TEST_SECRET_VALUE"));
}

#[test]
fn config_init_force_replaces_existing_metadata_without_temp_artifacts() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    initialize_profile(&paths, "default", &metadata(), false).unwrap();
    let replacement = ProviderMetadata {
        model: "replacement-model".into(),
        ..metadata()
    };
    let path = initialize_profile(&paths, "default", &replacement, true).unwrap();
    let content = fs::read_to_string(&path).unwrap();
    let entries: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();

    assert!(content.contains("replacement-model"));
    assert_eq!(entries, vec!["config.toml"]);
}
