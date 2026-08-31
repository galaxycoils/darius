//! Darius TUI — ratatui session UI.

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::io;

/// Design tokens (copper-on-near-black).
mod tokens {
    use ratatui::style::Color;

    pub const TEXT: Color = Color::Rgb(0xe8, 0xea, 0xef);
    pub const MUTED: Color = Color::Rgb(0x8b, 0x93, 0xa7);
    pub const ACCENT: Color = Color::Rgb(0xe8, 0xa5, 0x4b);
    pub const OK: Color = Color::Rgb(0x3d, 0xd6, 0x8c);
}

/// TUI state.
pub struct TuiState {
    pub messages: Vec<String>,
    pub tasks: Vec<String>,
    pub input: String,
    pub slash_mode: bool,
    pub permission_queue: Vec<PermissionRequest>,
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub id: String,
    pub reason: String,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            messages: vec!["Welcome to Darius TUI. Type /help for commands.".into()],
            tasks: vec![],
            input: String::new(),
            slash_mode: false,
            permission_queue: vec![],
        }
    }

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
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new()
    }
}

/// Draw the TUI.
pub fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &TuiState,
) -> io::Result<()> {
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(5),    // stream
                Constraint::Length(8), // tasks
                Constraint::Length(3), // input
            ])
            .split(f.size());

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                "darius",
                Style::default()
                    .fg(tokens::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ ", Style::default().fg(tokens::MUTED)),
            Span::styled("profile: default", Style::default().fg(tokens::TEXT)),
            Span::styled(" │ ", Style::default().fg(tokens::MUTED)),
            Span::styled("model: mock", Style::default().fg(tokens::TEXT)),
        ]))
        .block(Block::default().borders(Borders::ALL).title(" Header "));
        f.render_widget(header, chunks[0]);

        // Stream
        let messages: Vec<ListItem> = state
            .messages
            .iter()
            .map(|m| {
                ListItem::new(Line::from(Span::styled(
                    m,
                    Style::default().fg(tokens::TEXT),
                )))
            })
            .collect();
        let stream =
            List::new(messages).block(Block::default().borders(Borders::ALL).title(" Stream "));
        f.render_widget(stream, chunks[1]);

        // Tasks
        let tasks: Vec<ListItem> = state
            .tasks
            .iter()
            .map(|t| ListItem::new(Line::from(Span::styled(t, Style::default().fg(tokens::OK)))))
            .collect();
        let task_list =
            List::new(tasks).block(Block::default().borders(Borders::ALL).title(" Tasks "));
        f.render_widget(task_list, chunks[2]);

        // Input
        let input_text = if state.slash_mode {
            format!("/{}", state.input)
        } else {
            state.input.clone()
        };
        let input = Paragraph::new(Line::from(Span::styled(
            input_text,
            Style::default().fg(tokens::ACCENT),
        )))
        .block(Block::default().borders(Borders::ALL).title(" Input "));
        f.render_widget(input, chunks[3]);
    })?;
    Ok(())
}

/// Run the TUI (blocking).
pub fn run_tui() -> io::Result<()> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut state = TuiState::new();

    terminal.clear()?;
    loop {
        draw(&mut terminal, &state)?;

        if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
            match key.code {
                crossterm::event::KeyCode::Char('/') => {
                    state.slash_mode = true;
                }
                crossterm::event::KeyCode::Char(c) if state.slash_mode => {
                    state.input.push(c);
                }
                crossterm::event::KeyCode::Backspace if state.slash_mode => {
                    state.input.pop();
                }
                crossterm::event::KeyCode::Enter if state.slash_mode => {
                    let cmd = state.input.clone();
                    state.input.clear();
                    state.slash_mode = false;
                    match cmd.as_str() {
                        "quit" | "q" => break,
                        "help" => state.push_message("Commands: /run /stop /memory /pack /tasks /plan /approve /deny /mode /help /quit"),
                        "approve" => {
                            if let Some(perm) = state.next_permission() {
                                let id = perm.id.clone();
                                state.approve_permission(&id);
                                state.push_message(format!("Approved: {id}"));
                            } else {
                                state.push_message("No pending permissions");
                            }
                        }
                        "deny" => {
                            if let Some(perm) = state.next_permission() {
                                let id = perm.id.clone();
                                state.deny_permission(&id);
                                state.push_message(format!("Denied: {id}"));
                            } else {
                                state.push_message("No pending permissions");
                            }
                        }
                        _ => state.push_message(format!("Unknown command: /{}", cmd)),
                    }
                }
                crossterm::event::KeyCode::Esc => {
                    state.slash_mode = false;
                    state.input.clear();
                }
                crossterm::event::KeyCode::Char('q') if !state.slash_mode => break,
                _ => {}
            }
        }
    }

    terminal.clear()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_state_new() {
        let state = TuiState::new();
        assert!(!state.messages.is_empty());
        assert!(state.tasks.is_empty());
        assert!(!state.slash_mode);
    }

    #[test]
    fn tui_state_push_message() {
        let mut state = TuiState::new();
        state.push_message("hello");
        assert_eq!(state.messages.last().unwrap(), "hello");
    }

    #[test]
    fn tui_state_set_tasks() {
        let mut state = TuiState::new();
        state.set_tasks(vec!["task 1".into(), "task 2".into()]);
        assert_eq!(state.tasks.len(), 2);
    }

    #[test]
    fn tui_permission_queue() {
        let mut state = TuiState::new();
        assert!(state.next_permission().is_none());

        state.push_permission("perm-1".into(), "Write file".into());
        assert!(state.next_permission().is_some());

        assert!(state.approve_permission("perm-1"));
        assert!(state.next_permission().is_none());

        state.push_permission("perm-2".into(), "Read file".into());
        assert!(state.deny_permission("perm-2"));
        assert!(state.next_permission().is_none());
    }
}
