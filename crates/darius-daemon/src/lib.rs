//! Darius daemon — session manager, A2A hub, and service orchestrator.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod a2a;
pub mod a2a_quotas;
pub mod backup;
pub mod cache;
pub mod chaos;
pub mod compliance;
pub mod cron;
pub mod daemon;
pub mod data_ownership;
pub mod event_log;
pub mod handoff;
pub mod kanban;
pub mod memory;
mod messaging;

pub use messaging::{MessagingError, MessagingMessage, task_state_status};
pub mod model_router;
pub mod platform_adapters;
pub mod profile;
pub mod sandbox;
pub mod secrets;
pub mod status_projector;
pub mod telemetry;
pub mod tools;
pub mod worktrees;

pub use a2a::{A2aServer, AgentCard};
pub use a2a_quotas::{AgentQuota, AgentState, QueuePolicy, QuotaError, QuotaManager};
pub use backup::{BackupError, BackupManager};
pub use cache::{CacheCoordinator, CacheMetrics};
pub use chaos::{ChaosError, ChaosTester, ManagedProcess};
pub use compliance::{
    AuditEvent, ComplianceError, ComplianceManager, ProfileExport, RetentionPolicy,
};
pub use cron::{CronError, CronJob, CronScheduler};
pub use daemon::{Daemon, DaemonError, DaemonStatus, Session};
pub use darius_core::{ArtifactRef, Decision, SessionHandoff};
pub use event_log::{EventLog, EventLogError};
pub use handoff::{HandoffError, HandoffStore};
pub use kanban::{KanbanBoard, KanbanError, KanbanTask, TaskStatus};
pub use memory::{HindsightMemory, MemoryError, MentalModel, SessionMemory};
pub use model_router::{
    BudgetEnforcer, BudgetScope, LiveModel, ModelRole, ModelRouter, Provider, ProviderRegistry,
    RouterError,
};
pub use platform_adapters::{
    AdapterError, AdapterManager, DiscordAdapter, IncomingMessage, PlatformAdapter, SlackAdapter,
    TelegramAdapter,
};
pub use profile::{Profile, ProfileError};
pub use sandbox::{SandboxBackend, SandboxError, SandboxManager};
pub use secrets::{Secret, SecretError, SecretScope, SecretStore};
pub use status_projector::StatusProjector;
pub use telemetry::{OtlpExporter, Span, SpanCategory, SpanStatus, TelemetryCollector};
pub use tools::{
    GrepMatch, ToolError, bash, browser, glob, grep, read_file, validate_yield, write_file,
};
pub use worktrees::{Worktree, WorktreeError, WorktreeManager};
