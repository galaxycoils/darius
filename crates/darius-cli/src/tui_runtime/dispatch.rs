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
            RuntimeCommand::SelectModel(cfg) => {
                if let State::Idle(runtime) = &mut self.state {
                    match runtime.apply_model_config(&cfg) {
                        Ok(Some(warning)) => {
                            self.status(format!(
                                "Model active: {} ({}) — {}",
                                cfg.model, cfg.provider, warning
                            ));
                        }
                        Ok(None) => {
                            self.status(format!("Model active: {} ({})", cfg.model, cfg.provider));
                        }
                        Err(e) => {
                            self.emit(darius_cognitive::UiEvent::Error {
                                message: format!("Failed to apply model {}: {e}", cfg.model),
                            });
                        }
                    }
                }
            }
        }
        false
    }
}
