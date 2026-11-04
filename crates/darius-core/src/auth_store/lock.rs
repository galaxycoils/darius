use super::AuthStoreError as Error;
use std::{fs::File, path::Path};

// Closing the owned descriptor releases flock, including on unwind/process exit.
// Never unlink the sidecar: replacing its inode would permit competing locks.
pub(super) fn acquire(home: &Path) -> Result<File, Error> {
    #[cfg(not(unix))]
    {
        let _ = home;
        Err(Error::UnsupportedPlatform)
    }
    #[cfg(unix)]
    {
        use std::os::{
            fd::AsRawFd,
            unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        };
        use std::time::{Duration, Instant};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(home)
            .map_err(|_| Error::Io)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(home.join("auth.lock"))
            .map_err(|_| Error::Io)?;
        if !file.metadata().map_err(|_| Error::Io)?.is_file() {
            return Err(Error::InvalidState);
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::Io)?;
        let start = Instant::now();
        loop {
            // SAFETY: file owns a valid descriptor for the entire call; flock retains no pointer.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(file);
            }
            let error = std::io::Error::last_os_error();
            if !matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
            ) {
                return Err(Error::Io);
            }
            if start.elapsed() >= Duration::from_secs(10) {
                return Err(Error::LockTimeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
