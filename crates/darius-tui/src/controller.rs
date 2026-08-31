//! Controller channel — keeps model/tool execution off the draw/input loop.
//!
//! The TUI sends [`RuntimeCommand`]s over an mpsc channel; the runtime
//! emits [`UiEvent`]s over a broadcast channel that the TUI receives.

use darius_cognitive::UiEvent;

use crate::app::{Effort, Mode};

/// A parsed slash/dash command ready for execution.
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub name: String,
    pub args: String,
}

/// Commands sent from the TUI to the runtime.
#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    SubmitGoal {
        text: String,
        mode: Mode,
        effort: Effort,
    },
    ResolvePermission {
        id: String,
        choice: crate::app::PermissionChoice,
    },
    ExecuteSlash(ParsedCommand),
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

    #[tokio::test]
    async fn channel_ordering_is_fifo() {
        let (controller, mut cmd_rx, _event_tx) = TuiController::new(16, 16);

        // Send three commands in order.
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

        // Receive them in the same order.
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
}
