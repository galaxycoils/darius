//! Reusable typed emission helpers for the actor.
use super::actor::{SessionActor, State};
use darius_cognitive::UiEvent;
use darius_core::runtime_protocol::{RuntimeEvent, TurnId};

pub(super) fn status(actor: &SessionActor, line: impl Into<String>) {
    let turn_id = match &actor.state {
        State::Running(turn) => turn.turn_id,
        _ => TurnId::default(),
    };
    let _ = actor.events.send(RuntimeEvent {
        turn_id,
        event: UiEvent::Status { line: line.into() },
    });
}
