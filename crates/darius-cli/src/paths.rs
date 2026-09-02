mod environment;
mod error;

pub use environment::{Env, OsEnv};
pub use error::PathError;

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DariusPaths {
    pub home: PathBuf,
    pub workspace: PathBuf,
}

impl DariusPaths {
    pub fn resolve(env: &dyn Env, cwd: Option<&Path>) -> Result<Self, PathError> {
        let home = match env.var_os("DARIUS_HOME") {
            Some(path) => canonical_directory(PathBuf::from(path), PathError::InvalidHome)?,
            None => env
                .home_dir()
                .ok_or(PathError::HomeUnavailable)?
                .join(".darius"),
        };
        let requested = match cwd {
            Some(path) => path.to_path_buf(),
            None => std::env::current_dir()?,
        };
        let workspace = canonical_directory(requested, PathError::InvalidWorkspace)?;
        Ok(Self { home, workspace })
    }

    pub fn profile(&self, name: &str) -> Result<PathBuf, PathError> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(PathError::InvalidProfile(name.to_owned()));
        }
        Ok(self.home.join("profiles").join(name))
    }
}

fn canonical_directory(
    path: PathBuf,
    error: impl FnOnce(PathBuf) -> PathError,
) -> Result<PathBuf, PathError> {
    if !path.is_dir() {
        return Err(error(path));
    }
    Ok(path.canonicalize()?)
}
