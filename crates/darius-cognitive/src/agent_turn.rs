//! One turn: stable prompt + bounded memory + specs, rounds, compaction, cap.
use crate::agent_compact::compact_tool_results;
use crate::agent_exec::execute_calls;
use crate::agent_loop::{AgentLoop, MAX_ROUNDS};
use crate::conversation::Message;
use crate::model::AsyncModel;
use crate::{CognitiveError, LoopPolicy, TurnContext, UiEvent};
use crate::{coding_system_prompt, memory_message, model_tool_specs};
impl AgentLoop {
    pub(crate) async fn drive(
        &self,
        policy: &LoopPolicy,
        msgs: &mut Vec<Message>,
        model: &mut dyn AsyncModel,
        tools: &darius_tools::ToolRegistry,
        memory: &darius_memory::MemoryEngine,
        workspace: &str,
    ) -> Result<String, CognitiveError> {
        let ctx = TurnContext::new();
        let prompt = coding_system_prompt(workspace);
        let specs = model_tool_specs();
        for _ in 0..MAX_ROUNDS {
            if self.control.is_cancelled() || ctx.is_cancelled() {
                return Err(CognitiveError::Cancelled);
            }
            let mut view = Vec::with_capacity(msgs.len() + 2);
            view.push(Message::System {
                content: prompt.clone(),
            });
            if let Some(memory) = memory_message(memory, policy.memory_max_chars) {
                view.push(memory);
            }
            view.extend(msgs.iter().cloned());
            let out = model.complete(&view, &specs, &ctx).await?;
            out.validate()?;
            if out.tool_calls.is_empty() {
                let text = out.content.unwrap_or_default();
                msgs.push(Message::Assistant {
                    content: Some(text.clone()),
                    tool_calls: vec![],
                });
                self.sink
                    .emit(UiEvent::AssistantDelta { text: text.clone() });
                return Ok(text);
            }
            msgs.push(Message::Assistant {
                content: out.content,
                tool_calls: out.tool_calls.clone(),
            });
            let (control, sink) = (self.control.as_ref(), self.sink.as_ref());
            execute_calls(&out.tool_calls, tools, control, sink, &ctx, msgs)?;
            compact_tool_results(msgs, policy.compress_opts.max_chars);
        }
        Err(CognitiveError::Loop(
            "agent loop reached 12-round cap without terminal text".into(),
        ))
    }
}
