use darius_cognitive::{AgentLoop, EventSink, RunControl, UiEvent};
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
    pending: PendingPermissions,
    session_cache: Arc<Mutex<std::collections::HashSet<(String, String)>>>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl ChannelRunControl {
    pub fn new(
        sink: Arc<dyn EventSink>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            sink,
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

    fn normalize_target(name: &str, call: &darius_tools::ToolCall) -> String {
        match name {
            "write_file" => call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            "shell" => call
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        }
    }
}

impl RunControl for ChannelRunControl {
    fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    fn approve_tool(
        &self,
        call: &darius_tools::ToolCall,
        risk: ToolRisk,
    ) -> Result<darius_cognitive::PermissionChoice, darius_cognitive::CognitiveError> {
        let target = Self::normalize_target(&call.name, call);
        let cache_key = (call.name.clone(), target);

        {
            let cache = self.session_cache.lock().unwrap();
            if cache.contains(&cache_key) {
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
            command: format!("{:?}", call.arguments),
            reason: format!("Tool risk: {:?}", risk),
        });

        loop {
            if self.cancellation.is_cancelled() {
                return Err(darius_cognitive::CognitiveError::Cancelled);
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(choice) => {
                    if matches!(choice, darius_cognitive::PermissionChoice::AllowSession) {
                        let mut cache = self.session_cache.lock().unwrap();
                        cache.insert(cache_key);
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

/// The TUI runtime worker — owns the session and processes commands serially.
pub struct TuiWorker {
    runtime: SessionRuntime,
    control: Arc<ChannelRunControl>,
}

impl TuiWorker {
    pub fn new(runtime: SessionRuntime) -> (Self, tokio::sync::broadcast::Receiver<UiEvent>) {
        let event_sender = runtime.event_sender.clone();
        let cancellation = runtime.cancellation.clone();
        let control = Arc::new(ChannelRunControl::new(
            Arc::new(BroadcastEventSink(event_sender.clone())),
            cancellation,
        ));

        let worker = Self { runtime, control };
        let event_rx = event_sender.subscribe();

        (worker, event_rx)
    }

    pub fn control(&self) -> Arc<ChannelRunControl> {
        self.control.clone()
    }

    pub fn run_loop(&mut self, command_rx: std::sync::mpsc::Receiver<darius_tui::RuntimeCommand>) {
        if self.runtime.is_setup() {
            self.emit_setup_guidance();
        }
        loop {
            match command_rx.recv() {
                Ok(darius_tui::RuntimeCommand::SubmitGoal { text, .. }) => {
                    if self.runtime.is_setup() {
                        self.emit_setup_guidance();
                        continue;
                    }
                    if self.runtime.is_offline_demo() {
                        let _ = self.runtime.event_sender.send(UiEvent::Status {
                            line: "Runtime state: offline-demo".into(),
                        });
                        let _ = self.runtime.event_sender.send(UiEvent::Status {
                            line:
                                "Offline demo: no real file analysis or completion was performed."
                                    .into(),
                        });
                        continue;
                    }
                    let sink = Arc::new(BroadcastEventSink(self.runtime.event_sender.clone()));
                    let control = self.control.clone();
                    let loopt = AgentLoop::new(sink, control);
                    let workspace = self.runtime.workspace.to_string_lossy().to_string();
                    let _ = crate::runtime::block_on_turn(loopt.run_turn(
                        &self.runtime.metadata,
                        &self.runtime.policy,
                        &text,
                        &mut self.runtime.conversation,
                        self.runtime.model.as_mut(),
                        &self.runtime.tools,
                        &self.runtime.memory,
                        &workspace,
                    ));
                }
                Ok(darius_tui::RuntimeCommand::ExecuteSlash(inv)) => match inv.id {
                    darius_tui::CommandId::Status => {
                        for line in self.runtime.diagnostics() {
                            let _ = self
                                .runtime
                                .event_sender
                                .send(UiEvent::Status { line: line.clone() });
                        }
                    }
                    darius_tui::CommandId::Compact => {
                        let _ = self.runtime.event_sender.send(UiEvent::Status {
                            line: "Compacting session context (lean-tail)...".into(),
                        });
                        if let Ok(pack) = self
                            .runtime
                            .memory
                            .build_pack(self.runtime.policy.memory_max_chars, 12)
                        {
                            let _ = self.runtime.event_sender.send(UiEvent::Status {
                                line: format!(
                                    "Context compacted: {} memory records retained",
                                    pack.record_ids.len()
                                ),
                            });
                        }
                        let _ = self.runtime.event_sender.send(UiEvent::Done);
                    }
                    _ => {
                        let _ = self.runtime.event_sender.send(UiEvent::Status {
                            line: format!("Command: {}", inv.name),
                        });
                        let _ = self.runtime.event_sender.send(UiEvent::Done);
                    }
                },
                Ok(darius_tui::RuntimeCommand::ResolvePermission { id, choice }) => {
                    self.control.resolve(&id, choice.into());
                }
                Ok(darius_tui::RuntimeCommand::Interrupt) => {
                    self.runtime.cancellation.cancel();
                }
                Ok(darius_tui::RuntimeCommand::Shutdown) | Err(_) => {
                    self.runtime.cancellation.cancel();
                    break;
                }
            }
        }
    }
    fn emit_setup_guidance(&self) {
        let _ = self.runtime.event_sender.send(UiEvent::Status {
            line: "Setup required: set DARIUS_API_KEY or OPENAI_API_KEY, then run config init."
                .into(),
        });
        let _ = self.runtime.event_sender.send(UiEvent::Status {
            line: "No goal was run; no completion was claimed.".into(),
        });
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
