use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io;

use crate::app::AppState;
use crate::render::draw;

pub fn run_tui() -> io::Result<()> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut state = AppState::default();

    terminal.clear()?;
    loop {
        draw(&mut terminal, &state)?;

        if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
            match key.code {
                crossterm::event::KeyCode::Char('/') => {
                    state.composer.slash_mode = true;
                }
                crossterm::event::KeyCode::Char(c) if state.composer.slash_mode => {
                    state.composer.input.push(c);
                }
                crossterm::event::KeyCode::Backspace if state.composer.slash_mode => {
                    state.composer.input.pop();
                }
                crossterm::event::KeyCode::Enter if state.composer.slash_mode => {
                    let cmd = state.composer.input.clone();
                    state.composer.input.clear();
                    state.composer.slash_mode = false;
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
                    state.composer.slash_mode = false;
                    state.composer.input.clear();
                }
                crossterm::event::KeyCode::Char('q') if !state.composer.slash_mode => break,
                _ => {}
            }
        }
    }

    terminal.clear()?;
    Ok(())
}
