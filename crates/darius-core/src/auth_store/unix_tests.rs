use super::*;
use serde_json::json;
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
};

#[test]
fn private_permissions_and_atomic_inode_replacement() {
    let home = tempfile::tempdir().unwrap();
    let store = AuthStore::new(home.path());
    store
        .set_provider("nous", json!({"access_token":"fixture"}))
        .unwrap();
    let path = home.path().join("auth.json");
    let original = fs::File::open(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    store
        .set_provider("nous", json!({"access_token":"rotated"}))
        .unwrap();
    assert_ne!(
        original.metadata().unwrap().ino(),
        fs::metadata(&path).unwrap().ino()
    );
    for name in ["auth.json", "auth.lock"] {
        assert_eq!(
            fs::metadata(home.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn failed_partial_temp_write_preserves_original_and_cleans_up() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("auth.json");
    fs::write(&path, b"original-fixture").unwrap();
    assert_eq!(
        atomic::replace(&path, |file| {
            assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
            file.write_all(b"partial-fixture").unwrap();
            Err(AuthStoreError::Io)
        }),
        Err(AuthStoreError::Io)
    );
    assert_eq!(fs::read(path).unwrap(), b"original-fixture");
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 1);
}

#[path = "symlink_tests.rs"]
mod symlink_tests;
