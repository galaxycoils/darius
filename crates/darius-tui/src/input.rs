use crate::app::{Action, AppState, PermissionChoice};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Map a keyboard event to a TUI action based on current state.
pub fn map_key(key: KeyEvent, state: &AppState) -> Option<Action> {
    // Permission chooser takes priority when active
    if state.permission.is_some() {
        return match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Interrupt)
            }
            KeyCode::Char('1') | KeyCode::Char('y') => {
                Some(Action::PermissionDirect(PermissionChoice::AllowOnce))
            }
            KeyCode::Char('2') | KeyCode::Char('a') => {
                Some(Action::PermissionDirect(PermissionChoice::AllowSession))
            }
            KeyCode::Char('3') | KeyCode::Char('n') => {
                Some(Action::PermissionDirect(PermissionChoice::Deny))
            }
            KeyCode::Up | KeyCode::Char('k') => Some(Action::PermissionNext),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::PermissionPrev),
            KeyCode::Enter => Some(Action::PermissionChoose),
            KeyCode::Esc => Some(Action::Cancel),
            _ => None,
        };
    }

    // Model picker takes priority when open
    if state.model_picker.is_some() {
        return match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::ModelPickerSelect),
            KeyCode::Up | KeyCode::Char('k') => Some(Action::ModelPickerPrev),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::ModelPickerNext),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char(c) => Some(Action::Insert(c)),
            _ => None,
        };
    }

    // Palette mode takes priority when open
    if state.palette.open {
        return match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::PaletteAccept),
            KeyCode::Tab => Some(Action::PaletteComplete),
            KeyCode::Up => Some(Action::PalettePrev),
            KeyCode::Down => Some(Action::PaletteNext),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Delete => Some(Action::Delete),
            KeyCode::Left => Some(Action::MoveCursorLeft),
            KeyCode::Right => Some(Action::MoveCursorRight),
            KeyCode::Home => Some(Action::CursorHome),
            KeyCode::End => Some(Action::CursorEnd),
            KeyCode::Char(c) => Some(Action::Insert(c)),
            _ => None,
        };
    }

    // Normal mode
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(if state.running {
                Action::Interrupt
            } else {
                Action::Quit
            })
        }
        // Shift+Tab / BackTab is mode toggle
        KeyCode::BackTab => Some(Action::CycleMode),
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => Some(Action::CycleMode),
        KeyCode::PageUp => Some(Action::Scroll(-1)),
        KeyCode::PageDown => Some(Action::Scroll(1)),
        KeyCode::Enter => Some(Action::Submit),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Delete => Some(Action::Delete),
        KeyCode::Left => Some(Action::MoveCursorLeft),
        KeyCode::Right => Some(Action::MoveCursorRight),
        KeyCode::Home => Some(Action::CursorHome),
        KeyCode::End => Some(Action::CursorEnd),
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Char(c) => Some(Action::Insert(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::PermissionState;

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
    fn slash_opens_palette() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        // / is now typed as Insert, palette opens via reducer after Insert
        assert_eq!(map_key(key, &state), Some(Action::Insert('/')));
    }

    #[test]
    fn ctrl_c_quits_when_idle() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        // Not running, so Ctrl+C = Quit
        assert_eq!(map_key(key, &state), Some(Action::Quit));
    }

    #[test]
    fn ctrl_c_interrupts_when_running() {
        let mut state = AppState::default();
        state.running = true;
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key(key, &state), Some(Action::Interrupt));
    }

    #[test]
    fn esc_cancels() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(map_key(key, &state), Some(Action::Cancel));
    }

    #[test]
    fn page_keys_scroll() {
        let state = AppState::default();
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE), &state),
            Some(Action::Scroll(-1))
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE), &state),
            Some(Action::Scroll(1))
        );
    }

    #[test]
    fn shift_tab_cycles_mode() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        assert_eq!(map_key(key, &state), Some(Action::CycleMode));
    }

    #[test]
    fn palette_navigation() {
        let mut state = AppState::default();
        state.palette.open = true;
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &state),
            Some(Action::PalettePrev)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &state),
            Some(Action::PaletteNext)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &state),
            Some(Action::PaletteAccept)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &state),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn permission_chooser_priority() {
        let mut state = AppState::default();
        // Set active permission directly (new API)
        state.permission = Some(PermissionState::new(
            "p1".into(),
            "Write file".into(),
            "fs::write".into(),
            "Write to disk".into(),
        ));
        // When permission is active, arrow keys navigate permission, not scroll
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &state),
            Some(Action::PermissionNext)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &state),
            Some(Action::PermissionChoose)
        );
    }

    #[test]
    fn permission_chooser_direct_shortcuts() {
        let mut state = AppState::default();
        state.permission = Some(PermissionState::new(
            "p1".into(),
            "Write file".into(),
            "fs::write".into(),
            "Write to disk".into(),
        ));
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
                &state
            ),
            Some(Action::PermissionDirect(PermissionChoice::AllowOnce))
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
                &state
            ),
            Some(Action::PermissionDirect(PermissionChoice::AllowSession))
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
                &state
            ),
            Some(Action::PermissionDirect(PermissionChoice::Deny))
        );
    }

    #[test]
    fn model_picker_input_routing() {
        let mut state = AppState::default();
        state.model_picker = Some(crate::app::ModelPickerState::new());
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &state),
            Some(Action::ModelPickerPrev)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &state),
            Some(Action::ModelPickerNext)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &state),
            Some(Action::ModelPickerSelect)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &state),
            Some(Action::Cancel)
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
                &state
            ),
            Some(Action::Insert('g'))
        );
    }
}
