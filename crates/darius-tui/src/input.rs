use crate::app::{Action, AppState, Mode};
use crate::cognitive::UiEvent;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Map a keyboard event to a TUI action based on current state.
pub fn map_key(key: KeyEvent, state: &AppState) -> Option<Action> {
    // Permission chooser takes priority when active
    if state.permission_queue.first().is_some() {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(Action::PermissionNext),
            KeyCode::Down | KeyCode::Char('j') => Some(Action::PermissionPrev),
            KeyCode::Enter => Some(Action::PermissionChoose),
            KeyCode::Esc => Some(Action::Cancel),
            _ => None,
        };
    }

    // Slash palette mode
    if state.composer.slash_mode {
        return match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::PaletteAccept),
            KeyCode::Up => Some(Action::PalettePrev),
            KeyCode::Down => Some(Action::PaletteNext),
            KeyCode::Tab => Some(Action::PaletteAccept),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char(c) => Some(Action::Insert(c)),
            _ => None,
        };
    }

    // Normal mode
    match key.code {
        KeyCode::Char('/') => Some(Action::OpenPalette),
        KeyCode::Char('q') if key.modifiers.is_empty() => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Char('k') | KeyCode::PageUp => Some(Action::Scroll(-1)),
        KeyCode::Char('j') | KeyCode::PageDown => Some(Action::Scroll(1)),
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => Some(Action::CycleMode),
        KeyCode::Enter => Some(Action::Submit),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Char(c) => Some(Action::Insert(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_opens_palette() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        assert_eq!(map_key(key, &state), Some(Action::OpenPalette));
    }

    #[test]
    fn q_quits() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(map_key(key, &state), Some(Action::Quit));
    }

    #[test]
    fn ctrl_c_quits() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key(key, &state), Some(Action::Quit));
    }

    #[test]
    fn esc_cancels() {
        let state = AppState::default();
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(map_key(key, &state), Some(Action::Cancel));
    }

    #[test]
    fn scroll_keys() {
        let state = AppState::default();
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE), &state),
            Some(Action::Scroll(-1))
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), &state),
            Some(Action::Scroll(1))
        );
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
        state.composer.slash_mode = true;
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
        state.push_permission("p1".into(), "test".into());
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
}
