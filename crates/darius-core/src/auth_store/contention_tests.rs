use super::*;
use serde_json::json;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn subprocess_writer() {
    let Some(home) = std::env::var_os("DARIUS_AUTH_STORE_TEST_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    std::fs::write(home.join("ready"), b"ready").unwrap();
    let store = AuthStore::new(home);
    for _ in 0..20 {
        store
            .update_provider("counter", |old| {
                let n = old.and_then(|v| v["n"].as_u64()).unwrap_or(0);
                Ok(Some(json!({"n":n+1})))
            })
            .unwrap();
    }
}

#[test]
fn cross_process_contention_blocks_then_preserves_every_update() {
    let home = tempfile::tempdir().unwrap();
    let guard = lock::acquire(home.path()).unwrap();
    let mut children = Vec::new();
    for _ in 0..4 {
        children.push(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "auth_store::contention_tests::subprocess_writer"])
                .env("DARIUS_AUTH_STORE_TEST_HOME", home.path())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !home.path().join("ready").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready = home.path().join("ready").exists();
    std::thread::sleep(Duration::from_millis(100));
    let blocked = children
        .iter_mut()
        .all(|child| child.try_wait().unwrap().is_none());
    let absent = !home.path().join("auth.json").exists();
    drop(guard);
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    assert!(ready && blocked && absent, "writer did not block on flock");
    assert_eq!(
        AuthStore::new(home.path()).provider("counter").unwrap(),
        Some(json!({"n":80}))
    );
}
