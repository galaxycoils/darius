use crate::runtime::SessionRuntime;
use darius_cognitive::{AgentLoop, EventSink, RunControl};
use std::sync::Arc;

impl SessionRuntime {
    /// Opaque UUID created once per runtime conversation, never per turn.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Shared run/TUI/serve construction path preserves identity across turns.
    pub fn agent_loop(&self, sink: Arc<dyn EventSink>, control: Arc<dyn RunControl>) -> AgentLoop {
        AgentLoop::new(sink, control).with_session_id(self.session_id.clone())
    }
}
