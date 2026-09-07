mod actor;
mod dispatch;
mod emit_helpers;
mod guidance;
mod lifecycle;
mod mode;
mod permissions;
mod shutdown;
mod state;
pub(crate) use actor::SessionActor;
pub(crate) use state::State;
#[cfg(test)]
mod tests;
mod turn;
mod view;
use darius_cognitive::{EventSink, RunControl, UiEvent};
use darius_tools::ToolRisk;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::paths::{DariusPaths, OsEnv};
use crate::runtime::SessionRuntime;

type PendingPermissions = Arc<
    Mutex<
        Vec<(
            String,
            std::sync::mpsc::Sender<darius_cognitive::PermissionChoice>,
        )>,
    >,
>;

/// Channel-backed RunControl — emits PermissionRequired and blocks on a
/// one-shot response from the TUI. Session-scoped approvals are cached so
/// the user is not prompted twice for the same tool+target in one session.
pub struct ChannelRunControl {
    sink: Arc<dyn EventSink>,
    paths: darius_tools::PathPolicy,
    mode: darius_cognitive::ExecutionPolicy,
    pending: PendingPermissions,
    session_cache: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl ChannelRunControl {
    pub fn new(
        sink: Arc<dyn EventSink>,
        cancellation: tokio_util::sync::CancellationToken,
        paths: darius_tools::PathPolicy,
    ) -> Self {
        Self {
            sink,
            paths,
            mode: darius_cognitive::ExecutionPolicy::Auto,
            pending: Arc::new(Mutex::new(Vec::new())),
            session_cache: Arc::new(Mutex::new(std::collections::HashSet::new())),
            cancellation,
        }
    }

    pub fn resolve(&self, id: &str, choice: darius_cognitive::PermissionChoice) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(pos) = pending.iter().position(|(pid, _)| pid == id) {
            let (_, tx) = pending.remove(pos);
            let _ = tx.send(choice);
        }
    }
}

impl RunControl for ChannelRunControl {
    fn execution_policy(&self) -> darius_cognitive::ExecutionPolicy {
        self.mode
    }
    fn cancellation_token(&self) -> tokio_util::sync::CancellationToken {
        self.cancellation.clone()
    }
    fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    fn approve_tool(
        &self,
        call: &darius_tools::ToolCall,
        risk: ToolRisk,
    ) -> Result<darius_cognitive::PermissionChoice, darius_cognitive::CognitiveError> {
        let cache_key = crate::permissions::key(call, &self.paths);

        {
            let cache = self.session_cache.lock().unwrap();
            if cache_key.as_ref().is_some_and(|key| cache.contains(key)) {
                return Ok(darius_cognitive::PermissionChoice::AllowOnce);
            }
        }

        let (tx, rx) = std::sync::mpsc::channel();

        {
            let mut pending = self.pending.lock().unwrap();
            pending.push((call.id.clone(), tx));
        }

        self.sink.emit(UiEvent::PermissionRequired {
            id: call.id.clone(),
            title: format!("Execute {}", call.name),
            command: darius_safety::redact_secrets(&format!("{:?}", call.arguments)),
            reason: format!("Tool risk: {:?}", risk),
        });

        loop {
            if self.cancellation.is_cancelled() {
                self.pending
                    .lock()
                    .unwrap()
                    .retain(|(id, _)| id != &call.id);
                return Err(darius_cognitive::CognitiveError::Cancelled);
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(choice) => {
                    if matches!(choice, darius_cognitive::PermissionChoice::AllowSession)
                        && let Some(key) = cache_key
                    {
                        self.session_cache.lock().unwrap().insert(key);
                    }
                    return Ok(choice);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(darius_cognitive::CognitiveError::Cancelled);
                }
            }
        }
    }
}

/// Adapter from broadcast::Sender to EventSink.
pub struct BroadcastEventSink(pub tokio::sync::broadcast::Sender<UiEvent>);

impl EventSink for BroadcastEventSink {
    fn emit(&self, event: UiEvent) {
        let _ = self.0.send(event);
    }
}

/// The production worker owns the actor; commands remain serviceable during turns.
pub struct TuiWorker {
    actor: actor::SessionActor,
}
impl TuiWorker {
    pub fn new(
        runtime: SessionRuntime,
    ) -> (
        Self,
        tokio::sync::broadcast::Receiver<darius_core::runtime_protocol::RuntimeEvent<UiEvent>>,
    ) {
        let actor = actor::SessionActor::new(runtime);
        let events = actor.events.subscribe();
        (Self { actor }, events)
    }
    pub fn run_loop(
        &mut self,
        commands: tokio::sync::mpsc::UnboundedReceiver<darius_tui::RuntimeCommand>,
    ) {
        self.actor.run(commands);
    }
}

/// Build a session runtime from a profile name.
pub fn build_runtime(
    profile: &str,
    offline: bool,
) -> Result<SessionRuntime, crate::runtime::RuntimeError> {
    let paths = DariusPaths::resolve(&OsEnv, None)?;
    SessionRuntime::from_options(&paths, profile, crate::runtime::RuntimeOptions { offline })
}

/// Build a session runtime with a custom working directory.
pub fn build_runtime_with_cwd(
    profile: &str,
    cwd: PathBuf,
    offline: bool,
) -> Result<SessionRuntime, crate::runtime::RuntimeError> {
    let paths = DariusPaths::resolve(&OsEnv, Some(&cwd))?;
    let runtime =
        SessionRuntime::from_options(&paths, profile, crate::runtime::RuntimeOptions { offline })?;
    std::env::set_current_dir(cwd)?;
    Ok(runtime)
}
