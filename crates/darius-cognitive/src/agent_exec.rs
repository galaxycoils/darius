//! Per-call execution policy: allowlist gate, permission, one correlated result.
use crate::agent_events::{deny_call, emit_end, emit_start, emit_write_diff};
use crate::context::TurnContext;
use crate::conversation::Message;
use crate::{CognitiveError, EventSink, PermissionChoice, RunControl};
use darius_tools::{ToolCall, ToolOutcome, ToolRegistry, ToolRisk};

/// Model calls run through the allowlist gate (`execute_model`): unknown
/// tools are rejected before permission; one correlated result per call.
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
        let needs_approval = matches!(risk, Some(ToolRisk::Mutating | ToolRisk::Shell));
        if needs_approval
            && control.approve_tool(call, risk.unwrap_or(ToolRisk::Shell))?
                == PermissionChoice::Deny
        {
            deny_call(sink, msgs, call);
            continue;
        }
        emit_start(sink, call);
        match tools.execute_model(call) {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                emit_write_diff(sink, call, &preview);
                push_result(msgs, call, &preview);
                emit_end(sink, &call.id, true, &preview, spilled_path);
            }
            ToolOutcome::Err { message } => {
                push_result(msgs, call, &format!("Error: {message}"));
                emit_end(sink, &call.id, false, &message, None);
            }
            ToolOutcome::Interrupted => return Err(CognitiveError::Cancelled),
            ToolOutcome::TimedOut => {
                push_result(msgs, call, "Timed out");
                emit_end(sink, &call.id, false, "Timed out", None);
            }
        }
    }
    Ok(())
}
fn push_result(msgs: &mut Vec<Message>, call: &ToolCall, content: &str) {
    msgs.push(Message::Tool {
        tool_call_id: call.id.clone(),
        name: call.name.clone(),
        content: content.into(),
    });
}
