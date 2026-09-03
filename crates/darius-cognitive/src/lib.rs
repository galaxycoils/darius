//! Agent loop — multi-turn coding turns over a shared Conversation.

pub mod agent_compact;
pub mod agent_events;
pub mod agent_exec;
pub mod agent_loop;
pub mod agent_turn;
pub mod compress;
pub mod context;
pub mod conversation;
pub mod model;
pub mod skills;
pub mod subagent;
pub mod system_prompt;
pub mod tool_specs;

pub use agent_compact::*;
pub use agent_events::*;
pub use agent_exec::*;
pub use agent_loop::*;
pub use compress::*;
pub use context::*;
pub use conversation::*;
pub use model::*;
pub use subagent::*;
pub use system_prompt::*;
pub use tool_specs::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CognitiveError {
    #[error("loop error: {0}")]
    Loop(String),
    #[error("tool error: {0}")]
    Tool(#[from] darius_tools::ToolError),
    #[error("memory error: {0}")]
    Memory(#[from] darius_memory::MemoryError),
    #[error("invalid plan: {0}")]
    InvalidPlan(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("board error: {0}")]
    Board(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<String> for CognitiveError {
    fn from(s: String) -> Self {
        CognitiveError::Board(s)
    }
}

pub mod ui_events;
pub use ui_events::*;

/// Control handle for cancellation and tool approval.
pub trait RunControl: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn approve_tool(
        &self,
        call: &darius_tools::ToolCall,
        risk: darius_tools::ToolRisk,
    ) -> Result<PermissionChoice, CognitiveError>;
}

/// No-op RunControl for headless runs — never cancelled, auto-approves tools.
pub struct NoopRunControl;

impl RunControl for NoopRunControl {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn approve_tool(
        &self,
        _call: &darius_tools::ToolCall,
        _risk: darius_tools::ToolRisk,
    ) -> Result<PermissionChoice, CognitiveError> {
        Ok(PermissionChoice::AllowOnce)
    }
}

/// Run metadata — emitted in the Header event so consumers know which
/// profile, model, and mode produced a given session.
#[derive(Debug, Clone)]
pub struct RunMetadata {
    pub profile: String,
    pub model: String,
    pub mode: String,
}

/// Loop policy configuration.
#[derive(Debug, Clone)]
pub struct LoopPolicy {
    pub max_tasks: usize,
    pub max_react_iters: usize,
    pub memory_max_chars: usize,
    pub tool_preview_ceiling: usize,
    pub require_plan: bool,
    pub require_acceptance: bool,
    pub compress_opts: CompressOpts,
}

impl Default for LoopPolicy {
    fn default() -> Self {
        Self {
            max_tasks: 15,
            max_react_iters: 12,
            memory_max_chars: 3500,
            tool_preview_ceiling: 32768,
            require_plan: true,
            require_acceptance: true,
            compress_opts: CompressOpts::default(),
        }
    }
}

/// Scripted AsyncModel for tests — replays outputs, then answers "done".
pub struct MockModel {
    outputs: Vec<ModelOutput>,
    index: usize,
}

impl MockModel {
    pub fn new(outputs: Vec<ModelOutput>) -> Self {
        Self { outputs, index: 0 }
    }
}

#[async_trait::async_trait]
impl AsyncModel for MockModel {
    async fn complete(
        &mut self,
        _messages: &[crate::conversation::Message],
        _tools: &[ToolSpec],
        ctx: &TurnContext,
    ) -> Result<ModelOutput, CognitiveError> {
        if ctx.is_cancelled() || ctx.is_expired() {
            return Err(CognitiveError::Cancelled);
        }
        let out = self
            .outputs
            .get(self.index)
            .cloned()
            .unwrap_or(ModelOutput {
                content: Some("done".into()),
                tool_calls: vec![],
            });
        self.index += 1;
        Ok(out)
    }
}
