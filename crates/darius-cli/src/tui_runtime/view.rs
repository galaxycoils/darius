//! Immutable status snapshot available while the session is owned by a turn.
use crate::runtime::SessionRuntime;
pub(crate) struct SessionView {
    pub lines: Vec<String>,
}
impl SessionView {
    pub fn new(runtime: &SessionRuntime) -> Self {
        let mut lines = runtime.diagnostics().to_vec();
        lines.push(format!(
            "Profile: {}; model: {}; mode: {:?}; workspace: {}",
            runtime.metadata.profile,
            runtime.metadata.model,
            runtime.mode,
            runtime.workspace.display()
        ));
        lines.push(format!("Tasks: {}", runtime.task_board.lock().list().len()));
        Self { lines }
    }
}
