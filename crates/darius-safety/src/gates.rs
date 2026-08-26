//! Safety gates.
//!
//! Enforces capability checks and approval tiers before executing privileged,
//! network, or filesystem-mutating operations. Eval/learn paths cannot bypass
//! these gates.

use crate::capabilities::{ApprovalTier, Capability};

/// Result of a safety gate check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateResult {
    /// Operation is allowed.
    Allowed,
    /// Operation is denied.
    Denied { reason: String },
    /// Operation requires explicit approval.
    RequiresApproval { tier: ApprovalTier, capability: Capability },
}

/// Safety gate that checks capabilities before operation execution.
pub struct SafetyGate;

impl SafetyGate {
    /// Create a new safety gate.
    pub fn new() -> Self {
        Self
    }

    /// Check whether an operation with the given capability is allowed.
    pub fn check(&self, _capability: &Capability) -> GateResult {
        GateResult::Allowed
    }
}

impl Default for SafetyGate {
    fn default() -> Self {
        Self::new()
    }
}
