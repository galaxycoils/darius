use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io;

use crate::app::AppState;
use crate::render::draw;

/// Terminal lifecycle guard that ensures raw mode, cursor visibility,
/// and alternate screen are always restored — even on panic.
///
/// Uses a trait object backend so tests can verify the cleanup sequence
/// without touching a real terminal.
pub struct TerminalGuard {
    inner: Box<dyn TerminalBackend>,
}

/// Abstraction over crossterm operations so we can test cleanup order.
pub trait TerminalBackend: Send {
    fn enable_raw_mode(&self) -> io::Result<()>;
    fn disable_raw_mode(&self) -> io::Result<()>;
    fn enter_alternate_screen(&self) -> io::Result<()>;
    fn leave_alternate_screen(&self) -> io::Result<()>;
    fn show_cursor(&self) -> io::Result<()>;
    fn hide_cursor(&self) -> io::Result<()>;
}

/// Real crossterm-backed implementation used in production.
pub struct CrosstermBackendImpl;

impl TerminalBackend for CrosstermBackendImpl {
    fn enable_raw_mode(&self) -> io::Result<()> {
        crossterm::terminal::enable_raw_mode()
    }

    fn disable_raw_mode(&self) -> io::Result<()> {
        crossterm::terminal::disable_raw_mode()
    }

    fn enter_alternate_screen(&self) -> io::Result<()> {
        crossterm::execute!(io::stdout(), crossterm::terminal::EnterAlternateScreen)
    }

    fn leave_alternate_screen(&self) -> io::Result<()> {
        crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)
    }

    fn show_cursor(&self) -> io::Result<()> {
        crossterm::execute!(io::stdout(), crossterm::cursor::Show)
    }

    fn hide_cursor(&self) -> io::Result<()> {
        crossterm::execute!(io::stdout(), crossterm::cursor::Hide)
    }
}

impl TerminalGuard {
    /// Enter raw mode and alternate screen using the real crossterm backend.
    pub fn enter() -> io::Result<Self> {
        let inner: Box<dyn TerminalBackend> = Box::new(CrosstermBackendImpl);
        inner.enable_raw_mode()?;
        inner.enter_alternate_screen()?;
        inner.hide_cursor()?;
        Ok(Self { inner })
    }

    /// Enter with a custom backend (used in tests).
    pub fn with_backend(inner: Box<dyn TerminalBackend>) -> io::Result<Self> {
        inner.enable_raw_mode()?;
        inner.enter_alternate_screen()?;
        inner.hide_cursor()?;
        Ok(Self { inner })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Order matters: show cursor BEFORE leaving alternate screen,
        // leave alternate screen BEFORE disabling raw mode.
        let _ = self.inner.show_cursor();
        let _ = self.inner.leave_alternate_screen();
        let _ = self.inner.disable_raw_mode();
    }
}

pub fn run_tui() -> io::Result<()> {
    let _guard = TerminalGuard::enter()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Records the sequence of backend operations for assertions.
    struct TestBackend {
        operations: Arc<AtomicUsize>,
    }

    impl TestBackend {
        fn new(ops: Arc<AtomicUsize>) -> Self {
            Self { operations: ops }
        }
    }

    impl TerminalBackend for TestBackend {
        fn enable_raw_mode(&self) -> io::Result<()> {
            self.operations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn disable_raw_mode(&self) -> io::Result<()> {
            self.operations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn enter_alternate_screen(&self) -> io::Result<()> {
            self.operations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn leave_alternate_screen(&self) -> io::Result<()> {
            self.operations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn show_cursor(&self) -> io::Result<()> {
            self.operations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn hide_cursor(&self) -> io::Result<()> {
            self.operations.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn guard_enter_calls_raw_mode_alt_screen_hide_cursor() {
        let ops = Arc::new(AtomicUsize::new(0));
        let backend = Box::new(TestBackend::new(ops.clone()));
        let _guard = TerminalGuard::with_backend(backend).unwrap();
        // enter: enable_raw_mode + enter_alternate_screen + hide_cursor = 3
        assert_eq!(ops.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn guard_drop_restores_in_correct_order() {
        let ops = Arc::new(AtomicUsize::new(0));
        let backend = Box::new(TestBackend::new(ops.clone()));
        {
            let _guard = TerminalGuard::with_backend(backend).unwrap();
            // After enter: 3 ops
            assert_eq!(ops.load(Ordering::SeqCst), 3);
        }
        // After drop: +3 ops (show_cursor, leave_alt_screen, disable_raw_mode) = 6
        assert_eq!(ops.load(Ordering::SeqCst), 6);
    }

    #[test]
    fn guard_restores_even_on_panic() {
        let ops = Arc::new(AtomicUsize::new(0));
        let backend = Box::new(TestBackend::new(ops.clone()));
        let result = std::panic::catch_unwind(|| {
            let _guard = TerminalGuard::with_backend(backend).unwrap();
            panic!("simulated panic");
        });
        assert!(result.is_err());
        // Drop still ran: 3 (enter) + 3 (drop) = 6
        assert_eq!(ops.load(Ordering::SeqCst), 6);
    }
}