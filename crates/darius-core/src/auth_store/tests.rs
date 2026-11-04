use super::*;
use serde_json::json;

#[test]
fn provider_roundtrip_and_removal_preserve_other_state() {
    let home = tempfile::tempdir().unwrap();
    let store = AuthStore::new(home.path());
    assert_eq!(store.provider("nous").unwrap(), None);
    let token = json!({"access_token":"fixture", "refresh_token":"refresh-fixture",
        "expires_at":1234, "scope":"inference:invoke", "quarantined":false});
    store.set_provider("nous", token.clone()).unwrap();
    store
        .set_provider("other", json!({"api_key":"fixture-key"}))
        .unwrap();
    assert_eq!(store.provider("nous").unwrap(), Some(token));
    store.remove_provider("nous").unwrap();
    assert!(store.provider("nous").unwrap().is_none());
    assert!(store.provider("other").unwrap().is_some());
}

#[test]
fn aborted_update_preserves_original_bytes() {
    let home = tempfile::tempdir().unwrap();
    let store = AuthStore::new(home.path());
    store
        .set_provider("nous", json!({"access_token":"fixture"}))
        .unwrap();
    let path = home.path().join("auth.json");
    let before = std::fs::read(&path).unwrap();
    assert!(
        store
            .update_provider("nous", |_| Err(AuthStoreError::InvalidState))
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn malformed_store_is_not_overwritten_and_errors_are_secret_safe() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("auth.json");
    let before = br#"{"providers":"secret-fixture"}"#;
    std::fs::write(&path, before).unwrap();
    let err = AuthStore::new(home.path())
        .set_provider("nous", json!({}))
        .unwrap_err();
    assert!(!format!("{err:?} {err}").contains("secret-fixture"));
    assert_eq!(std::fs::read(path).unwrap(), before);
}
