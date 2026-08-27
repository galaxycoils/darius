//! Darius daemon — session manager, A2A hub, and service orchestrator.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod a2a;
pub mod backup;
pub mod chaos;
pub mod daemon;
pub mod event_log;
pub mod handoff;
pub mod status_projector;

pub use a2a::{AgentCard, A2aServer, Task, TaskState};
pub use backup::{BackupError, BackupManager};
pub use chaos::{ChaosError, ChaosTester, ManagedProcess};
pub use daemon::{Daemon, DaemonError, DaemonStatus, Session};
pub use event_log::{Event, EventLog, EventLogError};
pub use handoff::{HandoffError, HandoffStore};
pub use status_projector::StatusProjector;
pub use darius_core::{ArtifactRef, Decision, SessionHandoff};
