//! Darius daemon — session manager, A2A hub, and service orchestrator.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod a2a;
pub mod daemon;
pub mod event_log;
pub mod handoff;

pub use a2a::{AgentCard, A2aServer, Task, TaskState};
pub use daemon::{Daemon, DaemonError, DaemonStatus, Session};
pub use event_log::{Event, EventLog, EventLogError};
pub use handoff::{HandoffError, HandoffStore};
pub use darius_core::{ArtifactRef, Decision, SessionHandoff};
