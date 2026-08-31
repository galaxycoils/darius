use ratatui::{
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
    Frame, Terminal,
};
use std::io;

use crate::app::AppState;
use crate::theme::{ColorMode, Theme};

/// Render the welcome / launch card in the given area.
///
/// Produces a compact single-border card:
/// ```text
/// ╭─ ◆ darius v1.1.1 ─────────────────────────────────────────╮
/// │ Welcome back                                              │
/// │ model   gpt-4o-mini                                       │
/// │ cwd     ~/dev/project                                     │
/// │ profile default  ·  kernel rust  ·  /help                 │
/// ╰────────────────────────────────────────────────────────────╯
/// ```
pub fn render_welcome(area: Rect, buf: &mut Buffer, state: &AppState, theme: &Theme) {
    let version = env!("CARGO_PKG_VERSION");
    let title = format!("◆ darius v{version}");

    let body = vec![
        Line::from(Span::styled("Welcome back", Style::default().fg(theme.text))),
        Line::from(vec![
            Span::styled("model   ", Style::default().fg(theme.muted)),
            Span::styled(&state.model, Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("cwd     ", Style::default().fg(theme.muted)),
            Span::styled("~/dev/project", Style::default().fg(theme.text)),
        ]),
        Line::from(vec![
            Span::styled("profile ", Style::default().fg(theme.muted)),
            Span::styled(&state.profile, Style::default().fg(theme.text)),
            Span::styled("  ·  kernel rust  ·  /help", Style::default().fg(theme.muted)),
        ]),
    ];

    let card = Paragraph::new(body).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.rule))
            .title(title),
    );
    card.render(area, buf);
}

/// Helper for snapshot tests: render the welcome card into a buffer
/// and return its string representation.
fn render_to_string(width: u16, height: u16, state: &AppState) -> String {
    let theme = Theme::for_mode(ColorMode::Truecolor);
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    render_welcome(area, &mut buffer, state, &theme);
    buffer_to_string(&buffer)
}

/// Convert a ratatui Buffer to a printable string for snapshot testing.
fn buffer_to_string(buffer: &Buffer) -> String {
    let mut result = String::new();
    for y in 0..buffer.area().height {
        let mut line = String::new();
        for x in 0..buffer.area().width {
            let cell = &buffer.content[((y * buffer.area().width) + x) as usize];
            let sym = cell.symbol();
            if sym.is_empty() {
                line.push(' ');
            } else {
                line.push_str(sym);
            }
        }
        // Trim trailing whitespace but keep the line
        let trimmed = line.trim_end();
        if !trimmed.is_empty() || y < buffer.area().height - 1 {
            result.push_str(trimmed);
        }
        if y < buffer.area().height - 1 {
            result.push('\n');
        }
    }
    result
}

pub fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &AppState,
) -> io::Result<()> {
    let theme = Theme::for_mode(ColorMode::Truecolor);
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(8),
                Constraint::Length(3),
            ])
            .split(f.size());

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled("darius", Style::default().fg(theme.brand).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.muted)),
            Span::styled(format!("profile: {}", state.profile), Style::default().fg(theme.text)),
            Span::styled(" │ ", Style::default().fg(theme.muted)),
            Span::styled(format!("model: {}", state.model), Style::default().fg(theme.text)),
        ]))
        .block(Block::default().borders(Borders::ALL).title(" Header "));
        f.render_widget(header, chunks[0]);

        // Stream
        let messages: Vec<ListItem> = state
            .messages
            .iter()
            .map(|m| ListItem::new(Line::from(Span::styled(m, Style::default().fg(theme.text)))))
            .collect();
        let stream = List::new(messages).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Stream "),
        );
        f.render_widget(stream, chunks[1]);

        // Tasks
        let tasks: Vec<ListItem> = state
            .tasks
            .iter()
            .map(|t| ListItem::new(Line::from(Span::styled(t, Style::default().fg(theme.add)))))
            .collect();
        let task_list = List::new(tasks).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Tasks "),
        );
        f.render_widget(task_list, chunks[2]);

        // Input
        let input_text = if state.composer.slash_mode {
            format!("/{}", state.composer.input)
        } else {
            state.composer.input.clone()
        };
        let input = Paragraph::new(Line::from(Span::styled(
            input_text,
            Style::default().fg(theme.brand),
        )))
        .block(Block::default().borders(Borders::ALL).title(" Input "));
        f.render_widget(input, chunks[3]);
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_state() -> AppState {
        let mut state = AppState::default();
        state.profile = "default".into();
        state.model = "gpt-4o-mini".into();
        state
    }

    #[test]
    fn welcome_card_80x24() {
        let state = fixture_state();
        let rendered = render_to_string(80, 24, &state);
        insta::assert_snapshot!("welcome_card_80x24", rendered);
    }

    #[test]
    fn welcome_card_120x36() {
        let state = fixture_state();
        let rendered = render_to_string(120, 36, &state);
        insta::assert_snapshot!("welcome_card_120x36", rendered);
    }
}