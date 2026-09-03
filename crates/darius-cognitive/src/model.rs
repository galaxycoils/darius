//! Model contract: tool specs, validated output, async completion.
use crate::CognitiveError;
use crate::context::TurnContext;
use std::collections::HashSet;

/// Tool surface offered to the model for one turn.
#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Model turn output; tool ids must be non-empty and unique.
#[derive(Clone, Debug)]
pub struct ModelOutput {
    pub content: Option<String>,
    pub tool_calls: Vec<darius_tools::ToolCall>,
}

impl ModelOutput {
    pub fn validate(&self) -> Result<(), CognitiveError> {
        let mut seen = HashSet::new();
        for call in &self.tool_calls {
            if call.id.is_empty() {
                return Err(CognitiveError::InvalidPlan("empty tool id".into()));
            }
            if !seen.insert(call.id.as_str()) {
                return Err(CognitiveError::InvalidPlan("duplicate tool id".into()));
            }
        }
        Ok(())
    }
}

/// Cancellable async model; parallel to sync `Model` until Task 3.3.
#[async_trait::async_trait]
pub trait AsyncModel: Send {
    async fn complete(
        &mut self,
        messages: &[crate::conversation::Message],
        tools: &[ToolSpec],
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError>;
}
