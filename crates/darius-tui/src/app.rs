use darius_cognitive::UiEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Auto,
    Manual,
    AcceptEdits,
    Plan,
}

impl Mode {
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Manual,
            Self::Manual => Self::AcceptEdits,
            Self::AcceptEdits => Self::Plan,
            Self::Plan => Self::Auto,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "⏵⏵ auto mode on",
            Self::Manual => "⏸ manual mode on",
            Self::AcceptEdits => "⏵⏵ accept edits on",
            Self::Plan => "⏸ plan mode on",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Effort {
    #[default]
    Low,
    Medium,
    High,
    XHigh,
    Max,
    Ultracode,
}

impl Effort {
    pub fn chip(self) -> &'static str {
        match self {
            Self::Low => "○ low",
            Self::Medium => "◐ medium",
            Self::High => "● high",
            Self::XHigh => "◉ xhigh",
            Self::Max => "◈ max",
            Self::Ultracode => "✦ ultracode",
        }
    }
}

/// The three permission choices matching the interaction contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionChoice {
    AllowOnce,
    AllowSession,
    Deny,
}

impl PermissionChoice {
    /// The three options in display order.
    pub const ALL: [Self; 3] = [Self::AllowOnce, Self::AllowSession, Self::Deny];

    /// Short label for the rose permission box.
    pub fn label(self) -> &'static str {
        match self {
            Self::AllowOnce => "Yes",
            Self::AllowSession => "Yes, and don't ask again this session",
            Self::Deny => "No, and tell Darius what to do (esc)",
        }
    }
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
    pub slash_mode: bool,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub profile: String,
    pub model: String,
    pub goal: Option<String>,
    pub messages: Vec<String>,
    pub tasks: Vec<String>,
    pub running: bool,
    pub mode: Mode,
    pub effort: Effort,
    pub composer: ComposerState,
    pub permission_queue: Vec<PermissionRequest>,
    pub permission: Option<PermissionState>,
    pub scroll: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Insert(char),
    Backspace,
    Submit,
    Quit,
    Cancel,
    Interrupt,
    OpenPalette,
    PaletteNext,
    PalettePrev,
    PaletteAccept,
    CycleMode,
    CycleEffort,
    Scroll(i16),
    ToggleTool,
    PermissionNext,
    PermissionPrev,
    PermissionChoose,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            profile: "default".into(),
            model: "mock".into(),
            goal: None,
            messages: vec!["Welcome to Darius TUI. Type /help for commands.".into()],
            tasks: vec![],
            running: false,
            mode: Mode::Auto,
            effort: Effort::High,
            composer: ComposerState::default(),
            permission_queue: vec![],
            permission: None,
            scroll: 0,
        }
    }
}

impl AppState {
    pub fn push_message(&mut self, msg: impl Into<String>) {
        self.messages.push(msg.into());
    }

    pub fn set_tasks(&mut self, tasks: Vec<String>) {
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
            }
            UiEvent::UserMessage { text } => {
                self.messages.push(format!("❯ {text}"));
            }
            UiEvent::AssistantDelta { text } => {
                self.messages.push(text);
            }
            UiEvent::Thinking { text, .. } => {
                self.messages.push(format!("✦ {text}"));
            }
            UiEvent::ToolStart {
                name, args_preview, ..
            } => {
                self.messages.push(format!("⏺ {name}({args_preview})"));
            }
            UiEvent::ToolEnd { ok, preview, .. } => {
                let mark = if ok { "✓" } else { "✗" };
                self.messages.push(format!("⎿ {mark} {preview}"));
            }
            UiEvent::TaskBoard(tasks) => {
                self.tasks = tasks
                    .into_iter()
                    .map(|t| format!("{:?}: {}", t.status, t.title))
                    .collect();
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
                self.messages.push(if passed {
                    format!("✓ Accepted — {notes}")
                } else {
                    format!("✗ Rejected — {notes}")
                });
            }
            UiEvent::Status { line } => {
                self.messages.push(line);
            }
            UiEvent::Done => {
                self.running = false;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycle() {
        let m = Mode::Auto;
        assert_eq!(m.next(), Mode::Manual);
        assert_eq!(m.next().next(), Mode::AcceptEdits);
        assert_eq!(m.next().next().next(), Mode::Plan);
        assert_eq!(m.next().next().next().next(), Mode::Auto);
    }

    #[test]
    fn effort_chips() {
        assert_eq!(Effort::Low.chip(), "○ low");
        assert_eq!(Effort::Ultracode.chip(), "✦ ultracode");
    }

    #[test]
    fn tui_state_new() {
        let state = AppState::default();
        assert!(!state.messages.is_empty());
        assert!(state.tasks.is_empty());
        assert!(!state.running);
    }

    #[test]
    fn tui_state_push_message() {
        let mut state = AppState::default();
        state.push_message("hello");
        assert_eq!(state.messages.last().unwrap(), "hello");
    }

    #[test]
    fn tui_state_set_tasks() {
        let mut state = AppState::default();
        state.set_tasks(vec!["task 1".into(), "task 2".into()]);
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
    fn apply_event_done() {
        use darius_cognitive::UiEvent;
        let mut state = AppState::default();
        state.running = true;
        state.apply_event(UiEvent::Done);
        assert!(!state.running);
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
