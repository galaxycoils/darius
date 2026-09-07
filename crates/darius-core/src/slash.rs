//! Neutral slash payload; availability remains in the TUI registry for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    Help,
    Clear,
    Compact,
    Model,
    Mode,
    Permissions,
    Memory,
    Pack,
    Tasks,
    Status,
    Config,
    Stop,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    pub id: CommandId,
    pub name: String,
    pub args: String,
}
