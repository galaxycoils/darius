use crate::commands::CommandInvocation;
use darius_cognitive::UiEvent;
pub use darius_core::runtime_protocol::{Mode, PermissionChoice, TurnId};

// ── View types for rendering transcript items ──────────────────────────

/// A tool call view with its result.
#[derive(Debug, Clone)]
pub struct ToolView {
    pub name: String,
    pub args_preview: String,
    pub result: String,
    pub ok: bool,
}

/// A diff view with file, summary, and lines.
#[derive(Debug, Clone)]
pub struct DiffView {
    pub file: String,
    pub summary: String,
    pub lines: Vec<DiffLineView>,
}

/// A single diff line.
#[derive(Debug, Clone)]
pub struct DiffLineView {
    pub kind: DiffLineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

/// The kind of diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Add,
    Delete,
}

/// A transcript item to render.
#[derive(Debug, Clone)]
pub enum TranscriptItem {
    User { text: String },
    Assistant { text: String },
    Thinking { text: String, elapsed_ms: u64 },
    Tool { tool: ToolView, expanded: bool },
    Tasks { tasks: Vec<TaskDisplay> },
    Diff { diff: DiffView },
}

/// A task display with status glyph.
#[derive(Debug, Clone)]
pub struct TaskDisplay {
    pub title: String,
    pub status: TaskStatus,
}

/// Task status for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Done,
    Active,
    Todo,
}

/// Effects that the reducer can produce for the runtime to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    SubmitGoal(String),
    ExecuteCommand(CommandInvocation),
    Interrupt,
    ResolvePermission {
        id: String,
        choice: PermissionChoice,
    },
    Quit,
}

/// State for the command palette.
#[derive(Debug, Clone, Default)]
pub struct PaletteState {
    pub open: bool,
    pub selected: usize,
}

/// Active permission chooser state with selection.
#[derive(Debug, Clone)]
pub struct PermissionState {
    pub id: String,
    pub title: String,
    pub command: String,
    pub reason: String,
    pub selection: usize,
}

impl PermissionState {
    pub fn new(id: String, title: String, command: String, reason: String) -> Self {
        Self {
            id,
            title,
            command,
            reason,
            selection: 0,
        }
    }

    /// Current choice based on selection index.
    pub fn current_choice(&self) -> PermissionChoice {
        PermissionChoice::ALL[self.selection]
    }

    /// Move selection up (wraps to bottom).
    pub fn prev(&mut self) {
        if self.selection == 0 {
            self.selection = PermissionChoice::ALL.len() - 1;
        } else {
            self.selection -= 1;
        }
    }

    /// Move selection down (wraps to top).
    pub fn next(&mut self) {
        self.selection = (self.selection + 1) % PermissionChoice::ALL.len();
    }
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct ComposerState {
    pub input: String,
    pub cursor: usize,
    pub slash_mode: bool,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub profile: String,
    pub model: String,
    pub goal: Option<String>,
    pub transcript: Vec<TranscriptItem>,
    pub tasks: Vec<TaskDisplay>,
    pub running: bool,
    pub latest_turn: TurnId,
    pub mode: Mode,
    pub composer: ComposerState,
    pub permission_queue: Vec<PermissionRequest>,
    pub permission: Option<PermissionState>,
    pub palette: PaletteState,
    pub exit_requested: bool,
    pub interrupt_armed: bool,
    pub status_line: Option<String>,
    pub scroll: u16,
    pub cache_hit_ratio: Option<f64>,
    pub memory_chars: Option<usize>,
    pub running_subagents: usize,
    pub cwd: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Insert(char),
    Paste(String),
    Backspace,
    Delete,
    MoveCursorLeft,
    MoveCursorRight,
    CursorHome,
    CursorEnd,
    Submit,
    Quit,
    Cancel,
    Interrupt,
    OpenPalette,
    PaletteNext,
    PalettePrev,
    PaletteAccept,
    PaletteComplete,
    CycleMode,
    Scroll(i16),
    PermissionNext,
    PermissionPrev,
    PermissionChoose,
    ToggleTool(Option<usize>),
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            profile: "default".into(),
            model: "mock".into(),
            goal: None,
            transcript: vec![],
            tasks: vec![],
            running: false,
            latest_turn: TurnId::default(),
            mode: Mode::Auto,
            composer: ComposerState::default(),
            permission_queue: vec![],
            permission: None,
            palette: PaletteState::default(),
            exit_requested: false,
            interrupt_armed: false,
            status_line: None,
            scroll: 0,
            cache_hit_ratio: None,
            memory_chars: None,
            running_subagents: 0,
            cwd: None,
        }
    }
}

impl AppState {
    pub fn display_cwd(&self) -> String {
        if let Some(ref cwd) = self.cwd {
            let path_str = cwd.to_string_lossy();
            if let Ok(home) = std::env::var("HOME") {
                if let Some(rel) = path_str.strip_prefix(&home) {
                    return format!("~{rel}");
                }
            }
            path_str.into_owned()
        } else {
            ".".into()
        }
    }

    pub fn push_message(&mut self, msg: impl Into<String>) {
        self.transcript
            .push(TranscriptItem::Assistant { text: msg.into() });
    }

    pub fn set_tasks(&mut self, tasks: Vec<TaskDisplay>) {
        self.tasks = tasks;
    }

    pub fn push_permission(&mut self, id: String, reason: String) {
        self.permission_queue.push(PermissionRequest { id, reason });
    }

    pub fn approve_permission(&mut self, id: &str) -> bool {
        if let Some(pos) = self.permission_queue.iter().position(|p| p.id == id) {
            self.permission_queue.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn deny_permission(&mut self, id: &str) -> bool {
        if let Some(pos) = self.permission_queue.iter().position(|p| p.id == id) {
            self.permission_queue.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn next_permission(&self) -> Option<&PermissionRequest> {
        self.permission_queue.first()
    }

    pub fn toggle_tool(&mut self, index: Option<usize>) {
        self.reduce(Action::ToggleTool(index));
    }

    /// Apply a UI action to the state. Returns an optional permission choice
    /// when the user confirms via the permission chooser.
    pub fn apply_action(&mut self, action: Action) -> Option<PermissionChoice> {
        match action {
            Action::PermissionNext => {
                if let Some(ref mut perm) = self.permission {
                    perm.next();
                }
                None
            }
            Action::PermissionPrev => {
                if let Some(ref mut perm) = self.permission {
                    perm.prev();
                }
                None
            }
            Action::PermissionChoose => {
                if let Some(perm) = self.permission.take() {
                    let choice = perm.current_choice();
                    // Remove from queue as well
                    self.permission_queue.retain(|p| p.id != perm.id);
                    Some(choice)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Apply a UI action to the state, returning an optional Effect for the runtime to execute.
    pub fn reduce(&mut self, action: Action) -> Option<Effect> {
        // Permission chooser takes priority when active
        if self.permission.is_some() {
            return match action {
                Action::PermissionNext => {
                    if let Some(ref mut perm) = self.permission {
                        perm.next();
                    }
                    None
                }
                Action::PermissionPrev => {
                    if let Some(ref mut perm) = self.permission {
                        perm.prev();
                    }
                    None
                }
                Action::PermissionChoose => {
                    if let Some(perm) = self.permission.take() {
                        let choice = perm.current_choice();
                        self.permission_queue.retain(|p| p.id != perm.id);
                        Some(Effect::ResolvePermission {
                            id: perm.id,
                            choice,
                        })
                    } else {
                        None
                    }
                }
                Action::Cancel => {
                    if let Some(perm) = self.permission.take() {
                        self.permission_queue.retain(|p| p.id != perm.id);
                        Some(Effect::ResolvePermission {
                            id: perm.id,
                            choice: PermissionChoice::Deny,
                        })
                    } else {
                        None
                    }
                }
                Action::Interrupt => {
                    self.permission = None;
                    self.permission_queue.clear();
                    self.interrupt_armed = true;
                    Some(Effect::Interrupt)
                }
                Action::Quit => {
                    self.permission = None;
                    self.permission_queue.clear();
                    self.exit_requested = true;
                    Some(Effect::Quit)
                }
                _ => None,
            };
        }

        // Palette mode takes priority when open
        if self.palette.open {
            return match action {
                Action::PaletteNext => {
                    let filtered = crate::commands::filter(&self.composer.input);
                    if !filtered.is_empty() {
                        self.palette.selected = (self.palette.selected + 1) % filtered.len();
                    }
                    None
                }
                Action::PalettePrev => {
                    let filtered = crate::commands::filter(&self.composer.input);
                    if !filtered.is_empty() {
                        if self.palette.selected == 0 {
                            self.palette.selected = filtered.len() - 1;
                        } else {
                            self.palette.selected -= 1;
                        }
                    }
                    None
                }
                Action::PaletteComplete => {
                    let filtered = crate::commands::filter(&self.composer.input);
                    if let Some(cmd) = filtered.get(self.palette.selected) {
                        self.palette.open = false;
                        self.palette.selected = 0;
                        self.composer.input = format!("{} ", cmd.name);
                        self.composer.cursor = self.composer.input.chars().count();
                        self.composer.slash_mode = true;
                    }
                    None
                }
                Action::PaletteAccept => {
                    let filtered = crate::commands::filter(&self.composer.input);
                    if let Some(cmd) = filtered.get(self.palette.selected) {
                        self.palette.open = false;
                        self.palette.selected = 0;
                        self.composer.input.clear();
                        self.composer.cursor = 0;
                        self.composer.slash_mode = false;
                        match crate::commands::parse_invocation(cmd.name) {
                            Ok(invocation) => Some(Effect::ExecuteCommand(invocation)),
                            Err(e) => {
                                self.transcript.push(TranscriptItem::Assistant {
                                    text: format!("✗ {}", e),
                                });
                                None
                            }
                        }
                    } else {
                        None
                    }
                }
                Action::Cancel => {
                    self.palette.open = false;
                    self.palette.selected = 0;
                    None
                }
                Action::MoveCursorLeft => {
                    self.composer.cursor = self.composer.cursor.saturating_sub(1);
                    None
                }
                Action::MoveCursorRight => {
                    let max = self.composer.input.chars().count();
                    self.composer.cursor = (self.composer.cursor + 1).min(max);
                    None
                }
                Action::CursorHome => {
                    self.composer.cursor = 0;
                    None
                }
                Action::CursorEnd => {
                    self.composer.cursor = self.composer.input.chars().count();
                    None
                }
                Action::Delete => {
                    let cursor = self
                        .composer
                        .cursor
                        .min(self.composer.input.chars().count());
                    let mut chars: Vec<char> = self.composer.input.chars().collect();
                    if cursor < chars.len() {
                        chars.remove(cursor);
                        self.composer.input = chars.into_iter().collect();
                        if self.composer.input.is_empty() {
                            self.palette.open = false;
                            self.composer.slash_mode = false;
                        }
                        self.palette.selected = 0;
                    }
                    None
                }
                Action::Paste(text) => {
                    let normalized = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
                    let cursor = self
                        .composer
                        .cursor
                        .min(self.composer.input.chars().count());
                    let mut chars: Vec<char> = self.composer.input.chars().collect();
                    for (i, c) in normalized.chars().enumerate() {
                        chars.insert(cursor + i, c);
                    }
                    self.composer.cursor += normalized.chars().count();
                    self.composer.input = chars.into_iter().collect();
                    self.palette.selected = 0;
                    None
                }
                Action::Insert(c) => {
                    if self.composer.slash_mode {
                        let cursor = self
                            .composer
                            .cursor
                            .min(self.composer.input.chars().count());
                        let mut chars: Vec<char> = self.composer.input.chars().collect();
                        chars.insert(cursor, c);
                        self.composer.input = chars.into_iter().collect();
                        self.composer.cursor += 1;
                        self.palette.selected = 0;
                    }
                    None
                }
                Action::Backspace => {
                    if self.composer.slash_mode && self.composer.cursor > 0 {
                        let cursor = self
                            .composer
                            .cursor
                            .min(self.composer.input.chars().count());
                        let mut chars: Vec<char> = self.composer.input.chars().collect();
                        chars.remove(cursor - 1);
                        self.composer.input = chars.into_iter().collect();
                        self.composer.cursor -= 1;
                        if self.composer.input.is_empty() {
                            self.palette.open = false;
                            self.composer.slash_mode = false;
                        }
                        self.palette.selected = 0;
                    }
                    None
                }
                _ => None,
            };
        }

        // Normal mode
        match action {
            Action::Insert(c) => {
                // Insert at UTF-8 character boundary
                let cursor = self
                    .composer
                    .cursor
                    .min(self.composer.input.chars().count());
                let mut chars: Vec<char> = self.composer.input.chars().collect();
                chars.insert(cursor, c);
                self.composer.input = chars.into_iter().collect();
                self.composer.cursor += 1;
                // Open palette for / or leading -
                if self.composer.input == "/" || self.composer.input == "-" {
                    self.composer.slash_mode = true;
                    self.palette.open = true;
                    self.palette.selected = 0;
                }
                None
            }
            Action::Paste(text) => {
                let normalized = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
                let cursor = self
                    .composer
                    .cursor
                    .min(self.composer.input.chars().count());
                let mut chars: Vec<char> = self.composer.input.chars().collect();
                for (i, c) in normalized.chars().enumerate() {
                    chars.insert(cursor + i, c);
                }
                self.composer.cursor += normalized.chars().count();
                self.composer.input = chars.into_iter().collect();
                if self.composer.input.starts_with('/') || self.composer.input.starts_with('-') {
                    self.composer.slash_mode = true;
                    self.palette.open = true;
                    self.palette.selected = 0;
                }
                None
            }
            Action::MoveCursorLeft => {
                self.composer.cursor = self.composer.cursor.saturating_sub(1);
                None
            }
            Action::MoveCursorRight => {
                let max = self.composer.input.chars().count();
                self.composer.cursor = (self.composer.cursor + 1).min(max);
                None
            }
            Action::CursorHome => {
                self.composer.cursor = 0;
                None
            }
            Action::CursorEnd => {
                self.composer.cursor = self.composer.input.chars().count();
                self.scroll = 0;
                None
            }
            Action::Delete => {
                let cursor = self
                    .composer
                    .cursor
                    .min(self.composer.input.chars().count());
                let mut chars: Vec<char> = self.composer.input.chars().collect();
                if cursor < chars.len() {
                    chars.remove(cursor);
                    self.composer.input = chars.into_iter().collect();
                    if !self.composer.input.starts_with('/')
                        && !self.composer.input.starts_with('-')
                    {
                        self.composer.slash_mode = false;
                        self.palette.open = false;
                    }
                }
                None
            }
            Action::Backspace => {
                if self.composer.cursor > 0 {
                    let cursor = self
                        .composer
                        .cursor
                        .min(self.composer.input.chars().count());
                    let mut chars: Vec<char> = self.composer.input.chars().collect();
                    chars.remove(cursor - 1);
                    self.composer.input = chars.into_iter().collect();
                    self.composer.cursor -= 1;
                    // Exit slash mode if input no longer starts with / or -
                    if !self.composer.input.starts_with('/')
                        && !self.composer.input.starts_with('-')
                    {
                        self.composer.slash_mode = false;
                        self.palette.open = false;
                    }
                }
                None
            }
            Action::Submit => {
                let input = self.composer.input.trim().to_string();
                if input.is_empty() {
                    return None;
                }
                self.composer.input.clear();
                self.composer.cursor = 0;
                self.composer.slash_mode = false;
                self.palette.open = false;
                self.scroll = 0;
                self.status_line = None;

                // Check if it's a command (starts with / or -)
                if input.starts_with('/') || input.starts_with('-') {
                    match crate::commands::parse_invocation(&input) {
                        Ok(invocation) => Some(Effect::ExecuteCommand(invocation)),
                        Err(e) => {
                            self.transcript.push(TranscriptItem::Assistant {
                                text: format!("✗ {}", e),
                            });
                            None
                        }
                    }
                } else {
                    Some(Effect::SubmitGoal(input))
                }
            }
            Action::OpenPalette => {
                self.palette.open = true;
                self.palette.selected = 0;
                self.composer.slash_mode = true;
                None
            }
            Action::CycleMode => Some(Effect::ExecuteCommand(
                crate::commands::parse_invocation("/mode").unwrap(),
            )),
            Action::Scroll(delta) => {
                self.scroll = self.scroll.saturating_add_signed(delta);
                None
            }
            Action::Interrupt => {
                if self.running {
                    self.interrupt_armed = true;
                    Some(Effect::Interrupt)
                } else {
                    None
                }
            }
            Action::Cancel => {
                self.composer.input.clear();
                self.composer.cursor = 0;
                self.composer.slash_mode = false;
                self.palette.open = false;
                None
            }
            Action::Quit => {
                self.exit_requested = true;
                Some(Effect::Quit)
            }
            Action::ToggleTool(target) => {
                match target {
                    Some(idx) => {
                        if let Some(TranscriptItem::Tool { expanded, .. }) =
                            self.transcript.get_mut(idx)
                        {
                            *expanded = !*expanded;
                        }
                    }
                    None => {
                        if let Some(TranscriptItem::Tool { expanded, .. }) = self
                            .transcript
                            .iter_mut()
                            .rev()
                            .find(|i| matches!(i, TranscriptItem::Tool { .. }))
                        {
                            *expanded = !*expanded;
                        }
                    }
                }
                None
            }
            // Permission actions handled above when permission is active
            Action::PermissionNext | Action::PermissionPrev | Action::PermissionChoose => None,
            // Palette actions handled above when palette is open
            Action::PaletteNext
            | Action::PalettePrev
            | Action::PaletteAccept
            | Action::PaletteComplete => None,
        }
    }

    pub fn apply_event(&mut self, event: UiEvent) {
        match event {
            UiEvent::Header {
                profile,
                model,
                goal,
            } => {
                self.profile = profile;
                self.model = model;
                self.goal = Some(goal);
                self.running = true;
                self.status_line = None;
            }
            UiEvent::UserMessage { text } => {
                self.transcript.push(TranscriptItem::User { text });
            }
            UiEvent::AssistantDelta { text } => {
                // Coalesce consecutive deltas into one assistant item.
                if let Some(TranscriptItem::Assistant { text: existing }) =
                    self.transcript.last_mut()
                {
                    existing.push_str(&text);
                } else {
                    self.transcript.push(TranscriptItem::Assistant { text });
                }
            }
            UiEvent::Thinking {
                text, elapsed_ms, ..
            } => {
                self.transcript
                    .push(TranscriptItem::Thinking { text, elapsed_ms });
            }
            UiEvent::ToolStart {
                name, args_preview, ..
            } => {
                self.transcript.push(TranscriptItem::Tool {
                    tool: ToolView {
                        name,
                        args_preview,
                        result: String::new(),
                        ok: true,
                    },
                    expanded: false,
                });
            }
            UiEvent::ToolEnd { ok, preview, .. } => {
                // Pair with the most recent tool item.
                if let Some(TranscriptItem::Tool { tool, .. }) = self.transcript.last_mut() {
                    tool.ok = ok;
                    tool.result = preview;
                }
            }
            UiEvent::Diff {
                file,
                summary,
                lines,
                ..
            } => {
                let diff_lines: Vec<DiffLineView> = lines
                    .into_iter()
                    .map(|l| DiffLineView {
                        kind: match l.kind {
                            darius_cognitive::DiffKind::Context => DiffLineKind::Context,
                            darius_cognitive::DiffKind::Add => DiffLineKind::Add,
                            darius_cognitive::DiffKind::Delete => DiffLineKind::Delete,
                        },
                        old: l.old,
                        new: l.new,
                        text: l.text,
                    })
                    .collect();
                self.transcript.push(TranscriptItem::Diff {
                    diff: DiffView {
                        file,
                        summary,
                        lines: diff_lines,
                    },
                });
            }
            UiEvent::TaskBoard(tasks) => {
                self.tasks = tasks
                    .into_iter()
                    .map(|t| TaskDisplay {
                        title: t.title,
                        status: match t.status.as_str() {
                            "done" => TaskStatus::Done,
                            "active" => TaskStatus::Active,
                            _ => TaskStatus::Todo,
                        },
                    })
                    .collect();
                self.transcript.push(TranscriptItem::Tasks {
                    tasks: self.tasks.clone(),
                });
            }
            UiEvent::PermissionRequired {
                id,
                title,
                command,
                reason,
            } => {
                self.permission = Some(PermissionState::new(id, title, command, reason));
            }
            UiEvent::Accept { passed, notes } => {
                self.transcript.push(TranscriptItem::Assistant {
                    text: if passed {
                        format!("✓ Accepted — {notes}")
                    } else {
                        format!("✗ Rejected — {notes}")
                    },
                });
            }
            UiEvent::Status { line } => {
                self.transcript
                    .push(TranscriptItem::Assistant { text: line });
            }
            UiEvent::ModeChanged { mode } => {
                self.mode = mode;
                self.status_line = Some(mode.label().to_string());
            }
            UiEvent::Busy { message } => {
                self.status_line = Some(message);
            }
            UiEvent::ClearTranscript => {
                self.transcript.clear();
                self.scroll = 0;
            }
            UiEvent::Done => {
                self.running = false;
                self.status_line = Some("Done".into());
            }
            _ => {}
        }
    }
}

#[allow(clippy::field_reassign_with_default)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::map_key;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn ordinary_letters_edit_the_composer() {
        let state = AppState::default();
        assert_eq!(map_key(key('q'), &state), Some(Action::Insert('q')));
        assert_eq!(map_key(key('j'), &state), Some(Action::Insert('j')));
        assert_eq!(map_key(key('k'), &state), Some(Action::Insert('k')));
    }

    #[test]
    fn submitting_text_returns_runtime_goal_and_clears_input() {
        let mut state = AppState::default();
        state.composer.input = "hello".into();
        assert_eq!(
            state.reduce(Action::Submit),
            Some(Effect::SubmitGoal("hello".into()))
        );
        assert!(state.composer.input.is_empty());
    }

    #[test]
    fn mode_cycle() {
        let m = Mode::Auto;
        assert_eq!(m.next(), Mode::Plan);
        assert_eq!(m.next().next(), Mode::Auto);
    }

    #[test]
    fn tui_state_new() {
        let state = AppState::default();
        assert!(state.transcript.is_empty());
        assert!(state.tasks.is_empty());
        assert!(!state.running);
    }

    #[test]
    fn tui_state_push_message() {
        let mut state = AppState::default();
        state.push_message("hello");
        match state.transcript.last().unwrap() {
            TranscriptItem::Assistant { text } => assert_eq!(text, "hello"),
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn tui_state_set_tasks() {
        let mut state = AppState::default();
        state.set_tasks(vec![
            TaskDisplay {
                title: "task 1".into(),
                status: TaskStatus::Todo,
            },
            TaskDisplay {
                title: "task 2".into(),
                status: TaskStatus::Active,
            },
        ]);
        assert_eq!(state.tasks.len(), 2);
    }

    #[test]
    fn permission_queue() {
        let mut state = AppState::default();
        assert!(state.next_permission().is_none());

        state.push_permission("perm-1".into(), "Write file".into());
        assert!(state.next_permission().is_some());

        assert!(state.approve_permission("perm-1"));
        assert!(state.next_permission().is_none());

        state.push_permission("perm-2".into(), "Read file".into());
        assert!(state.deny_permission("perm-2"));
        assert!(state.next_permission().is_none());
    }

    #[test]
    fn apply_event_header() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::Header {
            profile: "work".into(),
            model: "gpt-4o-mini".into(),
            goal: "test".into(),
        });
        assert_eq!(state.profile, "work");
        assert_eq!(state.model, "gpt-4o-mini");
        assert!(state.running);
    }

    #[test]
    fn apply_event_user_message() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::UserMessage {
            text: "hello world".into(),
        });
        assert_eq!(state.transcript.len(), 1);
        match &state.transcript[0] {
            TranscriptItem::User { text } => assert_eq!(text, "hello world"),
            other => panic!("expected User, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_coalesces_consecutive_deltas() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::AssistantDelta {
            text: "Hello ".into(),
        });
        state.apply_event(UiEvent::AssistantDelta {
            text: "world".into(),
        });
        assert_eq!(state.transcript.len(), 1);
        match &state.transcript[0] {
            TranscriptItem::Assistant { text } => assert_eq!(text, "Hello world"),
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_delta_after_user_does_not_coalesce() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::UserMessage { text: "hi".into() });
        state.apply_event(UiEvent::AssistantDelta {
            text: "reply".into(),
        });
        assert_eq!(state.transcript.len(), 2);
        match &state.transcript[1] {
            TranscriptItem::Assistant { text } => assert_eq!(text, "reply"),
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_tool_start_end_pairing() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::ToolStart {
            id: "t1".into(),
            name: "read_file".into(),
            args_preview: "src/main.rs".into(),
        });
        // ToolStart creates a tool item with empty result.
        assert_eq!(state.transcript.len(), 1);
        match &state.transcript[0] {
            TranscriptItem::Tool { tool, .. } => {
                assert_eq!(tool.name, "read_file");
                assert_eq!(tool.args_preview, "src/main.rs");
                assert!(tool.result.is_empty());
                assert!(tool.ok);
            }
            other => panic!("expected Tool, got {:?}", other),
        }
        state.apply_event(UiEvent::ToolEnd {
            id: "t1".into(),
            ok: true,
            preview: "fn main() {}".into(),
            spilled: None,
        });
        // ToolEnd pairs with the last tool item.
        match &state.transcript[0] {
            TranscriptItem::Tool { tool, .. } => {
                assert_eq!(tool.result, "fn main() {}");
                assert!(tool.ok);
            }
            other => panic!("expected Tool, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_tool_end_fail() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::ToolStart {
            id: "t1".into(),
            name: "shell".into(),
            args_preview: "ls".into(),
        });
        state.apply_event(UiEvent::ToolEnd {
            id: "t1".into(),
            ok: false,
            preview: "permission denied".into(),
            spilled: None,
        });
        match &state.transcript[0] {
            TranscriptItem::Tool { tool, .. } => {
                assert!(!tool.ok);
                assert_eq!(tool.result, "permission denied");
            }
            other => panic!("expected Tool, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_task_status_mapping() {
        use darius_cognitive::{TaskSnapshot, UiEvent};
        let mut state = AppState::default();
        state.apply_event(UiEvent::TaskBoard(vec![
            TaskSnapshot {
                id: "1".into(),
                title: "done task".into(),
                status: "done".into(),
            },
            TaskSnapshot {
                id: "2".into(),
                title: "active task".into(),
                status: "active".into(),
            },
            TaskSnapshot {
                id: "3".into(),
                title: "todo task".into(),
                status: "todo".into(),
            },
            TaskSnapshot {
                id: "4".into(),
                title: "unknown task".into(),
                status: "something".into(),
            },
        ]));
        assert_eq!(state.tasks.len(), 4);
        assert_eq!(state.tasks[0].status, TaskStatus::Done);
        assert_eq!(state.tasks[0].title, "done task");
        assert_eq!(state.tasks[1].status, TaskStatus::Active);
        assert_eq!(state.tasks[2].status, TaskStatus::Todo);
        assert_eq!(state.tasks[3].status, TaskStatus::Todo); // unknown -> Todo
        // Also a Tasks item pushed to transcript.
        match &state.transcript[0] {
            TranscriptItem::Tasks { tasks } => assert_eq!(tasks.len(), 4),
            other => panic!("expected Tasks, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_diff_mapping() {
        use darius_cognitive::{DiffKind, DiffLine, UiEvent};
        let mut state = AppState::default();
        state.apply_event(UiEvent::Diff {
            file: "src/main.rs".into(),
            summary: "1 addition".into(),
            lines: vec![
                DiffLine {
                    kind: DiffKind::Context,
                    old: Some(1),
                    new: Some(1),
                    text: "fn main() {".into(),
                },
                DiffLine {
                    kind: DiffKind::Add,
                    old: None,
                    new: Some(2),
                    text: "    println!(\"hi\");".into(),
                },
                DiffLine {
                    kind: DiffKind::Delete,
                    old: Some(2),
                    new: None,
                    text: "    println!(\"bye\");".into(),
                },
            ],
        });
        assert_eq!(state.transcript.len(), 1);
        match &state.transcript[0] {
            TranscriptItem::Diff { diff } => {
                assert_eq!(diff.file, "src/main.rs");
                assert_eq!(diff.summary, "1 addition");
                assert_eq!(diff.lines.len(), 3);
                assert_eq!(diff.lines[0].kind, DiffLineKind::Context);
                assert_eq!(diff.lines[1].kind, DiffLineKind::Add);
                assert_eq!(diff.lines[2].kind, DiffLineKind::Delete);
            }
            other => panic!("expected Diff, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_permission_sets_state() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::PermissionRequired {
            id: "p1".into(),
            title: "Run shell command".into(),
            command: "ls -la".into(),
            reason: "List files".into(),
        });
        assert!(state.permission.is_some());
        let perm = state.permission.as_ref().unwrap();
        assert_eq!(perm.id, "p1");
        assert_eq!(perm.title, "Run shell command");
        assert_eq!(perm.command, "ls -la");
        assert_eq!(perm.reason, "List files");
    }

    #[test]
    fn apply_event_accept_passed() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::Accept {
            passed: true,
            notes: "all checks green".into(),
        });
        match &state.transcript[0] {
            TranscriptItem::Assistant { text } => {
                assert_eq!(text, "✓ Accepted — all checks green")
            }
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_accept_failed() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::Accept {
            passed: false,
            notes: "test failed".into(),
        });
        match &state.transcript[0] {
            TranscriptItem::Assistant { text } => {
                assert_eq!(text, "✗ Rejected — test failed")
            }
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_status_becomes_assistant_item() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::Status {
            line: "Compiling...".into(),
        });
        match &state.transcript[0] {
            TranscriptItem::Assistant { text } => assert_eq!(text, "Compiling..."),
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_done_clears_running_and_sets_status() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.running = true;
        state.apply_event(UiEvent::Done);
        assert!(!state.running);
        assert_eq!(state.status_line.as_deref(), Some("Done"));
    }

    #[test]
    fn apply_event_thinking() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.apply_event(UiEvent::Thinking {
            text: "analyzing".into(),
            elapsed_ms: 1234,
        });
        match &state.transcript[0] {
            TranscriptItem::Thinking { text, elapsed_ms } => {
                assert_eq!(text, "analyzing");
                assert_eq!(*elapsed_ms, 1234);
            }
            other => panic!("expected Thinking, got {:?}", other),
        }
    }

    #[test]
    fn apply_event_clear_transcript() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.transcript.push(TranscriptItem::User {
            text: "hello".into(),
        });
        state.scroll = 5;
        state.apply_event(UiEvent::ClearTranscript);
        assert!(state.transcript.is_empty());
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn command_palette_typing_slash_opens_and_filters() {
        let mut state = AppState::default();
        state.reduce(Action::Insert('/'));
        assert!(state.palette.open);
        assert!(state.composer.slash_mode);
        assert_eq!(state.composer.input, "/");

        state.reduce(Action::Insert('m'));
        assert_eq!(state.composer.input, "/m");
        let filtered = crate::commands::filter(&state.composer.input);
        assert!(filtered.iter().any(|c| c.name == "/model"));
        assert!(filtered.iter().any(|c| c.name == "/mode"));
        assert!(filtered.iter().any(|c| c.name == "/memory"));

        state.reduce(Action::Insert('d'));
        let filtered = crate::commands::filter(&state.composer.input);
        assert_eq!(filtered.len(), 2);

        state.reduce(Action::PaletteNext);
        assert_eq!(state.palette.selected, 1);
        state.reduce(Action::PalettePrev);
        assert_eq!(state.palette.selected, 0);

        state.reduce(Action::PaletteComplete);
        assert!(!state.palette.open);
        assert_eq!(state.composer.input, "/model ");
    }

    #[test]
    fn composer_cursor_navigation_and_editing() {
        let mut state = AppState::default();
        for c in "hello".chars() {
            state.reduce(Action::Insert(c));
        }
        assert_eq!(state.composer.input, "hello");
        assert_eq!(state.composer.cursor, 5);

        state.reduce(Action::MoveCursorLeft);
        state.reduce(Action::MoveCursorLeft);
        assert_eq!(state.composer.cursor, 3);

        state.reduce(Action::Delete);
        assert_eq!(state.composer.input, "helo");
        assert_eq!(state.composer.cursor, 3);

        state.reduce(Action::CursorHome);
        assert_eq!(state.composer.cursor, 0);

        state.reduce(Action::Delete);
        assert_eq!(state.composer.input, "elo");

        state.reduce(Action::CursorEnd);
        assert_eq!(state.composer.cursor, 3);

        state.reduce(Action::Backspace);
        assert_eq!(state.composer.input, "el");
        assert_eq!(state.composer.cursor, 2);
    }

    #[test]
    fn composer_auto_tail_scrolling() {
        let mut state = AppState::default();
        assert_eq!(state.scroll, 0);

        state.apply_event(UiEvent::UserMessage {
            text: "msg 1".into(),
        });
        assert_eq!(state.scroll, 0);

        state.reduce(Action::Scroll(3));
        assert_eq!(state.scroll, 3);

        state.apply_event(UiEvent::AssistantDelta {
            text: "resp".into(),
        });
        assert_eq!(state.scroll, 3);

        state.reduce(Action::CursorEnd);
        assert_eq!(state.scroll, 0);

        state.scroll = 4;
        state.composer.input = "new prompt".into();
        state.reduce(Action::Submit);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn composer_tool_disclosure_toggle() {
        let mut state = AppState::default();
        state.transcript.push(TranscriptItem::Tool {
            tool: ToolView {
                name: "read_file".into(),
                args_preview: "main.rs".into(),
                result: "ok".into(),
                ok: true,
            },
            expanded: false,
        });

        state.toggle_tool(None);
        match &state.transcript[0] {
            TranscriptItem::Tool { expanded, .. } => assert!(*expanded),
            _ => panic!("expected Tool"),
        }

        state.toggle_tool(Some(0));
        match &state.transcript[0] {
            TranscriptItem::Tool { expanded, .. } => assert!(!*expanded),
            _ => panic!("expected Tool"),
        }
    }

    // ── Permission chooser tests ─────────────────────────────────────

    #[test]
    fn permission_selection_wraps_down() {
        let mut perm = PermissionState::new(
            "p1".into(),
            "Run command".into(),
            "ls -la".into(),
            "List files".into(),
        );
        assert_eq!(perm.selection, 0);
        perm.next();
        assert_eq!(perm.selection, 1);
        perm.next();
        assert_eq!(perm.selection, 2);
        perm.next(); // wraps to 0
        assert_eq!(perm.selection, 0);
    }

    #[test]
    fn permission_selection_wraps_up() {
        let mut perm = PermissionState::new(
            "p1".into(),
            "Run command".into(),
            "ls -la".into(),
            "List files".into(),
        );
        assert_eq!(perm.selection, 0);
        perm.prev(); // wraps to last
        assert_eq!(perm.selection, 2);
        perm.prev();
        assert_eq!(perm.selection, 1);
    }

    #[test]
    fn permission_choose_returns_choice() {
        let mut state = AppState::default();
        state.permission = Some(PermissionState::new(
            "p1".into(),
            "Run command".into(),
            "ls -la".into(),
            "List files".into(),
        ));
        // Move to AllowSession
        state.permission.as_mut().unwrap().next();
        let choice = state.apply_action(Action::PermissionChoose);
        assert_eq!(choice, Some(PermissionChoice::AllowSession));
        // Permission is consumed
        assert!(state.permission.is_none());
    }

    #[test]
    fn permission_deny_consumes_and_returns_deny() {
        let mut state = AppState::default();
        state.permission = Some(PermissionState::new(
            "p1".into(),
            "Run command".into(),
            "ls -la".into(),
            "List files".into(),
        ));
        // Move to Deny (index 2)
        state.permission.as_mut().unwrap().next();
        state.permission.as_mut().unwrap().next();
        let choice = state.apply_action(Action::PermissionChoose);
        assert_eq!(choice, Some(PermissionChoice::Deny));
        assert!(state.permission.is_none());
    }

    #[test]
    fn permission_allow_session_persists_in_state() {
        let mut state = AppState::default();
        state.permission = Some(PermissionState::new(
            "p1".into(),
            "Run command".into(),
            "ls -la".into(),
            "List files".into(),
        ));
        state.permission.as_mut().unwrap().next(); // AllowSession
        let choice = state.apply_action(Action::PermissionChoose);
        assert_eq!(choice, Some(PermissionChoice::AllowSession));
    }
}
