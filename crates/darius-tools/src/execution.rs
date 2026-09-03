//! Cancellable execution context and executor contract.
use std::time::Instant;

/// Cancel-vs-deadline: cancel wins ties (reports `Interrupted` past deadline).
pub struct ExecutionContext {
    pub cancel: tokio_util::sync::CancellationToken,
    pub deadline: Instant,
}
pub trait ToolExecutor: Send + Sync {
    fn execute(&self, call: &crate::ToolCall, ctx: &ExecutionContext) -> crate::ToolOutcome;
}
pub(crate) enum RunEnd {
    Done(Option<i32>, Vec<u8>, Vec<u8>),
    Interrupted,
    TimedOut,
}
pub(crate) type RunResult = Result<RunEnd, String>;
