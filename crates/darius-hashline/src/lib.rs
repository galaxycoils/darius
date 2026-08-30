//! Hashline — content-hash anchored edit system.
//!
//! Hashline uses blake3 content hashing to anchor edits, ensuring that
//! modifications apply to the expected file state and rejecting stale
//! operations that would corrupt context.

pub mod anchors;
pub mod edits;
pub mod filesystem;

pub use anchors::{FileAnchor, anchor_for, anchors_match, compute_anchor};
pub use edits::{EditError, EditOp, EditResult, apply_cut, apply_put, rejects_stale_hash};
pub use filesystem::{DiskFilesystem, Filesystem, FilesystemError, InMemoryFilesystem};
