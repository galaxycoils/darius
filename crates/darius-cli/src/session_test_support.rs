use crate::{paths::DariusPaths, runtime::SessionRuntime};
#[path = "../../darius-cognitive/tests/support/session_observer.rs"]
mod observer;
pub use observer::Observer;

pub fn runtime(temp: &tempfile::TempDir) -> SessionRuntime {
    let paths = DariusPaths {
        home: temp.path().join("home"),
        workspace: temp.path().into(),
    };
    let profile = paths.profile("session").unwrap();
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("config.toml"),
        r#"[model]
provider = "openai_compatible"
base_url = "http://127.0.0.1:1/v1"
model = "test"
api_key_env = "NONE"
"#,
    )
    .unwrap();
    SessionRuntime::from_profile(&paths, "session").unwrap()
}
