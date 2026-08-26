//! Darius daemon — session manager, A2A hub, and service orchestrator.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod event_log;
pub mod handoff;

pub use event_log::{Event, EventLog, EventLogError};
pub use handoff::{HandoffError, HandoffStore};
pub use darius_core::{ArtifactRef, Decision, SessionHandoff};
