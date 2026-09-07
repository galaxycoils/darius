//! Dependency-neutral session control and correlated event envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Auto,
    Plan,
}
impl Mode {
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Plan,
            Self::Plan => Self::Auto,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "⏵⏵ auto mode on",
            Self::Plan => "⏸ plan mode on",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionChoice {
    AllowOnce,
    AllowSession,
    Deny,
}
impl PermissionChoice {
    pub const ALL: [Self; 3] = [Self::AllowOnce, Self::AllowSession, Self::Deny];

    pub fn label(self) -> &'static str {
        match self {
            Self::AllowOnce => "Yes",
            Self::AllowSession => "Yes, and don't ask again this session",
            Self::Deny => "No, and tell Darius what to do (esc)",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TurnId(pub u64);
impl TurnId {
    pub fn next() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}
/// TurnId(0) is reserved for session-level notices, never an executing turn.
#[derive(Debug, Clone)]
pub struct RuntimeEvent<E> {
    pub turn_id: TurnId,
    pub event: E,
}
impl<E> From<E> for RuntimeEvent<E> {
    fn from(event: E) -> Self {
        Self {
            turn_id: TurnId::default(),
            event,
        }
    }
}
