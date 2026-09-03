//! Correlate one tool outcome to one result and one ToolEnd event.
use crate::agent_events::{emit_end, emit_write_diff};
use crate::conversation::Message;
use crate::{CognitiveError, EventSink};
use darius_tools::{ToolCall, ToolOutcome};

pub(crate) fn record_outcome(
    outcome: ToolOutcome,
    call: &ToolCall,
    sink: &dyn EventSink,
    msgs: &mut Vec<Message>,
) -> Result<(), CognitiveError> {
    match outcome {
        ToolOutcome::Ok {
            preview,
            spilled_path,
        } => {
            emit_write_diff(sink, call, &preview);
            push(msgs, call, &preview);
            emit_end(sink, &call.id, true, &preview, spilled_path);
        }
        ToolOutcome::Err { message } => {
            push(msgs, call, &format!("Error: {message}"));
            emit_end(sink, &call.id, false, &message, None);
        }
        ToolOutcome::Interrupted => {
            push(msgs, call, "Interrupted");
            emit_end(sink, &call.id, false, "Interrupted", None);
            return Err(CognitiveError::Cancelled);
        }
        ToolOutcome::TimedOut => {
            push(msgs, call, "Timed out");
            emit_end(sink, &call.id, false, "Timed out", None);
        }
    }
    Ok(())
}

fn push(msgs: &mut Vec<Message>, call: &ToolCall, content: &str) {
    msgs.push(Message::Tool {
        tool_call_id: call.id.clone(),
        name: call.name.clone(),
        content: content.into(),
    });
}
