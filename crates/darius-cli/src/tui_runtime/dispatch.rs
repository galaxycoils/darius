use super::actor::{SessionActor, State};
use darius_tui::{CommandId, RuntimeCommand};
impl SessionActor {
    pub async fn dispatch(&mut self, command: RuntimeCommand) -> bool {
        match command {
            RuntimeCommand::SubmitGoal { text, mode, .. } => self.submit(text, mode),
            RuntimeCommand::ResolvePermission { id, choice } => {
                if let State::Running(turn) = &self.state {
                    turn.control.resolve(&id, choice);
                }
            }
            RuntimeCommand::Interrupt => self.interrupt(),
            RuntimeCommand::Shutdown => {
                self.shutdown().await;
                return true;
            }
            RuntimeCommand::ExecuteSlash(inv) => {
                if inv.id == CommandId::Quit {
                    self.shutdown().await;
                    return true;
                }
                crate::command_handler::handle_slash(self, &inv);
            }
        }
        false
    }
}
