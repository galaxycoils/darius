#![allow(dead_code)]
pub mod fake_provider;

/// Snapshot of ~/.darius state before/after a test to verify no pollution.
#[derive(Debug, Default)]
pub struct DariusHomeSnapshot {
    exists: bool,
    metadata: Option<std::fs::Metadata>,
}

impl DariusHomeSnapshot {
    pub fn capture() -> Self {
        let path = dirs::home_dir().map(|h| h.join(".darius"));
        let (exists, metadata) = path
            .as_ref()
            .map(|p| (p.exists(), std::fs::metadata(p).ok()))
            .unwrap_or((false, None));
        Self { exists, metadata }
    }

    pub fn assert_unchanged(&self, label: &str) {
        let after = Self::capture();
        assert_eq!(
            self.exists, after.exists,
            "{label}: ~/.darius existence changed (before={}, after={})",
            self.exists, after.exists
        );
        if let (Some(before), Some(after)) = (self.metadata.as_ref(), after.metadata.as_ref()) {
            assert_eq!(
                before.modified().ok(),
                after.modified().ok(),
                "{label}: ~/.darius mtime changed",
            );
        }
    }
}
