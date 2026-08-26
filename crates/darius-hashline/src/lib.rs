//! Hashline — content-hash anchored edit system.
//!
//! Hashline uses blake3 content hashing to anchor edits, ensuring that
//! modifications apply to the expected file state and rejecting stale
//! operations that would corrupt context.

pub mod anchors;
pub mod edits;
pub mod filesystem;

pub use anchors::{
    anchor_for, anchors_match, compute_anchor, FileAnchor,
};
pub use edits::{
    apply_cut, apply_put, EditError, EditOp, EditResult, rejects_stale_hash,
};
pub use filesystem::{
    DiskFilesystem, Filesystem, FilesystemError, InMemoryFilesystem,
};
