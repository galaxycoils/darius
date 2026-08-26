//! Capability-based access control.
//!
//! Defines capability tokens and approval tiers used to gate privileged,
//! network, and filesystem-mutating operations beyond the workspace.

/// Capability token granting a scoped permission.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Capability {
    pub name: String,
    pub scope: CapabilityScope,
}

/// Scope of a capability.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CapabilityScope {
    /// Read-only access within the workspace.
    WorkspaceRead,
    /// Write access within the workspace.
    WorkspaceWrite,
    /// Execute commands in the sandbox.
    SandboxExec,
    /// Network access.
    Network,
    /// Access to secrets.
    Secrets,
    /// Full access (trusted only).
    Full,
}

/// Approval tier required for an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApprovalTier {
    /// No approval required.
    None,
    /// Implicit approval for trusted, read-only operations.
    Implicit,
    /// Explicit user approval required.
    Explicit,
    /// Admin approval required (privileged operations).
    Admin,
}
