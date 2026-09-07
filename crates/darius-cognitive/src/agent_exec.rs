//! Per-call allowlist, permission, context-aware execution, correlated result.
use crate::agent_events::{deny_call, emit_start};
use crate::agent_outcome::record_outcome;
use crate::context::TurnContext;
use crate::conversation::Message;
use crate::{CognitiveError, EventSink, PermissionChoice, RunControl};
use darius_tools::{ToolCall, ToolRegistry, ToolRisk};

pub fn execute_calls(
    calls: &[ToolCall],
    tools: &ToolRegistry,
    control: &dyn RunControl,
    sink: &dyn EventSink,
    ctx: &TurnContext,
    msgs: &mut Vec<Message>,
) -> Result<(), CognitiveError> {
    for call in calls {
        if ctx.is_cancelled() || control.is_cancelled() {
            return Err(CognitiveError::Cancelled);
        }
        let risk = darius_tools::model_tools::model_tool_risk(&call.name);
        if let Some(reason) = crate::execution_policy::denial(control.execution_policy(), risk) {
            record_outcome(
                darius_tools::ToolOutcome::Err {
                    message: reason.into(),
                },
                call,
                sink,
                msgs,
            )?;
            continue;
        }
        let needs_approval = matches!(risk, Some(ToolRisk::Mutating | ToolRisk::Shell));
        if needs_approval
            && control.approve_tool(call, risk.unwrap_or(ToolRisk::Shell))?
                == PermissionChoice::Deny
        {
            deny_call(sink, msgs, call);
            continue;
        }
        emit_start(sink, call);
        let execution = ctx.execution_context();
        let outcome = tools.execute_model_with_context(call, &execution);
        record_outcome(outcome, call, sink, msgs)?;
    }
    Ok(())
}
