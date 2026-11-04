use darius_cli::ProfileConfig;
use darius_cli::paths::DariusPaths;
use std::fs;

fn paths(temp: &std::path::Path, home_sub: &str) -> DariusPaths {
    let home = temp.join(home_sub);
    let workspace = temp.join("workspace");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&workspace).unwrap();
    DariusPaths { home, workspace }
}

#[test]
fn config_deprecation_fires_only_for_openai_host() {
    let temp = tempfile::TempDir::new().unwrap();
    let p = paths(temp.path(), "home-openai");
    let profile = p.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"openai_compatible\"\nbase_url = \"https://api.openai.com/v1\"\nmodel = \"gpt-4o-mini\"",
    )
    .unwrap();
    let cfg = ProfileConfig::load(&p, "default").unwrap();
    assert!(cfg.openai_deprecation().is_some());
}

#[test]
fn config_deprecation_absent_for_other_openai_compatible_hosts() {
    let temp = tempfile::TempDir::new().unwrap();
    let p = paths(temp.path(), "home-other");
    let profile = p.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"openai_compatible\"\nbase_url = \"https://api.example.test/v1\"\nmodel = \"test\"\napi_key_env = \"DARIUS_API_KEY\"",
    )
    .unwrap();
    let cfg = ProfileConfig::load(&p, "default").unwrap();
    assert!(cfg.openai_deprecation().is_none());
}

#[test]
fn nous_keepalive_default_and_override_round_trip() {
    let temp = tempfile::TempDir::new().unwrap();
    let p = paths(temp.path(), "home-nous");
    let profile = p.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"nous\"\nbase_url = \"https://inference-api.nousresearch.com/v1\"\nmodel = \"m\"\napi_key_env = \"NOUS_API_KEY\"\n[nous]\nkeepalive_interval_seconds = 300",
    )
    .unwrap();
    let cfg = ProfileConfig::load(&p, "default").unwrap();
    assert_eq!(cfg.nous.keepalive_interval_seconds, 300);
}

#[test]
fn openrouter_routing_section_round_trip() {
    let temp = tempfile::TempDir::new().unwrap();
    let p = paths(temp.path(), "home-openrouter");
    let profile = p.profile("default").unwrap();
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[model]\nprovider = \"openrouter\"\nbase_url = \"https://openrouter.ai/api/v1\"\nmodel = \"test\"\napi_key_env = \"OPENROUTER_API_KEY\"\n[openrouter]\nsort = \"latency\"\nonly = [\"a\"]\nignore = [\"b\"]\norder = [\"a\", \"c\"]",
    )
    .unwrap();
    let cfg = ProfileConfig::load(&p, "default").unwrap();
    let or = cfg.openrouter.unwrap();
    assert_eq!(or.sort.as_deref(), Some("latency"));
    assert_eq!(or.only, ["a"]);
    assert_eq!(or.ignore, ["b"]);
    assert_eq!(or.order, ["a", "c"]);
}
