//! Reject tool-call identifiers already present in the transcript.
use crate::CognitiveError;
use crate::conversation::Message;
use darius_tools::ToolCall;

pub(crate) fn validate_new_calls(
    msgs: &[Message],
    calls: &[ToolCall],
) -> Result<(), CognitiveError> {
    for call in calls {
        let reused = msgs.iter().any(|msg| match msg {
            Message::Assistant { tool_calls, .. } => {
                tool_calls.iter().any(|prior| prior.id == call.id)
            }
            _ => false,
        });
        if reused {
            return Err(CognitiveError::InvalidPlan("duplicate tool id".into()));
        }
    }
    Ok(())
}
