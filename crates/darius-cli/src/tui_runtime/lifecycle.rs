use super::actor::{Running, SessionActor, State};
use super::view::SessionView;
use darius_cognitive::UiEvent;
impl SessionActor {
    pub fn submit(&mut self, text: String, mode: darius_core::runtime_protocol::Mode) {
        let State::Idle(runtime) = &self.state else {
            self.emit(UiEvent::Busy {
                message: "Busy: turn running".into(),
            });
            return;
        };
        if self.guidance(runtime) {
            return;
        }
        let State::Idle(mut runtime) = std::mem::replace(&mut self.state, State::Stopped) else {
            unreachable!()
        };
        runtime.mode = mode;
        let view = SessionView::new(&runtime);
        let turn_id = darius_core::runtime_protocol::TurnId::next();
        let (join, control) = super::turn::spawn(*runtime, text, turn_id, self.events.clone());
        self.state = State::Running(Running {
            turn_id,
            join,
            control,
            view,
        });
    }
    pub fn restore(&mut self, result: Result<super::turn::TurnResult, tokio::task::JoinError>) {
        let turn_id = match &self.state {
            State::Running(turn) => turn.turn_id,
            _ => return,
        };
        match result {
            Ok(result) => {
                let _outcome = result.outcome; // The agent emitted any terminal error already.
                self.state = State::Idle(Box::new(result.runtime));
            }
            Err(_) => {
                self.state = State::Stopped;
                self.emit(UiEvent::Error {
                    message: "Fatal: turn task panicked".into(),
                });
            }
        }
        let _ = self
            .events
            .send(darius_core::runtime_protocol::RuntimeEvent {
                turn_id,
                event: UiEvent::Done,
            });
    }
    pub fn interrupt(&self) {
        if let State::Running(turn) = &self.state {
            turn.control.cancellation.cancel();
        }
    }
}
