#![allow(dead_code)]
pub mod fake_provider;
pub mod screen;

/// Snapshot every entry, file byte, and symlink without following links.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DariusHomeSnapshot {
    entries: std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
}
impl DariusHomeSnapshot {
    pub fn capture() -> Self {
        fn walk(
            path: &std::path::Path,
            entries: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
        ) {
            let metadata = std::fs::symlink_metadata(path).expect("snapshot metadata");
            let value = if metadata.is_symlink() {
                format!("link:{}", std::fs::read_link(path).unwrap().display()).into_bytes()
            } else if metadata.is_file() {
                std::fs::read(path).expect("snapshot file")
            } else {
                b"directory".to_vec()
            };
            entries.insert(path.to_path_buf(), value);
            if metadata.is_dir() {
                for entry in std::fs::read_dir(path).expect("snapshot directory") {
                    walk(&entry.unwrap().path(), entries);
                }
            }
        }
        let mut snapshot = Self::default();
        let path = dirs::home_dir().expect("real home").join(".darius");
        if path.exists() {
            walk(&path, &mut snapshot.entries);
        }
        snapshot
    }
    pub fn assert_unchanged(&self, label: &str) {
        // Avoid dumping private home contents in assertion diagnostics.
        assert!(self == &Self::capture(), "{label}: real ~/.darius changed");
    }
}
