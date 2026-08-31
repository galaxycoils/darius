// darius-tui: Claude-code-style TUI
#![allow(clippy::field_reassign_with_default)]
pub mod app;
pub mod commands;
pub mod controller;
pub mod input;
pub mod render;
pub mod terminal;
pub mod theme;

pub use app::{
    Action, AppState, DiffLineKind, DiffLineView, DiffView, Effect, Effort, Mode, PaletteState,
    PermissionChoice, PermissionRequest, PermissionState, TaskDisplay, TaskStatus, ToolView,
    TranscriptItem,
};
pub use controller::{ParsedCommand, RuntimeCommand, TuiController};
pub use terminal::run_tui;

pub use darius_cognitive as cognitive;