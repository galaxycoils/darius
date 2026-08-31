use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

use crate::app::{Action, AppState};
use crate::controller::{RuntimeCommand, TuiController};
use crate::input::map_key;
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

/// Map an `Effect` to the `RuntimeCommand` that the worker understands.
fn effect_to_command(state: &AppState, effect: crate::app::Effect) -> Option<RuntimeCommand> {
    match effect {
        crate::app::Effect::SubmitGoal(text) => Some(RuntimeCommand::SubmitGoal {
            text,
            mode: state.mode,
            effort: state.effort,
        }),
        crate::app::Effect::ExecuteCommand(inv) => Some(RuntimeCommand::ExecuteSlash(inv)),
        crate::app::Effect::Interrupt => Some(RuntimeCommand::Interrupt),
        crate::app::Effect::ResolvePermission { id, choice } => {
            Some(RuntimeCommand::ResolvePermission { id, choice })
        }
        crate::app::Effect::Quit => Some(RuntimeCommand::Shutdown),
    }
}

/// True when the command means "exit the TUI now".
fn cmd_is_quit(cmd: &RuntimeCommand) -> bool {
    matches!(cmd, RuntimeCommand::Shutdown)
}

/// Run the TUI event loop.
///
/// 1. Draw.
/// 2. Drain `controller.events.try_recv()` → `state.apply_event`.
/// 3. Poll crossterm with a short timeout (never block indefinitely).
/// 4. Ignore key-release events.
/// 5. Route press through `map_key` → `reduce` → `RuntimeCommand`.
/// 6. Break only on Quit, controller closure, or fatal terminal error.
pub fn run_tui(mut state: AppState, mut controller: TuiController) -> io::Result<()> {
    let _guard = TerminalGuard::enter()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let result: io::Result<()> = loop {
        // 1. Draw.
        if let Err(e) = draw(&mut terminal, &state) {
            break Err(e);
        }

        // 2. Drain events (non-blocking).
        while let Ok(event) = controller.events.try_recv() {
            state.apply_event(event);
        }

        // 3. Poll crossterm — do not block the redraw indefinitely.
        match crossterm::event::poll(Duration::from_millis(25)) {
            Ok(true) => {
                match crossterm::event::read() {
                    Ok(crossterm::event::Event::Key(key)) => {
                        // 4. Ignore key-release events.
                        if matches!(key.kind, crossterm::event::KeyEventKind::Release) {
                            continue;
                        }
                        // 5. Map and reduce.
                        if let Some(action) = map_key(key, &state) {
                            if let Some(effect) = state.reduce(action) {
                                if let Some(cmd) = effect_to_command(&state, effect) {
                                    // 6. Send command; break on closure.
                                    if cmd_is_quit(&cmd) {
                                        break Ok(());
                                    }
                                    match controller.commands.send(cmd) {
                                        Ok(()) => {}
                                        Err(_) => {
                                            // Worker dropped — exit gracefully.
                                            break Ok(());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(_) => {
                        // Non-key events (resize, paste) — redraw will pick them up.
                    }
                    Err(e) => break Err(e),
                }
            }
            Ok(false) => {
                // Timeout — loop back to draw + drain events.
            }
            Err(e) => break Err(e),
        }
    };

    terminal.clear()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ── Test backend (records operation counts) ─────────────────────────

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
        assert_eq!(ops.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn guard_drop_restores_in_correct_order() {
        let ops = Arc::new(AtomicUsize::new(0));
        let backend = Box::new(TestBackend::new(ops.clone()));
        {
            let _guard = TerminalGuard::with_backend(backend).unwrap();
            assert_eq!(ops.load(Ordering::SeqCst), 3);
        }
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
        assert_eq!(ops.load(Ordering::SeqCst), 6);
    }

    // ── Event-source harness (proves the loop logic without a PTY) ─────

    use crate::app::{Effort, Mode};
    use crate::commands::{CommandId, CommandInvocation};
    use darius_cognitive::UiEvent;

    /// Build a `KeyEvent` (crossterm 0.27 API: `new(code, modifiers)` is the
    /// canonical constructor; `kind` defaults to `Press`).
    fn key_event(code: crossterm::event::KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    /// Build a `KeyEvent` with the given modifiers.
    fn key_event_mod(
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, modifiers)
    }

    /// Simulate the loop body: drain events, then process each key through
    /// `map_key → reduce → RuntimeCommand`. Returns the final state and the
    /// collected commands.
    fn run_loop_with(
        events: Vec<UiEvent>,
        keys: Vec<crossterm::event::KeyEvent>,
    ) -> (AppState, Vec<RuntimeCommand>) {
        let (mut controller, mut cmd_rx, event_tx) = TuiController::new(64);

        // Replay the canned events.
        for ev in events {
            let _ = event_tx.send(ev);
        }

        let mut local_state = AppState::default();

        // Drain all available events (mirrors the loop's drain step).
        loop {
            match controller.events.try_recv() {
                Ok(ev) => local_state.apply_event(ev),
                Err(_) => break,
            }
        }

        let mut collected = Vec::new();
        for key in &keys {
            if matches!(key.kind, crossterm::event::KeyEventKind::Release) {
                continue;
            }
            if let Some(action) = map_key(*key, &local_state) {
                if let Some(effect) = local_state.reduce(action) {
                    if let Some(cmd) = effect_to_command(&local_state, effect) {
                        collected.push(cmd.clone());
                        if controller.commands.send(cmd).is_err() {
                            break;
                        }
                    }
                }
            }
        }

        // Drain whatever landed in the channel.
        while let Ok(cmd) = cmd_rx.try_recv() {
            collected.push(cmd);
        }

        (local_state, collected)
    }

    #[test]
    fn ordinary_text_submits_goal() {
        let (_state, cmds) = run_loop_with(
            vec![],
            vec![
                key_event(crossterm::event::KeyCode::Char('h')),
                key_event(crossterm::event::KeyCode::Char('i')),
                key_event(crossterm::event::KeyCode::Enter),
            ],
        );

        let found = cmds
            .iter()
            .any(|c| matches!(c, RuntimeCommand::SubmitGoal { text, .. } if text == "hi"));
        assert!(found, "expected SubmitGoal for 'hi', got {cmds:?}");
    }

    #[test]
    fn slash_command_produces_execute_slash() {
        // Type `/quit`, close the palette (Esc), then Enter to submit.
        let (_state, cmds) = run_loop_with(
            vec![],
            vec![
                key_event(crossterm::event::KeyCode::Char('/')),
                key_event(crossterm::event::KeyCode::Char('q')),
                key_event(crossterm::event::KeyCode::Char('u')),
                key_event(crossterm::event::KeyCode::Char('i')),
                key_event(crossterm::event::KeyCode::Char('t')),
                key_event(crossterm::event::KeyCode::Esc),
                key_event(crossterm::event::KeyCode::Enter),
            ],
        );

        let found = cmds
            .iter()
            .any(|c| matches!(c, RuntimeCommand::ExecuteSlash(inv) if inv.id == CommandId::Quit));
        assert!(found, "expected ExecuteSlash(/quit), got {cmds:?}");
    }

    #[test]
    fn events_are_applied_to_state() {
        let (state, _cmds) = run_loop_with(
            vec![
                UiEvent::Header {
                    profile: "work".into(),
                    model: "mock".into(),
                    goal: "test".into(),
                },
                UiEvent::Done,
            ],
            vec![],
        );
        assert_eq!(state.profile, "work");
        assert!(matches!(state.goal.as_deref(), Some("test")));
        assert_eq!(state.status_line.as_deref(), Some("Done"));
    }

    #[test]
    fn key_release_is_ignored() {
        let (_state, cmds) = run_loop_with(
            vec![],
            vec![
                // Release event for 'h' — must be ignored.
                crossterm::event::KeyEvent {
                    kind: crossterm::event::KeyEventKind::Release,
                    code: crossterm::event::KeyCode::Char('h'),
                    modifiers: crossterm::event::KeyModifiers::NONE,
                    state: crossterm::event::KeyEventState::empty(),
                },
                // Press event for 'h'.
                crossterm::event::KeyEvent {
                    kind: crossterm::event::KeyEventKind::Press,
                    code: crossterm::event::KeyCode::Char('h'),
                    modifiers: crossterm::event::KeyModifiers::NONE,
                    state: crossterm::event::KeyEventState::empty(),
                },
                key_event(crossterm::event::KeyCode::Enter),
            ],
        );

        let found = cmds
            .iter()
            .any(|c| matches!(c, RuntimeCommand::SubmitGoal { text, .. } if text == "h"));
        assert!(
            found,
            "expected SubmitGoal for 'h' (release ignored), got {cmds:?}"
        );
    }

    #[test]
    fn ctrl_c_while_idle_sends_shutdown() {
        let (_state, cmds) = run_loop_with(
            vec![],
            vec![key_event_mod(
                crossterm::event::KeyCode::Char('c'),
                crossterm::event::KeyModifiers::CONTROL,
            )],
        );

        assert!(
            cmds.iter().any(|c| matches!(c, RuntimeCommand::Shutdown)),
            "expected Shutdown from Ctrl+C idle, got {cmds:?}"
        );
    }

    #[test]
    fn permission_choose_sends_resolve() {
        let mut state = AppState::default();
        state.permission = Some(crate::app::PermissionState::new(
            "perm-1".into(),
            "Write file".into(),
            "fs::write".into(),
            "Write to disk".into(),
        ));
        // Select Deny (index 2).
        state.permission.as_mut().unwrap().next();
        state.permission.as_mut().unwrap().next();

        let key = key_event(crossterm::event::KeyCode::Enter);
        let action = map_key(key, &state);
        assert_eq!(action, Some(Action::PermissionChoose));

        let effect = state.reduce(action.unwrap());
        assert!(matches!(
            effect,
            Some(crate::app::Effect::ResolvePermission { .. })
        ));

        let cmd = effect_to_command(&state, effect.unwrap());
        assert!(
            matches!(cmd, Some(RuntimeCommand::ResolvePermission { ref id, choice: crate::app::PermissionChoice::Deny }) if id == "perm-1"),
            "expected ResolvePermission deny, got {cmd:?}"
        );
    }

    #[test]
    fn effect_to_command_maps_all_variants() {
        let state = AppState::default();

        let cases: Vec<(crate::app::Effect, RuntimeCommand)> = vec![
            (
                crate::app::Effect::SubmitGoal("x".into()),
                RuntimeCommand::SubmitGoal {
                    text: "x".into(),
                    mode: Mode::Auto,
                    effort: Effort::High,
                },
            ),
            (
                crate::app::Effect::ExecuteCommand(CommandInvocation {
                    id: CommandId::Help,
                    name: "/help".into(),
                    args: String::new(),
                }),
                RuntimeCommand::ExecuteSlash(CommandInvocation {
                    id: CommandId::Help,
                    name: "/help".into(),
                    args: String::new(),
                }),
            ),
            (
                crate::app::Effect::Interrupt,
                RuntimeCommand::Interrupt,
            ),
            (
                crate::app::Effect::ResolvePermission {
                    id: "p".into(),
                    choice: crate::app::PermissionChoice::AllowOnce,
                },
                RuntimeCommand::ResolvePermission {
                    id: "p".into(),
                    choice: crate::app::PermissionChoice::AllowOnce,
                },
            ),
            (crate::app::Effect::Quit, RuntimeCommand::Shutdown),
        ];

        for (effect, expected) in cases {
            let got = effect_to_command(&state, effect).expect("Some command");
            match (&got, &expected) {
                (
                    RuntimeCommand::SubmitGoal { text: t1, .. },
                    RuntimeCommand::SubmitGoal { text: t2, .. },
                ) => assert_eq!(t1, t2),
                (RuntimeCommand::ExecuteSlash(a), RuntimeCommand::ExecuteSlash(b)) => {
                    assert_eq!(a.name, b.name)
                }
                (RuntimeCommand::Interrupt, RuntimeCommand::Interrupt) => {}
                (
                    RuntimeCommand::ResolvePermission { id: i1, .. },
                    RuntimeCommand::ResolvePermission { id: i2, .. },
                ) => assert_eq!(i1, i2),
                (RuntimeCommand::Shutdown, RuntimeCommand::Shutdown) => {}
                other => panic!("mismatch: {other:?}"),
            }
        }
    }
}