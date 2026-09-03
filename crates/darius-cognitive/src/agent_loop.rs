//! One multi-turn coding agent loop over a shared Conversation.
use crate::agent_events::emit_header;
use crate::conversation::{Conversation, Message};
use crate::model::AsyncModel;
use crate::{CognitiveError, EventSink, LoopPolicy, RunControl, RunMetadata, UiEvent};
use std::sync::Arc;
pub const MAX_ROUNDS: usize = 12;
pub struct AgentLoop {
    pub(crate) sink: Arc<dyn EventSink>,
    pub(crate) control: Arc<dyn RunControl>,
}
impl AgentLoop {
    pub fn new(sink: Arc<dyn EventSink>, control: Arc<dyn RunControl>) -> Self {
        Self { sink, control }
    }
    /// Run one user turn; transcript persists only on terminal text.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_turn(
        &self,
        meta: &RunMetadata,
        policy: &LoopPolicy,
        goal: &str,
        convo: &mut Conversation,
        model: &mut dyn AsyncModel,
        tools: &darius_tools::ToolRegistry,
        memory: &darius_memory::MemoryEngine,
        workspace: &str,
    ) -> Result<String, CognitiveError> {
        emit_header(self.sink.as_ref(), meta, goal);
        let mut msgs = convo.messages().to_vec();
        msgs.push(Message::User {
            content: goal.into(),
        });
        self.sink.emit(UiEvent::UserMessage { text: goal.into() });
        let outcome = self
            .drive(policy, &mut msgs, model, tools, memory, workspace)
            .await;
        self.finish(outcome, msgs, convo)
    }
}
