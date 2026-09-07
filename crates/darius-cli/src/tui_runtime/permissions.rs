//! Read the shared approval cache without waiting for the running session.
use super::actor::{SessionActor, State};
impl SessionActor {
    pub fn permissions(&self) {
        let cache = match &self.state {
            State::Idle(runtime) => &runtime.permissions,
            State::Running(turn) => &turn.control.session_cache,
            State::Stopped => return,
        };
        let mut approved: Vec<_> = cache
            .lock()
            .unwrap()
            .iter()
            .map(|(tool, target)| format!("{tool}: {target}"))
            .collect();
        approved.sort();
        self.status(format!("Session permissions: {} approved", approved.len()));
        for approval in approved {
            self.status(darius_safety::redact_secrets(&approval));
        }
    }
}
