use darius_cognitive::*;
use std::sync::{Arc, Mutex};
pub struct Observer(pub Arc<Mutex<Vec<String>>>);
#[async_trait]
impl AsyncModel for Observer {
    async fn complete(
        &mut self,
        _: &[Message],
        _: &[ToolSpec],
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        self.0.lock().unwrap().push(ctx.session_id().into());
        Ok(ModelOutput {
            content: Some("ok".into()),
            tool_calls: vec![],
        })
    }
}
