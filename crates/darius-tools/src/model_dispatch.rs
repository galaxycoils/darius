//! Model dispatch that threads the caller-owned execution context into shell.
use crate::execution::{ExecutionContext, ToolExecutor};
use crate::{ToolCall, ToolOutcome, ToolRegistry, ToolRisk};

pub(crate) fn execute(
    registry: &ToolRegistry,
    call: &ToolCall,
    ctx: &ExecutionContext,
) -> ToolOutcome {
    if ctx.cancel.is_cancelled() {
        return ToolOutcome::Interrupted;
    }
    if std::time::Instant::now() >= ctx.deadline {
        return ToolOutcome::TimedOut;
    }
    let clean = crate::model_tools::sanitize_call(call);
    if !crate::model_tools::is_model_tool(&clean.name) {
        return crate::model_tools::hidden_tool_error(&clean);
    }
    if clean.name != "shell" || registry.risk("shell") != Some(ToolRisk::Shell) {
        return registry.execute(&clean);
    }
    crate::shell::ShellExecutor {
        workspace: registry.workspace_root().to_path_buf(),
        spill_dir: registry.spill_dir().to_path_buf(),
        ceiling: registry.preview_ceiling(),
    }
    .execute(&clean, ctx)
}
