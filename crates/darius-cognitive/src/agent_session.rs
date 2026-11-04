use crate::AgentLoop;
use std::sync::Arc;

impl AgentLoop {
    /// Reuse the runtime conversation identity for every turn and tool round.
    pub fn with_session_id(mut self, session_id: impl Into<Arc<str>>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}
