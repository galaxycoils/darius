use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use darius_cli::paths::{DariusPaths, Env, PathError};

#[derive(Default)]
struct FakeEnv {
    vars: HashMap<&'static str, OsString>,
    home: Option<PathBuf>,
}

impl Env for FakeEnv {
    fn var_os(&self, key: &str) -> Option<OsString> {
        self.vars.get(key).cloned()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home.clone()
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().expect("test path should canonicalize")
}

#[test]
fn paths_override_uses_canonical_temp_home_and_workspace() {
    let home = tempfile::tempdir().expect("temp home");
    let workspace = tempfile::tempdir().expect("temp workspace");
    let env = FakeEnv {
        vars: [("DARIUS_HOME", home.path().as_os_str().to_owned())]
            .into_iter()
            .collect(),
        home: None,
    };

    let paths = DariusPaths::resolve(&env, Some(workspace.path())).expect("resolve paths");

    assert_eq!(paths.home, canonical(home.path()));
    assert_eq!(paths.workspace, canonical(workspace.path()));
}

#[test]
fn paths_fallback_uses_platform_home_darius_directory() {
    let home = tempfile::tempdir().expect("temp platform home");
    let workspace = tempfile::tempdir().expect("temp workspace");
    let env = FakeEnv {
        vars: HashMap::new(),
        home: Some(home.path().to_path_buf()),
    };

    let paths = DariusPaths::resolve(&env, Some(workspace.path())).expect("resolve paths");

    assert_eq!(paths.home, home.path().join(".darius"));
    assert_eq!(paths.workspace, canonical(workspace.path()));
}

#[test]
fn paths_reject_profile_traversal_and_invalid_profile_characters() {
    let home = tempfile::tempdir().expect("temp home");
    let workspace = tempfile::tempdir().expect("temp workspace");
    let env = FakeEnv {
        vars: [("DARIUS_HOME", home.path().as_os_str().to_owned())]
            .into_iter()
            .collect(),
        home: None,
    };
    let paths = DariusPaths::resolve(&env, Some(workspace.path())).expect("resolve paths");

    for profile in ["../escape", "default/other", "name space", "", "."] {
        assert!(
            matches!(paths.profile(profile), Err(PathError::InvalidProfile(_))),
            "profile {profile:?} must be rejected"
        );
    }
}

#[test]
fn paths_reject_missing_or_nondirectory_workspace_without_changing_cwd() {
    let home = tempfile::tempdir().expect("temp home");
    let workspace = tempfile::tempdir().expect("temp workspace");
    let missing = workspace.path().join("missing");
    let file = workspace.path().join("file");
    std::fs::write(&file, "not a directory").expect("test file");
    let env = FakeEnv {
        vars: [("DARIUS_HOME", home.path().as_os_str().to_owned())]
            .into_iter()
            .collect(),
        home: None,
    };
    let before = std::env::current_dir().expect("current directory");

    assert!(matches!(
        DariusPaths::resolve(&env, Some(&missing)),
        Err(PathError::InvalidWorkspace(_))
    ));
    assert!(matches!(
        DariusPaths::resolve(&env, Some(&file)),
        Err(PathError::InvalidWorkspace(_))
    ));
    assert_eq!(std::env::current_dir().expect("current directory"), before);
}
