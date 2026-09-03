//! Temporary sync-`Model` bridge; removed in Task 3.3 after caller audit.
use crate::CognitiveError;
use crate::context::TurnContext;
use crate::conversation::Message;
use crate::model::{AsyncModel, ModelOutput, ToolSpec};

/// Delegates to sync `plan()` over the latest user text; honors cancel.
pub struct LegacyModelAdapter {
    inner: Box<dyn crate::Model>,
}

impl LegacyModelAdapter {
    pub fn new(inner: Box<dyn crate::Model>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl AsyncModel for LegacyModelAdapter {
    async fn complete(
        &mut self,
        messages: &[Message],
        _tools: &[ToolSpec],
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        if ctx.is_cancelled() || ctx.is_expired() {
            return Err(CognitiveError::Cancelled);
        }
        let goal = messages
            .iter()
            .rev()
            .find_map(|m| match m {
                Message::User { content } => Some(content.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let content = self.inner.plan(&goal)?;
        Ok(ModelOutput {
            content: Some(content),
            tool_calls: vec![],
        })
    }
}
