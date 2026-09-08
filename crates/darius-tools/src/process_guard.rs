//! Own a shell process group through every completion and error path.
#[cfg(unix)]
pub(crate) struct ProcessGuard(pub std::process::Child, pub bool);

#[cfg(unix)]
impl ProcessGuard {
    pub fn terminate(&mut self) -> Result<std::process::ExitStatus, String> {
        if self.1 {
            return self.0.wait().map_err(|error| error.to_string());
        }
        let rc = unsafe { libc::killpg(self.0.id() as libc::pid_t, libc::SIGKILL) };
        if rc != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let status = self.0.wait().map_err(|error| error.to_string())?;
        self.1 = true;
        Ok(status)
    }
}

#[cfg(unix)]
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}
