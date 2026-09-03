//! Transcript rules: no empty/orphan/duplicate ids; every call resolved;
//! each result follows its call exactly once (causal order enforced).
use super::Message;
use crate::CognitiveError;
use std::collections::{HashMap, HashSet};

pub(super) fn validate(msgs: &[Message]) -> Result<(), CognitiveError> {
    let mut calls: HashMap<&str, usize> = HashMap::new();
    let mut results: HashSet<&str> = HashSet::new();
    for (idx, msg) in msgs.iter().enumerate() {
        match msg {
            Message::Assistant { tool_calls, .. } => {
                for call in tool_calls {
                    if call.id.is_empty() {
                        return Err(CognitiveError::InvalidPlan("empty tool id".into()));
                    }
                    if calls.insert(call.id.as_str(), idx).is_some() {
                        return Err(CognitiveError::InvalidPlan("duplicate tool id".into()));
                    }
                }
            }
            Message::Tool { tool_call_id, .. } => {
                if tool_call_id.is_empty() {
                    return Err(CognitiveError::InvalidPlan("empty tool id".into()));
                }
                if !results.insert(tool_call_id.as_str()) {
                    return Err(CognitiveError::InvalidPlan("duplicate tool result".into()));
                }
                match calls.get(tool_call_id.as_str()) {
                    Some(&call_idx) if call_idx < idx => {}
                    _ => return Err(CognitiveError::InvalidPlan("orphan tool result".into())),
                }
            }
            _ => {}
        }
    }
    for id in calls.keys() {
        if !results.contains(id) {
            return Err(CognitiveError::InvalidPlan("unresolved tool call".into()));
        }
    }
    Ok(())
}
