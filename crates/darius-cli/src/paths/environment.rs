use std::ffi::OsString;
use std::path::PathBuf;

pub trait Env {
    fn var_os(&self, key: &str) -> Option<OsString>;
    fn home_dir(&self) -> Option<PathBuf>;
}

pub struct OsEnv;

impl Env for OsEnv {
    fn var_os(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }

    fn home_dir(&self) -> Option<PathBuf> {
        dirs::home_dir()
    }
}
