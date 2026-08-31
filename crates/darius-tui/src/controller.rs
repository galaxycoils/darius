//! Controller channel — keeps model/tool execution off the draw/input loop.
//!
//! The TUI sends [`RuntimeCommand`]s over an mpsc channel; the runtime
//! emits [`UiEvent`]s over a broadcast channel that the TUI receives.

use darius_cognitive::UiEvent;

use crate::app::{Effort, Mode};
use crate::commands::CommandInvocation;

/// Commands sent from the TUI to the runtime.
#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    SubmitGoal {
        text: String,
        mode: Mode,
        effort: Effort,
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
    pub commands: tokio::sync::mpsc::Sender<RuntimeCommand>,
    pub events: tokio::sync::broadcast::Receiver<UiEvent>,
}

impl TuiController {
    /// Create a new controller with the given channel capacities.
    pub fn new(
        command_capacity: usize,
        event_capacity: usize,
    ) -> (
        Self,
        tokio::sync::mpsc::Receiver<RuntimeCommand>,
        tokio::sync::broadcast::Sender<UiEvent>,
    ) {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(command_capacity);
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
    use crate::app::{Effort, Mode};
    use crate::commands::CommandId;

    fn dummy_invocation() -> CommandInvocation {
        CommandInvocation {
            id: CommandId::Help,
            name: "/help".into(),
            args: String::new(),
        }
    }

    #[tokio::test]
    async fn channel_ordering_is_fifo() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(16, 16);

        controller
            .commands
            .send(RuntimeCommand::Interrupt)
            .await
            .unwrap();
        controller
            .commands
            .send(RuntimeCommand::Shutdown)
            .await
            .unwrap();
        controller
            .commands
            .send(RuntimeCommand::Interrupt)
            .await
            .unwrap();

        assert!(matches!(
            cmd_rx.recv().await,
            Some(RuntimeCommand::Interrupt)
        ));
        assert!(matches!(
            cmd_rx.recv().await,
            Some(RuntimeCommand::Shutdown)
        ));
        assert!(matches!(
            cmd_rx.recv().await,
            Some(RuntimeCommand::Interrupt)
        ));
    }

    #[tokio::test]
    async fn broadcast_delivers_events_to_receiver() {
        let (mut controller, _cmd_rx, event_tx) = TuiController::new(16, 16);

        event_tx.send(UiEvent::Done).unwrap();
        event_tx
            .send(UiEvent::Status {
                line: "hello".into(),
            })
            .unwrap();

        assert!(matches!(controller.events.recv().await, Ok(UiEvent::Done)));
        assert!(
            matches!(controller.events.recv().await, Ok(UiEvent::Status { line }) if line == "hello")
        );
    }

    #[tokio::test]
    async fn shutdown_command_drops_cleanly() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(16, 16);
        controller
            .commands
            .send(RuntimeCommand::Shutdown)
            .await
            .unwrap();
        // Drop the sender to simulate runtime shutdown.
        drop(controller);
        // The queued message is still delivered.
        assert!(matches!(
            cmd_rx.recv().await,
            Some(RuntimeCommand::Shutdown)
        ));
        // Then the channel closes.
        assert!(cmd_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn submit_goal_carries_text_mode_and_effort() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(16, 16);
        controller
            .commands
            .send(RuntimeCommand::SubmitGoal {
                text: "build a feature".into(),
                mode: Mode::Plan,
                effort: Effort::Max,
            })
            .await
            .unwrap();

        match cmd_rx.recv().await {
            Some(RuntimeCommand::SubmitGoal { text, mode, effort }) => {
                assert_eq!(text, "build a feature");
                assert_eq!(mode, Mode::Plan);
                assert_eq!(effort, Effort::Max);
            }
            other => panic!("expected SubmitGoal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn execute_slash_uses_canonical_invocation() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(16, 16);
        controller
            .commands
            .send(RuntimeCommand::ExecuteSlash(dummy_invocation()))
            .await
            .unwrap();

        match cmd_rx.recv().await {
            Some(RuntimeCommand::ExecuteSlash(inv)) => {
                assert_eq!(inv.id, CommandId::Help);
                assert_eq!(inv.name, "/help");
                assert!(inv.args.is_empty());
            }
            other => panic!("expected ExecuteSlash, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn resolve_permission_carries_id_and_choice() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(16, 16);
        controller
            .commands
            .send(RuntimeCommand::ResolvePermission {
                id: "perm-1".into(),
                choice: crate::app::PermissionChoice::AllowOnce,
            })
            .await
            .unwrap();

        match cmd_rx.recv().await {
            Some(RuntimeCommand::ResolvePermission { id, choice }) => {
                assert_eq!(id, "perm-1");
                assert_eq!(choice, crate::app::PermissionChoice::AllowOnce);
            }
            other => panic!("expected ResolvePermission, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn lag_event_receivers_get_latest_after_catch_up() {
        let (_controller, _cmd_rx, event_tx) = TuiController::new(16, 4);
        // Fill the channel beyond capacity.
        for i in 0..10 {
            let _ = event_tx.send(UiEvent::Status {
                line: format!("evt-{i}"),
            });
        }
        // The channel should still function (lagging receivers get Lag error).
        assert!(event_tx.send(UiEvent::Done).is_ok());
    }

    #[tokio::test]
    async fn closed_command_channel_signals_runtime_stop() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(16, 16);
        // Drop the controller's sender immediately.
        drop(controller);
        // The runtime sees None and should stop.
        assert!(cmd_rx.recv().await.is_none());
    }
}