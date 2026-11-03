//! Per-turn cancellation, deadline, and opaque conversation identity.
use std::time::{Duration, Instant};

/// Fresh token + deadline per turn; cancel wins ties.
#[derive(Clone, Debug)]
pub struct TurnContext {
    cancel: tokio_util::sync::CancellationToken,
    deadline: Instant,
    session_id: std::sync::Arc<str>,
}

impl TurnContext {
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(60))
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            cancel: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + timeout,
            session_id: "darius-default-session".into(),
        }
    }

    pub fn with_token(cancel: tokio_util::sync::CancellationToken) -> Self {
        Self {
            cancel,
            ..Self::new()
        }
    }

    /// Attach an opaque identity without changing cancellation or deadline.
    pub fn with_session_id(mut self, session_id: impl Into<std::sync::Arc<str>>) -> Self {
        self.session_id = session_id.into();
        self
    }

    /// Stable fallback for callers without a session; runtime callers supply their own ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn token(&self) -> tokio_util::sync::CancellationToken {
        self.cancel.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub fn execution_context(&self) -> darius_tools::ExecutionContext {
        darius_tools::ExecutionContext {
            cancel: self.token(),
            deadline: self.deadline,
        }
    }

    pub fn deadline_duration(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

impl Default for TurnContext {
    fn default() -> Self {
        Self::new()
    }
}
