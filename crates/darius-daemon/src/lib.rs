//! Darius daemon — session manager, A2A hub, and service orchestrator.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod a2a;
pub mod backup;
pub mod cache;
pub mod chaos;
pub mod daemon;
pub mod event_log;
pub mod handoff;
pub mod model_router;
pub mod profile;
pub mod status_projector;
pub mod tools;
pub mod worktrees;

pub use a2a::{AgentCard, A2aServer, Task, TaskState};
pub use backup::{BackupError, BackupManager};
pub use cache::{CacheCoordinator, CacheMetrics};
pub use chaos::{ChaosError, ChaosTester, ManagedProcess};
pub use daemon::{Daemon, DaemonError, DaemonStatus, Session};
pub use event_log::{Event, EventLog, EventLogError};
pub use handoff::{HandoffError, HandoffStore};
pub use model_router::{BudgetEnforcer, BudgetScope, ModelRole, ModelRouter, Provider, ProviderRegistry, RouterError};
pub use status_projector::StatusProjector;
pub use profile::{Profile, ProfileError};
pub use tools::{GrepMatch, ToolError, bash, browser, glob, grep, read_file, validate_yield, write_file};
pub use worktrees::{Worktree, WorktreeError, WorktreeManager};
pub use darius_core::{ArtifactRef, Decision, SessionHandoff};
