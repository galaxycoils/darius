use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};
use std::io;

use crate::app::{AppState, PermissionChoice, PermissionState};
use crate::commands::{COMMANDS, CommandSpec};
use crate::theme::{ColorMode, Theme};

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

// ── Render functions ───────────────────────────────────────────────────

/// Render a user message: `❯ text`.
pub fn render_user(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::styled("❯ ", Style::default().fg(theme.brand)),
        Span::styled(text.to_string(), Style::default().fg(theme.text)),
    ])]
}

/// Render assistant text (no role chip).
pub fn render_assistant(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    text.lines()
        .map(|line| {
            Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(theme.text),
            ))
        })
        .collect()
}

/// Render thinking: `✦ <verb>... (<elapsed>ms)`.
pub fn render_thinking(text: &str, elapsed_ms: u64, theme: &Theme) -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::styled("✦ ", Style::default().fg(theme.muted)),
        Span::styled(
            format!("{} ({}ms)", text, elapsed_ms),
            Style::default().fg(theme.muted),
        ),
    ])]
}

/// Render a tool call with its result.
pub fn render_tool(tool: &ToolView, expanded: bool, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled("⏺ ", Style::default().fg(theme.brand)),
        Span::styled(
            format!("{}({})", tool.name, tool.args_preview),
            Style::default().fg(theme.text),
        ),
    ])];

    if expanded {
        for result_line in tool.result.lines() {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(theme.muted)),
                Span::styled("⎿ ", Style::default().fg(theme.muted)),
                Span::styled(
                    result_line.to_string(),
                    Style::default().fg(if tool.ok { theme.text } else { theme.delete }),
                ),
            ]));
        }
    }

    lines
}

/// Render a task board with status glyphs.
pub fn render_tasks(tasks: &[TaskDisplay], theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "Update Todos",
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    ))];

    for task in tasks {
        let (glyph, color) = match task.status {
            TaskStatus::Done => ("✓", theme.muted),
            TaskStatus::Active => ("◐", theme.auto_mode),
            TaskStatus::Todo => ("○", theme.muted),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {glyph} "), Style::default().fg(color)),
            Span::styled(task.title.clone(), Style::default().fg(theme.text)),
        ]));
    }

    lines
}

/// Render a diff with filename, summary, and line-numbered rows.
pub fn render_diff(diff: &DiffView, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            diff.file.clone(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", diff.summary),
            Style::default().fg(theme.muted),
        ),
    ])];

    for line in &diff.lines {
        let (prefix, color) = match line.kind {
            DiffLineKind::Context => (" ", theme.muted),
            DiffLineKind::Add => ("+", theme.add),
            DiffLineKind::Delete => ("-", theme.delete),
        };
        let old_num = line.old.map(|n| n.to_string()).unwrap_or_default();
        let new_num = line.new.map(|n| n.to_string()).unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{old_num:>4} {new_num:>4} {prefix} "),
                Style::default().fg(color),
            ),
            Span::styled(line.text.clone(), Style::default().fg(color)),
        ]));
    }

    lines
}

/// Render a transcript item to lines.
pub fn render_transcript_item(item: &TranscriptItem, theme: &Theme) -> Vec<Line<'static>> {
    match item {
        TranscriptItem::User { text } => render_user(text, theme),
        TranscriptItem::Assistant { text } => render_assistant(text, theme),
        TranscriptItem::Thinking { text, elapsed_ms } => render_thinking(text, *elapsed_ms, theme),
        TranscriptItem::Tool { tool, expanded } => render_tool(tool, *expanded, theme),
        TranscriptItem::Tasks { tasks } => render_tasks(tasks, theme),
        TranscriptItem::Diff { diff } => render_diff(diff, theme),
    }
}

// ── Welcome card ───────────────────────────────────────────────────────

/// Render the welcome / launch card in the given area.
pub fn render_welcome(area: Rect, buf: &mut Buffer, state: &AppState, theme: &Theme) {
    let version = env!("CARGO_PKG_VERSION");
    let title = format!("◆ darius v{version}");

    let body = vec![
        Line::from(Span::styled(
            "Welcome back",
            Style::default().fg(theme.text),
        )),
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
            Span::styled(
                "  ·  kernel rust  ·  /help",
                Style::default().fg(theme.muted),
            ),
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

// ── Permission chooser ────────────────────────────────────────────────

/// Render the rose permission chooser box with the three options.
/// The selected option is marked with `❯`.
pub fn render_permission(area: Rect, buf: &mut Buffer, perm: &PermissionState, theme: &Theme) {
    let mut body = vec![
        Line::from(vec![Span::styled(
            &perm.title,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            &perm.command,
            Style::default().fg(theme.text),
        )]),
        Line::from(vec![Span::styled(
            &perm.reason,
            Style::default().fg(theme.muted),
        )]),
        Line::from(Span::raw("")),
    ];

    for (i, choice) in PermissionChoice::ALL.iter().enumerate() {
        let marker = if i == perm.selection { "❯" } else { " " };
        let color = if i == perm.selection {
            theme.active
        } else {
            theme.text
        };
        body.push(Line::from(vec![Span::styled(
            format!("{marker} {}", choice.label()),
            Style::default().fg(color),
        )]));
    }

    let card = Paragraph::new(body).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.permission))
            .title(" Permission Required "),
    );
    card.render(area, buf);
}

// ── Composer ───────────────────────────────────────────────────────────

/// Render the dual-rule composer with effort chip and mode footer.
pub fn render_composer(area: Rect, buf: &mut Buffer, state: &AppState, theme: &Theme) {
    let width = area.width as usize;
    let rule = "─".repeat(width.saturating_sub(2));

    let mut lines = vec![];

    // Effort chip line
    let effort_text = format!("{} · /effort", state.effort.chip());
    lines.push(Line::from(vec![Span::styled(
        effort_text,
        Style::default().fg(theme.muted),
    )]));

    // Top rule
    lines.push(Line::from(Span::styled(
        &rule,
        Style::default().fg(theme.rule),
    )));

    // Input line
    let input_text = if state.composer.slash_mode {
        format!("/{}", state.composer.input)
    } else {
        state.composer.input.clone()
    };
    lines.push(Line::from(vec![
        Span::styled("❯ ", Style::default().fg(theme.brand)),
        Span::styled(input_text, Style::default().fg(theme.text)),
    ]));

    // Bottom rule
    lines.push(Line::from(Span::styled(
        &rule,
        Style::default().fg(theme.rule),
    )));

    // Mode footer
    let mode_text = format!(
        "{} (shift+tab to cycle) · ? for shortcuts",
        state.mode.label()
    );
    lines.push(Line::from(vec![Span::styled(
        mode_text,
        Style::default().fg(theme.muted),
    )]));

    let composer = Paragraph::new(lines);
    composer.render(area, buf);
}

// ── Slash palette ──────────────────────────────────────────────────────

/// Render the slash command palette above the composer.
pub fn render_palette(
    area: Rect,
    buf: &mut Buffer,
    query: &str,
    selected_idx: usize,
    theme: &Theme,
) {
    let filtered: Vec<&CommandSpec> = if query.is_empty() {
        COMMANDS.iter().collect()
    } else {
        let q = query.to_lowercase();
        COMMANDS
            .iter()
            .filter(|cmd| cmd.name.contains(&q) || cmd.description.to_lowercase().contains(&q))
            .collect()
    };

    let mut items: Vec<ListItem> = Vec::new();

    // Header
    items.push(ListItem::new(Line::from(vec![Span::styled(
        "Commands",
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    )])));

    for (i, cmd) in filtered.iter().enumerate() {
        let marker = if i == selected_idx { "❯" } else { " " };
        let color = if i == selected_idx {
            theme.active
        } else {
            theme.text
        };
        let name_width = 20;
        let name = format!("{:width$}", cmd.name, width = name_width);
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{marker} "), Style::default().fg(color)),
            Span::styled(name, Style::default().fg(color)),
            Span::styled(cmd.description, Style::default().fg(theme.muted)),
        ])));
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.rule))
            .title(" Slash Commands "),
    );
    list.render(area, buf);
}

// ── Snapshot helpers ───────────────────────────────────────────────────

/// Render a transcript to a string for snapshot testing.
#[cfg(test)]
fn render_transcript_to_string(width: u16, height: u16, items: &[TranscriptItem]) -> String {
    let theme = Theme::for_mode(ColorMode::Truecolor);
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);

    // Render items vertically starting at y=0
    let mut y_offset: u16 = 0;
    for item in items {
        let lines = render_transcript_item(item, &theme);
        for line in &lines {
            if y_offset >= height {
                break;
            }
            let line_area = Rect::new(0, y_offset, width, 1);
            Paragraph::new(line.clone()).render(line_area, &mut buffer);
            y_offset += 1;
        }
        // Add a blank line between items
        y_offset += 1;
    }

    buffer_to_string(&buffer)
}

/// Convert a ratatui Buffer to a printable string for snapshot testing.
#[cfg(test)]
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
            Span::styled(
                "darius",
                Style::default()
                    .fg(theme.brand)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("profile: {}", state.profile),
                Style::default().fg(theme.text),
            ),
            Span::styled(" │ ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("model: {}", state.model),
                Style::default().fg(theme.text),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title(" Header "));
        f.render_widget(header, chunks[0]);

        // Stream
        let messages: Vec<ListItem> = state
            .messages
            .iter()
            .map(|m| ListItem::new(Line::from(Span::styled(m, Style::default().fg(theme.text)))))
            .collect();
        let stream =
            List::new(messages).block(Block::default().borders(Borders::ALL).title(" Stream "));
        f.render_widget(stream, chunks[1]);

        // Tasks
        let tasks: Vec<ListItem> = state
            .tasks
            .iter()
            .map(|t| ListItem::new(Line::from(Span::styled(t, Style::default().fg(theme.add)))))
            .collect();
        let task_list =
            List::new(tasks).block(Block::default().borders(Borders::ALL).title(" Tasks "));
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
        let theme = Theme::for_mode(ColorMode::Truecolor);
        let state = fixture_state();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        render_welcome(area, &mut buffer, &state, &theme);
        insta::assert_snapshot!("welcome_card_80x24", buffer_to_string(&buffer));
    }

    #[test]
    fn welcome_card_120x36() {
        let theme = Theme::for_mode(ColorMode::Truecolor);
        let state = fixture_state();
        let area = Rect::new(0, 0, 120, 36);
        let mut buffer = Buffer::empty(area);
        render_welcome(area, &mut buffer, &state, &theme);
        insta::assert_snapshot!("welcome_card_120x36", buffer_to_string(&buffer));
    }

    /// Full transcript fixture: user, assistant, thinking, todos, tool (collapsed + expanded), diff.
    fn full_transcript_fixture() -> Vec<TranscriptItem> {
        vec![
            TranscriptItem::User {
                text: "Summarize the main.rs file".into(),
            },
            TranscriptItem::Assistant {
                text: "I'll read the main.rs file for you.".into(),
            },
            TranscriptItem::Thinking {
                text: "analyzing".into(),
                elapsed_ms: 1240,
            },
            TranscriptItem::Tasks {
                tasks: vec![
                    TaskDisplay {
                        title: "Read main.rs".into(),
                        status: TaskStatus::Done,
                    },
                    TaskDisplay {
                        title: "Analyze structure".into(),
                        status: TaskStatus::Active,
                    },
                    TaskDisplay {
                        title: "Write summary".into(),
                        status: TaskStatus::Todo,
                    },
                ],
            },
            TranscriptItem::Tool {
                tool: ToolView {
                    name: "read_file".into(),
                    args_preview: "src/main.rs".into(),
                    result: "fn main() {\n    println!(\"hello\");\n}".into(),
                    ok: true,
                },
                expanded: false,
            },
            TranscriptItem::Tool {
                tool: ToolView {
                    name: "read_file".into(),
                    args_preview: "src/main.rs".into(),
                    result: "fn main() {\n    println!(\"hello\");\n}".into(),
                    ok: true,
                },
                expanded: true,
            },
            TranscriptItem::Diff {
                diff: DiffView {
                    file: "src/main.rs".into(),
                    summary: "2 additions, 1 removal in 3 lines".into(),
                    lines: vec![
                        DiffLineView {
                            kind: DiffLineKind::Context,
                            old: Some(1),
                            new: Some(1),
                            text: "fn main() {".into(),
                        },
                        DiffLineView {
                            kind: DiffLineKind::Delete,
                            old: Some(2),
                            new: None,
                            text: "    println!(\"goodbye\");".into(),
                        },
                        DiffLineView {
                            kind: DiffLineKind::Add,
                            old: None,
                            new: Some(2),
                            text: "    println!(\"hello\");".into(),
                        },
                        DiffLineView {
                            kind: DiffLineKind::Context,
                            old: Some(3),
                            new: Some(3),
                            text: "}".into(),
                        },
                    ],
                },
            },
        ]
    }

    #[test]
    fn full_transcript_80x24() {
        let items = full_transcript_fixture();
        let rendered = render_transcript_to_string(80, 24, &items);
        insta::assert_snapshot!("full_transcript_80x24", rendered);
    }

    #[test]
    fn full_transcript_120x36() {
        let items = full_transcript_fixture();
        let rendered = render_transcript_to_string(120, 36, &items);
        insta::assert_snapshot!("full_transcript_120x36", rendered);
    }

    #[test]
    fn permission_chooser_80x24() {
        let theme = Theme::for_mode(ColorMode::Truecolor);
        let perm = PermissionState::new(
            "perm-1".into(),
            "Run shell command".into(),
            "ls -la ~/".into(),
            "List files in home directory".into(),
        );
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        render_permission(area, &mut buffer, &perm, &theme);
        insta::assert_snapshot!("permission_chooser_80x24", buffer_to_string(&buffer));
    }

    #[test]
    fn permission_chooser_selected_session() {
        let theme = Theme::for_mode(ColorMode::Truecolor);
        let mut perm = PermissionState::new(
            "perm-1".into(),
            "Run shell command".into(),
            "ls -la ~/".into(),
            "List files in home directory".into(),
        );
        perm.next(); // move to AllowSession
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        render_permission(area, &mut buffer, &perm, &theme);
        insta::assert_snapshot!(
            "permission_chooser_selected_session",
            buffer_to_string(&buffer)
        );
    }

    #[test]
    fn composer_80x6() {
        let theme = Theme::for_mode(ColorMode::Truecolor);
        let mut state = fixture_state();
        state.effort = crate::app::Effort::XHigh;
        state.mode = crate::app::Mode::Auto;
        state.composer.input = "hello world".into();
        let area = Rect::new(0, 0, 80, 6);
        let mut buffer = Buffer::empty(area);
        render_composer(area, &mut buffer, &state, &theme);
        insta::assert_snapshot!("composer_80x6", buffer_to_string(&buffer));
    }

    #[test]
    fn palette_80x10() {
        let theme = Theme::for_mode(ColorMode::Truecolor);
        let area = Rect::new(0, 0, 80, 10);
        let mut buffer = Buffer::empty(area);
        render_palette(area, &mut buffer, "/mo", 0, &theme);
        insta::assert_snapshot!("palette_80x10", buffer_to_string(&buffer));
    }

    #[test]
    fn palette_empty_query() {
        let theme = Theme::for_mode(ColorMode::Truecolor);
        let area = Rect::new(0, 0, 80, 15);
        let mut buffer = Buffer::empty(area);
        render_palette(area, &mut buffer, "", 0, &theme);
        insta::assert_snapshot!("palette_empty_query", buffer_to_string(&buffer));
    }
}
