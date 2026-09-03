use crate::agent_compact::compact_model_request;
use crate::agent_exec::execute_calls;
use crate::agent_loop::{AgentLoop, MAX_ROUNDS};
use crate::agent_validate::validate_new_calls;
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
        let rounds = policy.max_react_iters.clamp(1, MAX_ROUNDS);
        for _ in 0..rounds {
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
            compact_model_request(&mut view, &specs, policy.compress_opts.max_chars)?;
            let out = model.complete(&view, &specs, &ctx).await?;
            out.validate()?;
            validate_new_calls(msgs, &out.tool_calls)?;
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
        }
        Err(CognitiveError::Loop(format!(
            "agent loop reached {rounds}-round cap without terminal text"
        )))
    }
}
