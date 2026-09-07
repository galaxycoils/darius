//! Reject stale turn events before they can change the visible session.
use crate::app::AppState;
use darius_cognitive::UiEvent;
use darius_core::runtime_protocol::{RuntimeEvent, TurnId};
impl AppState {
    pub fn apply_runtime_event(&mut self, envelope: RuntimeEvent<UiEvent>) {
        let RuntimeEvent { turn_id, event } = envelope;
        if turn_id != TurnId::default() {
            if turn_id < self.latest_turn {
                return;
            }
            if matches!(event, UiEvent::Header { .. }) {
                if turn_id == self.latest_turn {
                    return;
                }
                self.latest_turn = turn_id;
                self.permission = None;
                self.interrupt_armed = false;
            } else if turn_id != self.latest_turn || !self.running {
                return;
            }
        }
        self.apply_event(event);
    }
}
