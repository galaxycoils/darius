//! The async actor owns the session or its single blocking turn task.
pub(crate) use super::state::{Running, State};
use crate::runtime::{SessionRuntime, block_on_turn};
use darius_cognitive::UiEvent;
use darius_core::runtime_protocol::{RuntimeEvent, TurnId};

pub(crate) struct SessionActor {
    pub state: State,
    pub events: tokio::sync::broadcast::Sender<RuntimeEvent<UiEvent>>,
}
impl SessionActor {
    pub fn new(runtime: SessionRuntime) -> Self {
        Self {
            events: tokio::sync::broadcast::channel(256).0,
            state: State::Idle(Box::new(runtime)),
        }
    }
    pub fn emit(&self, event: UiEvent) {
        let turn_id = match &self.state {
            State::Running(turn) => turn.turn_id,
            _ => TurnId::default(),
        };
        let _ = self.events.send(RuntimeEvent { turn_id, event });
    }
    pub fn status(&self, line: impl Into<String>) {
        crate::tui_runtime::emit_helpers::status(self, line);
    }
    pub fn run(
        &mut self,
        mut commands: tokio::sync::mpsc::UnboundedReceiver<darius_tui::RuntimeCommand>,
    ) {
        block_on_turn(async {
            if let State::Idle(runtime) = &self.state {
                self.guidance(runtime);
            }
            loop {
                if matches!(self.state, State::Stopped) {
                    break;
                }
                tokio::select! {
                    biased;
                    result = async {
                        match &mut self.state {
                            State::Running(turn) => (&mut turn.join).await,
                            _ => std::future::pending().await,
                        }
                    } => self.restore(result),
                    command = commands.recv() => match command {
                        Some(command) => { if self.dispatch(command).await { break; } }
                        None => { self.shutdown().await; break; }
                    },
                }
            }
        });
    }
}
