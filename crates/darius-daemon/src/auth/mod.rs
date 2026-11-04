//! Nous Portal OAuth device-code service and proactive token keepalive.
//!
//! Ported behavior from NousResearch/hermes-agent @ `a6102b8d809d6b32824757b547b60f9d22d6a88e`
//! — `hermes_cli/auth_nous.py`, `hermes_cli/auth_constants.py`,
//! `hermes_cli/nous_auth_keepalive.py`. Engine only; CLI, request transport, and
//! the daemon run/TUI/serve lifecycles are wired by the parent after this lands.
//!
//! # Public surface
//! - `NousAuthService`: device-code login (start/poll), status, logout, and a
//!   single-flight, refresh-aware token resolver with cross-instance shared-store
//!   mirroring. The transport layer resolves the Nous bearer via `resolve_token`
//!   and treats a 401 as a forced refresh (re-resolve after 401).
//! - `Keepalive`: background tokio task that refreshes the Nous credential ahead
//!   of expiry so long sessions never hit a mid-turn 401.

pub mod keepalive;
pub mod nous;

pub use keepalive::Keepalive;
pub use nous::{AuthStatus, DeviceCode, NousAuthConfig, NousAuthError, NousAuthService, NousCred};
