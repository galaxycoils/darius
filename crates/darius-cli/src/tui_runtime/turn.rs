//! Move the entire session in, poll its non-Send future locally, move it back.
use super::ChannelRunControl;
use crate::runtime::{SessionRuntime, block_on_turn};
use darius_cognitive::{AgentLoop, CognitiveError, EventSink, UiEvent};
use darius_core::runtime_protocol::{RuntimeEvent, TurnId};
use std::sync::Arc;

pub(crate) struct TurnResult {
    pub runtime: SessionRuntime,
    pub outcome: Result<String, CognitiveError>,
}
struct TurnSink {
    turn_id: TurnId,
    events: tokio::sync::broadcast::Sender<RuntimeEvent<UiEvent>>,
}
impl EventSink for TurnSink {
    fn emit(&self, event: UiEvent) {
        // Done is published by the actor only after joining and restoring Idle.
        if !matches!(event, UiEvent::Done) {
            let _ = self.events.send(RuntimeEvent {
                turn_id: self.turn_id,
                event,
            });
        }
    }
}
pub(super) fn spawn(
    mut runtime: SessionRuntime,
    text: String,
    turn_id: TurnId,
    events: tokio::sync::broadcast::Sender<RuntimeEvent<UiEvent>>,
) -> (tokio::task::JoinHandle<TurnResult>, Arc<ChannelRunControl>) {
    runtime.cancellation = tokio_util::sync::CancellationToken::new();
    let sink: Arc<dyn EventSink> = Arc::new(TurnSink { turn_id, events });
    let mut control = ChannelRunControl::new(
        sink.clone(),
        runtime.cancellation.clone(),
        runtime.tools.path_policy().clone(),
    );
    control.session_cache = runtime.permissions.clone();
    control.mode = runtime.mode;
    let control = Arc::new(control);
    let turn_control = control.clone();
    let join = tokio::task::spawn_blocking(move || {
        let agent = AgentLoop::new(sink, turn_control);
        let workspace = runtime.workspace.to_string_lossy().into_owned();
        let outcome = block_on_turn(agent.run_turn_with_extra_tools(
            &runtime.metadata,
            &runtime.policy,
            &text,
            &mut runtime.conversation,
            runtime.model.as_mut(),
            &runtime.tools,
            &runtime.memory,
            &workspace,
            &runtime.dynamic_tool_specs,
        ));
        TurnResult { runtime, outcome }
    });
    (join, control)
}
