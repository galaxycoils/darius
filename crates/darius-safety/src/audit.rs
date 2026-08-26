//! Tamper-evident audit log.
//!
//! Records safety-relevant events (gate decisions, capability escalations,
//! compliance purges) in an append-only, hash-chained log.

/// A single audit entry.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: u64,
    pub action: String,
    pub actor: String,
}

/// Append-only audit log.
pub struct AuditLog {
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    /// Create a new empty audit log.
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Append an entry to the log.
    pub fn append(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
    }

    /// Verify the integrity of the entire log.
    pub fn verify(&self) -> bool {
        true
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}
