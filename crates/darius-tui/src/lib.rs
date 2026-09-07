// darius-tui: Claude-code-style TUI
#![allow(clippy::field_reassign_with_default)]
pub mod app;
mod app_events;
pub mod commands;
pub mod controller;
pub mod input;
pub mod render;
pub mod terminal;
pub mod theme;

pub use app::{
    Action, AppState, DiffLineKind, DiffLineView, DiffView, Effect, Mode, PaletteState,
    PermissionChoice, PermissionRequest, PermissionState, TaskDisplay, TaskStatus, ToolView,
    TranscriptItem,
};
pub use commands::{COMMANDS, CommandId, CommandInvocation, CommandSpec};
pub use controller::{RuntimeCommand, TuiController};
pub use terminal::run_tui;

pub use darius_cognitive as cognitive;
