use super::AuthStoreError as Error;
use std::{fs::File, path::Path};

/// Same-directory tempfile: private before the first byte and cleaned on failure.
pub(super) fn replace<F>(path: &Path, write: F) -> Result<(), Error>
where
    F: FnOnce(&mut File) -> Result<(), Error>,
{
    #[cfg(not(unix))]
    {
        let _ = (path, write);
        Err(Error::UnsupportedPlatform)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = path.parent().ok_or(Error::Io)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|_| Error::Io)?;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::Io)?;
        write(temp.as_file_mut())?;
        temp.as_file().sync_all().map_err(|_| Error::Io)?;
        temp.persist(path).map_err(|_| Error::Io)?;
        // Rename has committed; a subsequent directory-sync failure is reported as I/O.
        File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| Error::Io)
    }
}
