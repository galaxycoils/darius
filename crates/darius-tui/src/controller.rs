//! Controller channel — keeps model/tool execution off the draw/input loop.
//!
//! The TUI sends [`RuntimeCommand`]s over an mpsc channel; the runtime
//! emits [`UiEvent`]s over a broadcast channel that the TUI receives.

use darius_cognitive::UiEvent;
use darius_core::runtime_protocol::RuntimeEvent;

use crate::app::Mode;
use crate::commands::CommandInvocation;

/// Commands sent from the TUI to the runtime.
#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    SubmitGoal {
        text: String,
        mode: Mode,
    },
    ExecuteSlash(CommandInvocation),
    ResolvePermission {
        id: String,
        choice: crate::app::PermissionChoice,
    },
    Interrupt,
    Shutdown,
}

/// The TUI's handle to the runtime: send commands, receive events.
pub struct TuiController {
    pub commands: tokio::sync::mpsc::UnboundedSender<RuntimeCommand>,
    pub events: tokio::sync::broadcast::Receiver<RuntimeEvent<UiEvent>>,
}

impl TuiController {
    /// Create a new controller with the given channel capacities.
    pub fn new(
        event_capacity: usize,
    ) -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<RuntimeCommand>,
        tokio::sync::broadcast::Sender<RuntimeEvent<UiEvent>>,
    ) {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::broadcast::channel(event_capacity);
        (
            Self {
                commands: cmd_tx,
                events: event_rx,
            },
            cmd_rx,
            event_tx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use crate::commands::CommandId;

    fn dummy_invocation() -> CommandInvocation {
        CommandInvocation {
            id: CommandId::Help,
            name: "/help".into(),
            args: String::new(),
        }
    }

    #[test]
    fn channel_ordering_is_fifo() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(64);

        controller.commands.send(RuntimeCommand::Interrupt).unwrap();
        controller.commands.send(RuntimeCommand::Shutdown).unwrap();
        controller.commands.send(RuntimeCommand::Interrupt).unwrap();

        assert!(matches!(
            cmd_rx.blocking_recv().unwrap(),
            RuntimeCommand::Interrupt
        ));
        assert!(matches!(
            cmd_rx.blocking_recv().unwrap(),
            RuntimeCommand::Shutdown
        ));
        assert!(matches!(
            cmd_rx.blocking_recv().unwrap(),
            RuntimeCommand::Interrupt
        ));
    }

    #[test]
    fn broadcast_delivers_events_to_receiver() {
        let (mut controller, _cmd_rx, event_tx) = TuiController::new(64);

        event_tx.send(UiEvent::Done.into()).unwrap();
        event_tx
            .send(
                UiEvent::Status {
                    line: "hello".into(),
                }
                .into(),
            )
            .unwrap();

        assert!(matches!(
            controller.events.try_recv().unwrap().event,
            UiEvent::Done
        ));
        assert!(
            matches!(controller.events.try_recv().unwrap().event, UiEvent::Status { line } if line == "hello")
        );
    }

    #[test]
    fn shutdown_command_drops_cleanly() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(64);
        controller.commands.send(RuntimeCommand::Shutdown).unwrap();
        drop(controller);
        assert!(matches!(
            cmd_rx.blocking_recv().unwrap(),
            RuntimeCommand::Shutdown
        ));
        assert!(cmd_rx.blocking_recv().is_none());
    }

    #[test]
    fn submit_goal_carries_text_and_mode() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(64);
        controller
            .commands
            .send(RuntimeCommand::SubmitGoal {
                text: "build a feature".into(),
                mode: Mode::Plan,
            })
            .unwrap();

        match cmd_rx.blocking_recv().unwrap() {
            RuntimeCommand::SubmitGoal { text, mode } => {
                assert_eq!(text, "build a feature");
                assert_eq!(mode, Mode::Plan);
            }
            other => panic!("expected SubmitGoal, got {other:?}"),
        }
    }

    #[test]
    fn execute_slash_uses_canonical_invocation() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(64);
        controller
            .commands
            .send(RuntimeCommand::ExecuteSlash(dummy_invocation()))
            .unwrap();

        match cmd_rx.blocking_recv().unwrap() {
            RuntimeCommand::ExecuteSlash(inv) => {
                assert_eq!(inv.id, CommandId::Help);
                assert_eq!(inv.name, "/help");
                assert!(inv.args.is_empty());
            }
            other => panic!("expected ExecuteSlash, got {other:?}"),
        }
    }

    #[test]
    fn resolve_permission_carries_id_and_choice() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(64);
        controller
            .commands
            .send(RuntimeCommand::ResolvePermission {
                id: "perm-1".into(),
                choice: crate::app::PermissionChoice::AllowOnce,
            })
            .unwrap();

        match cmd_rx.blocking_recv().unwrap() {
            RuntimeCommand::ResolvePermission { id, choice } => {
                assert_eq!(id, "perm-1");
                assert_eq!(choice, crate::app::PermissionChoice::AllowOnce);
            }
            other => panic!("expected ResolvePermission, got {other:?}"),
        }
    }

    #[test]
    fn lag_event_receivers_get_latest_after_catch_up() {
        let (_controller, _cmd_rx, event_tx) = TuiController::new(4);
        for i in 0..10 {
            let _ = event_tx.send(
                UiEvent::Status {
                    line: format!("evt-{i}"),
                }
                .into(),
            );
        }
        assert!(event_tx.send(UiEvent::Done.into()).is_ok());
    }

    #[test]
    fn closed_command_channel_signals_runtime_stop() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(64);
        drop(controller);
        assert!(cmd_rx.blocking_recv().is_none());
    }
}
