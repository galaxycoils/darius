use super::super::*;
use serde_json::json;
use std::{fs, os::unix::fs::symlink};

#[test]
fn symlinked_auth_or_lock_is_rejected_without_modifying_target() {
    for name in ["auth.json", "auth.lock"] {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("target");
        fs::write(&target, b"untouched-fixture").unwrap();
        symlink(&target, home.path().join(name)).unwrap();
        assert!(
            AuthStore::new(home.path())
                .set_provider("nous", json!({}))
                .is_err()
        );
        assert_eq!(fs::read(target).unwrap(), b"untouched-fixture");
    }
}
