//! Chaos engineering — fault injection and cross-platform process termination.

use std::process::Child;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChaosError {
    #[error("process not found: {0}")]
    NotFound(String),
    #[error("termination failed: {0}")]
    TerminationFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Managed child process handle.
pub struct ManagedProcess {
    child: Child,
    label: String,
}

impl ManagedProcess {
    /// Spawn a new managed child process.
    pub fn spawn(cmd: &str, args: &[&str], label: impl Into<String>) -> Result<Self, ChaosError> {
        let child = std::process::Command::new(cmd).args(args).spawn()?;
        Ok(Self {
            child,
            label: label.into(),
        })
    }

    /// Get the PID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Get the label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Gracefully terminate: send interrupt, wait, then force-kill if needed.
    pub fn terminate(&mut self, graceful_timeout_ms: u64) -> Result<(), ChaosError> {
        // Try graceful interrupt first.
        #[cfg(unix)]
        unsafe {
            libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM);
        }
        #[cfg(windows)]
        {
            // On Windows, there's no SIGTERM; use terminate.
            let _ = self.child.kill();
        }

        // Wait for graceful shutdown.
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(graceful_timeout_ms);
        while start.elapsed() < timeout {
            match self.child.try_wait()? {
                Some(status) => {
                    if !status.success() {
                        return Err(ChaosError::TerminationFailed(format!(
                            "{} exited with {:?}",
                            self.label, status
                        )));
                    }
                    return Ok(());
                }
                None => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }

        // Force-kill if still running.
        self.force_kill()
    }

    /// Force-kill the process immediately.
    pub fn force_kill(&mut self) -> Result<(), ChaosError> {
        self.child.kill()?;
        self.child.wait()?;
        Ok(())
    }
}

/// Chaos testing utilities for fault injection.
pub struct ChaosTester;

impl ChaosTester {
    /// Test that a process can be force-killed.
    pub fn test_force_kill(cmd: &str, args: &[&str]) -> Result<(), ChaosError> {
        let mut proc = ManagedProcess::spawn(cmd, args, "force-kill-test")?;
        assert!(proc.is_running());
        proc.force_kill()?;
        assert!(!proc.is_running());
        Ok(())
    }

    /// Test graceful termination with timeout.
    pub fn test_graceful_termination(
        cmd: &str,
        args: &[&str],
        timeout_ms: u64,
    ) -> Result<(), ChaosError> {
        let mut proc = ManagedProcess::spawn(cmd, args, "graceful-test")?;
        assert!(proc.is_running());
        proc.terminate(timeout_ms)?;
        assert!(!proc.is_running());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_process_spawn_and_kill() {
        #[cfg(unix)]
        let (cmd, args) = ("sleep", vec!["60"]);
        #[cfg(windows)]
        let (cmd, args) = ("cmd", vec!["/C", "timeout", "/t", "60"]);

        let mut proc = ManagedProcess::spawn(cmd, &args, "test").unwrap();
        assert!(proc.is_running());
        assert!(proc.pid() > 0);
        proc.force_kill().unwrap();
        assert!(!proc.is_running());
    }

    #[test]
    fn chaos_tester_force_kill() {
        #[cfg(unix)]
        ChaosTester::test_force_kill("sleep", &["60"]).unwrap();
        #[cfg(windows)]
        ChaosTester::test_force_kill("cmd", &["/C", "timeout", "/t", "60"]).unwrap();
    }
}
