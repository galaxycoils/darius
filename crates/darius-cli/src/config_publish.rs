use crate::config_error::ConfigError;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub(crate) fn publish(temporary: &Path, target: &Path, force: bool) -> Result<(), ConfigError> {
    if force {
        return fs::rename(temporary, target).map_err(ConfigError::Write);
    }
    match fs::hard_link(temporary, target) {
        Ok(()) => fs::remove_file(temporary).map_err(ConfigError::Write),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(ConfigError::AlreadyExists {
                path: target.to_path_buf(),
            })
        }
        Err(error) => Err(ConfigError::Write(error)),
    }
}

pub(crate) fn write_temp(path: &Path, content: &str) -> Result<(), ConfigError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(ConfigError::Write)?;
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(ConfigError::Write)?;
    file.write_all(content.as_bytes())
        .map_err(ConfigError::Write)?;
    file.sync_all().map_err(ConfigError::Write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_without_force_never_replaces_existing_target() {
        let directory = tempfile::tempdir().expect("temp directory");
        let temporary = directory.path().join(".config.tmp");
        let target = directory.path().join("config.toml");
        fs::write(&temporary, "new metadata").expect("temporary config");
        fs::write(&target, "existing metadata").expect("existing config");

        assert!(matches!(
            publish(&temporary, &target, false),
            Err(ConfigError::AlreadyExists { .. })
        ));
        assert_eq!(fs::read_to_string(&target).unwrap(), "existing metadata");
    }
}
