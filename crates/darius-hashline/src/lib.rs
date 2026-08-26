//! Hashline — content-hash anchored edit system.
//!
//! Hashline uses blake3 content hashing to anchor edits, ensuring that
//! modifications apply to the expected file state and rejecting stale
//! operations that would corrupt context.

pub mod anchors;
pub mod edits;
pub mod filesystem;
