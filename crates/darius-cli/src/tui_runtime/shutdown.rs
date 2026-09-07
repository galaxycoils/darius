//! Cooperative shutdown must join; abort cannot stop an already-running blocking task.
use super::actor::{SessionActor, State};
use darius_cognitive::UiEvent;
use std::time::Duration;
impl SessionActor {
    pub async fn shutdown(&mut self) {
        self.interrupt();
        let mut fatal = false;
        if let State::Running(turn) = &mut self.state {
            let result = tokio::time::timeout(Duration::from_secs(2), &mut turn.join).await;
            match result {
                Ok(result) => self.restore(result),
                Err(_) => fatal = true,
            }
        }
        if fatal {
            self.emit(UiEvent::Error {
                message: "Fatal: turn missed shutdown deadline; still joining, not stopped".into(),
            });
            // Never pretend abort() reaps a running spawn_blocking task or detach it.
            if let State::Running(turn) = &mut self.state {
                let result = (&mut turn.join).await;
                self.restore(result);
            }
        }
        self.state = State::Stopped;
    }
}
