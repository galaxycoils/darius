//! Per-turn cancellation token plus 60-second request deadline.
use std::time::{Duration, Instant};

/// Fresh token + deadline per turn; cancel wins ties.
#[derive(Clone, Debug)]
pub struct TurnContext {
    cancel: tokio_util::sync::CancellationToken,
    deadline: Instant,
}

impl TurnContext {
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(60))
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            cancel: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + timeout,
        }
    }

    pub fn with_token(cancel: tokio_util::sync::CancellationToken) -> Self {
        Self {
            cancel,
            ..Self::new()
        }
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
