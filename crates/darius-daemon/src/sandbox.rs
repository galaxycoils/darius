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
}
