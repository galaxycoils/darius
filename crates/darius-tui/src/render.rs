use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io;

use crate::app::AppState;
use crate::theme::Theme;

pub fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &AppState,
) -> io::Result<()> {
    let theme = Theme::for_mode(crate::theme::ColorMode::Truecolor);
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