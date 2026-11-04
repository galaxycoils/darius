use super::*;

#[test]
fn request_headers_have_darius_attribution_and_version() {
    for id in ["opencode-go", "opencode-free", "openrouter"] {
        let headers = resolve(id).unwrap().request_headers();
        assert!(headers.contains(&("X-Title".into(), "Darius".into())));
        assert!(headers.contains(&(
            "HTTP-Referer".into(),
            "https://github.com/galaxycoils/darius".into()
        )));
        assert!(headers.contains(&(
            "User-Agent".into(),
            format!("Darius/{}", env!("CARGO_PKG_VERSION"))
        )));
    }
}

#[test]
fn all_six_profiles_resolve_ids_and_every_alias() {
    assert_eq!(all_profiles().len(), 6);
    for profile in all_profiles() {
        assert!(std::ptr::eq(resolve(profile.id).unwrap(), profile));
        for alias in profile.aliases {
            assert!(std::ptr::eq(resolve(alias).unwrap(), profile));
        }
    }
    assert!(resolve("opencode-zen").is_none());
    assert!(resolve("unknown").is_none());
}

#[test]
fn exactly_one_keyless_profile() {
    let keyless: Vec<_> = all_profiles()
        .iter()
        .filter(|p| p.auth_type == AuthType::Keyless)
        .collect();
    assert_eq!(keyless.len(), 1);
    assert_eq!(keyless[0].id, "opencode-free");
    for p in all_profiles() {
        assert_eq!(p.key_env.is_none(), p.auth_type == AuthType::Keyless);
        assert_eq!(p.omit_authorization, p.auth_type == AuthType::Keyless);
    }
}
