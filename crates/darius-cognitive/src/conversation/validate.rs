//! Transcript rules: no empty/orphan/duplicate ids; every call resolved.
use super::Message;
use crate::CognitiveError;
use std::collections::HashSet;

pub(super) fn validate(msgs: &[Message]) -> Result<(), CognitiveError> {
    let mut calls: HashSet<&str> = HashSet::new();
    let mut results: HashSet<&str> = HashSet::new();
    for msg in msgs {
        match msg {
            Message::Assistant { tool_calls, .. } => {
                for call in tool_calls {
                    if call.id.is_empty() {
                        return Err(CognitiveError::InvalidPlan("empty tool id".into()));
                    }
                    if !calls.insert(call.id.as_str()) {
                        return Err(CognitiveError::InvalidPlan("duplicate tool id".into()));
                    }
                }
            }
            Message::Tool { tool_call_id, .. } => {
                results.insert(tool_call_id.as_str());
            }
            _ => {}
        }
    }
    for id in &calls {
        if !results.contains(id) {
            return Err(CognitiveError::InvalidPlan("unresolved tool call".into()));
        }
    }
    for id in &results {
        if !calls.contains(id) {
            return Err(CognitiveError::InvalidPlan("orphan tool result".into()));
        }
    }
    Ok(())
}
