//! Apply only supported session modes; running-state rejection stays in dispatch.
use super::actor::{SessionActor, State};
use darius_cognitive::UiEvent;
use darius_core::runtime_protocol::Mode;
impl SessionActor {
    pub fn set_mode(&mut self, args: &str) {
        let State::Idle(runtime) = &mut self.state else {
            return;
        };
        let mode = match args {
            "" => runtime.mode.next(),
            "auto" => Mode::Auto,
            "plan" => Mode::Plan,
            _ => {
                self.emit(UiEvent::Error {
                    message: "/mode accepts only auto or plan".into(),
                });
                return;
            }
        };
        runtime.mode = mode;
        self.emit(UiEvent::ModeChanged { mode });
    }
}
