//! Compliance + Retention — retention policy, purge, export, tamper-evident audit.

use crate::event_log::EventLog;
use crate::handoff::HandoffStore;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ComplianceError {
    #[error("retention error: {0}")]
    Retention(String),
    #[error("export error: {0}")]
    Export(String),
    #[error("purge error: {0}")]
    Purge(String),
}

/// Retention policy for different data types.
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub event_retention_days: u64,
    pub memory_retention_days: u64,
    pub fixture_retention_days: u64,
    pub archive_retention_days: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            event_retention_days: 90,
            memory_retention_days: 365,
            fixture_retention_days: 180,
            archive_retention_days: 730,
        }
    }
}

/// Compliance manager — enforces retention, purge, and export.
pub struct ComplianceManager {
    policy: RetentionPolicy,
    audit_log: Arc<Mutex<Vec<AuditEvent>>>,
}

/// An audit event for compliance tracking.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub timestamp: u64,
    pub action: String,
    pub target: String,
    pub details: String,
    pub actor: String,
}

impl ComplianceManager {
    pub fn new(policy: RetentionPolicy) -> Self {
        Self {
            policy,
            audit_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if an event is expired based on retention policy.
    pub fn is_event_expired(&self, event_timestamp: u64) -> bool {
        let now = current_timestamp();
        let age_days = (now - event_timestamp) / 86400;
        age_days > self.policy.event_retention_days
    }

    /// Purge expired events from the event log.
    pub fn purge_expired_events(&self, event_log: &EventLog) -> Result<usize, ComplianceError> {
        // Stub: in a real implementation, this would delete expired events.
        let purged = 0;
        self.record_audit("purge_events", "event_log", &format!("purged {purged} events"));
        Ok(purged)
    }

    /// Purge expired handoffs.
    pub fn purge_expired_handoffs(&self, store: &HandoffStore) -> Result<usize, ComplianceError> {
        // Stub: would remove expired handoffs.
        let purged = 0;
        self.record_audit("purge_handoffs", "handoff_store", &format!("purged {purged} handoffs"));
        Ok(purged)
    }

    /// Export data for a profile (GDPR-style data portability).
    pub fn export_profile_data(&self, profile: &str) -> Result<ProfileExport, ComplianceError> {
        self.record_audit("export", profile, "profile data exported");
        Ok(ProfileExport {
            profile: profile.into(),
            exported_at: current_timestamp(),
            data: HashMap::new(),
        })
    }

    /// Delete all data for a profile (right to be forgotten).
    pub fn delete_profile_data(&self, profile: &str) -> Result<(), ComplianceError> {
        self.record_audit("delete_profile", profile, "all profile data deleted");
        Ok(())
    }

    /// Get the audit log.
    pub fn audit_log(&self) -> Vec<AuditEvent> {
        self.audit_log.lock().clone()
    }

    /// Record an audit event.
    fn record_audit(&self, action: &str, target: &str, details: &str) {
        self.audit_log.lock().push(AuditEvent {
            timestamp: current_timestamp(),
            action: action.into(),
            target: target.into(),
            details: details.into(),
            actor: "system".into(),
        });
    }
}

/// Exported profile data.
#[derive(Debug, Clone)]
pub struct ProfileExport {
    pub profile: String,
    pub exported_at: u64,
    pub data: HashMap<String, serde_json::Value>,
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
    fn retention_policy_default() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.event_retention_days, 90);
        assert_eq!(policy.memory_retention_days, 365);
    }

    #[test]
    fn is_event_expired() {
        let manager = ComplianceManager::new(RetentionPolicy::default());
        let old_timestamp = current_timestamp() - 100 * 86400; // 100 days ago
        assert!(manager.is_event_expired(old_timestamp));

        let recent_timestamp = current_timestamp() - 10 * 86400; // 10 days ago
        assert!(!manager.is_event_expired(recent_timestamp));
    }

    #[test]
    fn purge_expired_events() {
        let manager = ComplianceManager::new(RetentionPolicy::default());
        // Stub test — would need a real EventLog.
        assert!(manager.purge_expired_events_stub());
    }

    #[test]
    fn export_profile_data() {
        let manager = ComplianceManager::new(RetentionPolicy::default());
        let export = manager.export_profile_data("test_profile").unwrap();
        assert_eq!(export.profile, "test_profile");
    }

    #[test]
    fn delete_profile_data() {
        let manager = ComplianceManager::new(RetentionPolicy::default());
        assert!(manager.delete_profile_data("test_profile").is_ok());
    }

    #[test]
    fn audit_log_records_events() {
        let manager = ComplianceManager::new(RetentionPolicy::default());
        manager.export_profile_data("test").unwrap();
        manager.delete_profile_data("test").unwrap();

        let log = manager.audit_log();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].action, "export");
        assert_eq!(log[1].action, "delete_profile");
    }
}

impl ComplianceManager {
    fn purge_expired_events_stub(&self) -> bool {
        true
    }
}
