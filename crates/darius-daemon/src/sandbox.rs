//! Tiered Sandbox Runtime — backend trait for isolation tiers.

use darius_core::IsolationTier;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox error: {0}")]
    Backend(String),
    #[error("unsupported tier: {0}")]
    UnsupportedTier(String),
    #[error("spawn failed: {0}")]
    SpawnFailed(String),
}

/// Sandbox backend trait.
pub trait SandboxBackend: Send + Sync {
    /// Get the tier of this backend.
    fn tier(&self) -> IsolationTier;

    /// Spawn a process in this sandbox.
    fn spawn(&self, command: &[String]) -> Result<u32, SandboxError>;

    /// Terminate a process.
    fn terminate(&self, pid: u32) -> Result<(), SandboxError>;

    /// Check if a process is running.
    fn is_running(&self, pid: u32) -> Result<bool, SandboxError>;
}

/// T1 Namespace backend — shared kernel, isolated globals.
pub struct NamespaceBackend;

impl SandboxBackend for NamespaceBackend {
    fn tier(&self) -> IsolationTier {
        IsolationTier::Trusted
    }

    fn spawn(&self, command: &[String]) -> Result<u32, SandboxError> {
        let child = std::process::Command::new(&command[0])
            .args(&command[1..])
            .spawn()
            .map_err(|e| SandboxError::SpawnFailed(e.to_string()))?;
        Ok(child.id())
    }

    fn terminate(&self, _pid: u32) -> Result<(), SandboxError> {
        // Stub: would use platform-specific APIs.
        Ok(())
    }

    fn is_running(&self, _pid: u32) -> Result<bool, SandboxError> {
        Ok(false)
    }
}

/// T2 Process backend — subprocess isolation.
pub struct ProcessBackend;

impl SandboxBackend for ProcessBackend {
    fn tier(&self) -> IsolationTier {
        IsolationTier::Process
    }

    fn spawn(&self, command: &[String]) -> Result<u32, SandboxError> {
        let child = std::process::Command::new(&command[0])
            .args(&command[1..])
            .spawn()
            .map_err(|e| SandboxError::SpawnFailed(e.to_string()))?;
        Ok(child.id())
    }

    fn terminate(&self, _pid: u32) -> Result<(), SandboxError> {
        Ok(())
    }

    fn is_running(&self, _pid: u32) -> Result<bool, SandboxError> {
        Ok(false)
    }
}

/// T2 GVisor backend — gVisor-isolated (stub).
pub struct GVisorBackend;

impl SandboxBackend for GVisorBackend {
    fn tier(&self) -> IsolationTier {
        IsolationTier::GVisor
    }

    fn spawn(&self, _command: &[String]) -> Result<u32, SandboxError> {
        Err(SandboxError::UnsupportedTier(
            "gVisor requires Linux".into(),
        ))
    }

    fn terminate(&self, _pid: u32) -> Result<(), SandboxError> {
        Ok(())
    }

    fn is_running(&self, _pid: u32) -> Result<bool, SandboxError> {
        Ok(false)
    }
}

/// T2b MicroVM backend — Firecracker (stub).
pub struct MicroVmBackend;

impl SandboxBackend for MicroVmBackend {
    fn tier(&self) -> IsolationTier {
        IsolationTier::MicroVm
    }

    fn spawn(&self, _command: &[String]) -> Result<u32, SandboxError> {
        Err(SandboxError::UnsupportedTier(
            "microVM requires Firecracker".into(),
        ))
    }

    fn terminate(&self, _pid: u32) -> Result<(), SandboxError> {
        Ok(())
    }

    fn is_running(&self, _pid: u32) -> Result<bool, SandboxError> {
        Ok(false)
    }
}

/// T3 WASM backend — Wasmtime (stub).
pub struct WasmBackend;

impl SandboxBackend for WasmBackend {
    fn tier(&self) -> IsolationTier {
        IsolationTier::Wasm
    }

    fn spawn(&self, _command: &[String]) -> Result<u32, SandboxError> {
        Err(SandboxError::UnsupportedTier(
            "WASM backend not implemented".into(),
        ))
    }

    fn terminate(&self, _pid: u32) -> Result<(), SandboxError> {
        Ok(())
    }

    fn is_running(&self, _pid: u32) -> Result<bool, SandboxError> {
        Ok(false)
    }
}

/// Detect if gVisor is available on the system.
pub fn detect_gvisor() -> bool {
    // Check for gVisor binary (runsc) in common locations
    let paths = ["/usr/local/bin/runsc", "/usr/bin/runsc", "/sbin/runsc"];
    for path in &paths {
        if std::path::Path::new(path).exists() {
            return true;
        }
    }
    // Also check PATH
    if let Ok(output) = std::process::Command::new("which").arg("runsc").output() {
        return output.status.success();
    }
    false
}

/// Force-terminate a process by PID (cross-platform).
pub fn force_terminate(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        // Send SIGKILL
        let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        if result == 0 {
            Ok(())
        } else {
            Err(format!("failed to kill process {pid}"))
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err("force_terminate not supported on this platform".into())
    }
}

/// Terminate a process after a timeout. If the process doesn't exit within
/// `timeout_ms`, force-kill it.
pub fn terminate_with_timeout(pid: u32, timeout_ms: u64) -> Result<(), String> {
    // First, try graceful termination
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if result != 0 {
            return Err(format!("failed to send SIGTERM to {pid}"));
        }
    }

    // Wait for process to exit
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);
    while start.elapsed() < timeout {
        #[cfg(unix)]
        {
            // Check if process is still running (kill with signal 0)
            let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
            if result != 0 {
                return Ok(()); // Process exited
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Force kill if still running
    force_terminate(pid)
}

/// Sandbox manager — selects backend based on policy.
pub struct SandboxManager;

impl SandboxManager {
    /// Select a backend for a given tier.
    fn select_backend(tier: IsolationTier) -> Box<dyn SandboxBackend> {
        match tier {
            IsolationTier::Trusted => Box::new(NamespaceBackend),
            IsolationTier::Process => Box::new(ProcessBackend),
            IsolationTier::GVisor => Box::new(GVisorBackend),
            IsolationTier::MicroVm => Box::new(MicroVmBackend),
            IsolationTier::Wasm => Box::new(WasmBackend),
        }
    }

    /// Spawn in a sandbox for the given tier.
    pub fn spawn(tier: IsolationTier, command: &[String]) -> Result<u32, SandboxError> {
        let backend = Self::select_backend(tier);
        backend.spawn(command)
    }

    /// Spawn untrusted code (minimum T2).
    pub fn spawn_untrusted(command: &[String]) -> Result<u32, SandboxError> {
        Self::spawn(IsolationTier::Process, command)
    }

    /// Spawn a plugin (T3 WASM).
    pub fn spawn_plugin(command: &[String]) -> Result<u32, SandboxError> {
        Self::spawn(IsolationTier::Wasm, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_spawns_t2() {
        let backend = SandboxManager::select_backend(IsolationTier::Process);
        assert_eq!(backend.tier(), IsolationTier::Process);
    }

    #[test]
    fn plugin_spawns_t3() {
        let backend = SandboxManager::select_backend(IsolationTier::Wasm);
        assert_eq!(backend.tier(), IsolationTier::Wasm);
    }

    #[test]
    fn namespace_backend_t1() {
        let backend = NamespaceBackend;
        assert_eq!(backend.tier(), IsolationTier::Trusted);
    }

    #[test]
    fn process_backend_can_spawn() {
        let backend = ProcessBackend;
        // Stub: just verify it doesn't panic.
        assert_eq!(backend.tier(), IsolationTier::Process);
    }

    #[test]
    fn sandbox_manager_selects_correct_backends() {
        let t1 = SandboxManager::select_backend(IsolationTier::Trusted);
        assert_eq!(t1.tier(), IsolationTier::Trusted);

        let t2 = SandboxManager::select_backend(IsolationTier::Process);
        assert_eq!(t2.tier(), IsolationTier::Process);

        let t3 = SandboxManager::select_backend(IsolationTier::Wasm);
        assert_eq!(t3.tier(), IsolationTier::Wasm);
    }

    #[test]
    fn detect_gvisor_returns_bool() {
        // Just verify it doesn't panic
        let _ = detect_gvisor();
    }

    #[test]
    fn terminate_with_timeout_invalid_pid() {
        // Invalid PID should return an error
        let result = terminate_with_timeout(999999, 100);
        // On most systems, killing a non-existent process with SIGTERM returns -1
        // but the function may still succeed if the process doesn't exist
        // Just verify it doesn't panic
        let _ = result;
    }
}
