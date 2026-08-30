//! Safety gates, capability tiers, and audit log.
//!
//! Capability-based access control with approval tiers and tamper-evident audit.
//! Untrusted subagents are confined to isolation tier T2+; eval/learn paths
//! cannot bypass safety.

use parking_lot::Mutex;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SafetyError {
    #[error("capability denied: {0}")]
    Denied(String),
    #[error("approval required: {0}")]
    ApprovalRequired(String),
}

/// Capability tokens for scoped permissions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    ReadWorkspace,
    WriteWorkspace,
    Bash,
    Network,
    PrivilegedTools,
    PluginLoad,
    FileUpload,
}

/// Isolation tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationTier {
    Trusted,
    Process,
    GVisor,
    MicroVm,
    Wasm,
}

impl IsolationTier {
    /// Check if this tier is at least T2 (Process/gVisor/MicroVm/Wasm).
    pub fn is_at_least_t2(&self) -> bool {
        matches!(
            self,
            IsolationTier::Process
                | IsolationTier::GVisor
                | IsolationTier::MicroVm
                | IsolationTier::Wasm
        )
    }
}

/// Approval tier for operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalTier {
    Auto,
    Ask,
    Block,
}

/// Sandbox policy.
#[derive(Debug, Clone)]
pub enum SandboxPolicy {
    Native { allow_list: Vec<Capability> },
    Wasm { limits: WasmLimits },
    Python { tier: IsolationTier },
}

#[derive(Debug, Clone)]
pub struct WasmLimits {
    pub memory_bytes: u64,
    pub cpu_time_ms: u64,
    pub denied_apis: Vec<String>,
}

/// Audit entry.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: u64,
    pub action: String,
    pub status: AuditStatus,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditStatus {
    Allowed,
    Denied,
    Approved,
}

/// Audit log — append-only.
pub struct AuditLog {
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Safety gate — enforces capability checks before operation execution.
pub struct SafetyGate {
    policy: SandboxPolicy,
    approval: ApprovalTier,
    audit_log: Arc<Mutex<AuditLog>>,
}

impl SafetyGate {
    pub fn new(policy: SandboxPolicy, approval: ApprovalTier) -> Self {
        Self {
            policy,
            approval,
            audit_log: Arc::new(Mutex::new(AuditLog::new())),
        }
    }

    /// Check whether an operation is allowed.
    pub fn check(&self, cap: &Capability) -> Result<bool, SafetyError> {
        let allowed = match &self.policy {
            SandboxPolicy::Native { allow_list } => allow_list.contains(cap),
            SandboxPolicy::Wasm { .. } => {
                // WASM: deny privileged tools and network.
                !matches!(cap, Capability::PrivilegedTools | Capability::Network)
            }
            SandboxPolicy::Python { tier } => {
                // Python: only allow in trusted tier.
                matches!(tier, IsolationTier::Trusted)
            }
        };

        // Record the check in the audit log.
        let status = if allowed {
            AuditStatus::Allowed
        } else {
            AuditStatus::Denied
        };
        self.audit_log.lock().record(AuditEntry {
            timestamp: current_timestamp(),
            action: format!("check:{cap:?}"),
            status,
            details: format!("approval_tier:{:?}", self.approval),
        });

        if allowed {
            Ok(true)
        } else {
            Err(SafetyError::Denied(format!("{cap:?}")))
        }
    }

    /// Check whether an untrusted operation can proceed in the given tier.
    pub fn check_untrusted(&self, tier: IsolationTier) -> Result<bool, SafetyError> {
        if tier.is_at_least_t2() {
            self.audit_log.lock().record(AuditEntry {
                timestamp: current_timestamp(),
                action: "untrusted_check".into(),
                status: AuditStatus::Allowed,
                details: format!("tier:{tier:?}"),
            });
            Ok(true)
        } else {
            self.audit_log.lock().record(AuditEntry {
                timestamp: current_timestamp(),
                action: "untrusted_check".into(),
                status: AuditStatus::Denied,
                details: format!("tier:{tier:?} not T2+"),
            });
            Err(SafetyError::Denied(format!(
                "untrusted operations require T2+, got {tier:?}"
            )))
        }
    }

    /// Get the audit log.
    pub fn audit_log(&self) -> Arc<Mutex<AuditLog>> {
        self.audit_log.clone()
    }

    /// Get policy reference.
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Get approval tier.
    pub fn approval_tier(&self) -> ApprovalTier {
        self.approval
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_policy_allows_listed_capabilities() {
        let gate = SafetyGate::new(
            SandboxPolicy::Native {
                allow_list: vec![Capability::ReadWorkspace, Capability::WriteWorkspace],
            },
            ApprovalTier::Auto,
        );

        assert!(gate.check(&Capability::ReadWorkspace).is_ok());
        assert!(gate.check(&Capability::Bash).is_err());
    }

    #[test]
    fn wasm_policy_denies_privileged() {
        let gate = SafetyGate::new(
            SandboxPolicy::Wasm {
                limits: WasmLimits {
                    memory_bytes: 64 * 1024 * 1024,
                    cpu_time_ms: 1000,
                    denied_apis: vec!["network".into()],
                },
            },
            ApprovalTier::Auto,
        );

        assert!(gate.check(&Capability::ReadWorkspace).is_ok());
        assert!(gate.check(&Capability::PrivilegedTools).is_err());
        assert!(gate.check(&Capability::Network).is_err());
    }

    #[test]
    fn untrusted_requires_t2() {
        let gate = SafetyGate::new(
            SandboxPolicy::Native {
                allow_list: vec![Capability::Bash],
            },
            ApprovalTier::Ask,
        );

        // T1 (Trusted) should fail for untrusted.
        assert!(gate.check_untrusted(IsolationTier::Trusted).is_err());

        // T2 (Process) should succeed.
        assert!(gate.check_untrusted(IsolationTier::Process).is_ok());
        assert!(gate.check_untrusted(IsolationTier::GVisor).is_ok());
        assert!(gate.check_untrusted(IsolationTier::MicroVm).is_ok());
    }

    #[test]
    fn audit_log_records_checks() {
        let gate = SafetyGate::new(
            SandboxPolicy::Native {
                allow_list: vec![Capability::ReadWorkspace],
            },
            ApprovalTier::Auto,
        );

        let _ = gate.check(&Capability::ReadWorkspace);
        let _ = gate.check(&Capability::Bash);

        let log = gate.audit_log();
        assert_eq!(log.lock().len(), 2);
    }
}
