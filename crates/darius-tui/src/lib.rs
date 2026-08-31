pub mod app;
pub mod commands;
pub mod controller;
pub mod input;
pub mod render;
pub mod terminal;
pub mod theme;

pub use app::{
    Action, AppState, Effort, Mode, PermissionChoice, PermissionRequest, PermissionState,
};
pub use controller::{ParsedCommand, RuntimeCommand, TuiController};
pub use render::{
    DiffLineKind, DiffLineView, DiffView, TaskDisplay, TaskStatus, ToolView, TranscriptItem,
};
pub use terminal::run_tui;

pub use darius_cognitive as cognitive;
