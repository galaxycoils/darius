use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use darius_cognitive::UiEvent;
use darius_tui::{
    AppState, Mode,
    app::{Action, Effect, PermissionChoice},
    input::map_key,
};

#[test]
fn mode_composer_hides_effort() {
    let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 100, 8));
    darius_tui::render::render_composer(
        buffer.area,
        &mut buffer,
        &AppState::default(),
        &darius_tui::theme::Theme::for_mode(darius_tui::theme::ColorMode::Ansi),
    );
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(!text.contains("effort"), "{text}");
    assert!(!text.contains("high"), "{text}");
    assert!(text.to_lowercase().contains("auto"), "{text}");
}

fn chooser() -> AppState {
    let mut state = AppState {
        running: true,
        ..Default::default()
    };
    state.apply_event(UiEvent::PermissionRequired {
        id: "p1".into(),
        title: "write".into(),
        command: "write".into(),
        reason: "mutation".into(),
    });
    state
}

#[test]
fn permission_escape_resolves_deny_instead_of_dropping_prompt() {
    let mut state = chooser();
    let action = map_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &state).unwrap();
    assert_eq!(
        state.reduce(action),
        Some(Effect::ResolvePermission {
            id: "p1".into(),
            choice: PermissionChoice::Deny
        })
    );
    assert!(state.permission.is_none());
    assert!(state.permission_queue.is_empty());
}

#[test]
fn permission_escape_ctrl_c_interrupts_active_chooser() {
    let mut state = chooser();
    let action = map_key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &state,
    );
    assert_eq!(action, Some(Action::Interrupt));
    assert_eq!(state.reduce(action.unwrap()), Some(Effect::Interrupt));
    assert!(state.permission.is_none());
}

#[test]
fn permission_escape_quit_clears_chooser_and_requests_shutdown() {
    let mut state = chooser();
    assert_eq!(state.reduce(Action::Quit), Some(Effect::Quit));
    assert!(state.permission.is_none());
}

#[test]
fn mode_backtab_requests_toggle_and_waits_for_acknowledgement() {
    let mut state = AppState::default();
    for expected in [Mode::Plan, Mode::Auto, Mode::Plan] {
        let previous = state.mode;
        let action = map_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT), &state).unwrap();
        let effect = state.reduce(action);
        assert_eq!(
            state.mode, previous,
            "must not claim a mode before runtime acknowledgement"
        );
        assert_eq!(
            effect,
            Some(Effect::ExecuteCommand(
                darius_tui::commands::parse_invocation("/mode").unwrap()
            ))
        );
        state.apply_event(UiEvent::ModeChanged { mode: expected });
        assert_eq!(state.mode, expected);
    }
}

#[test]
fn mode_rejects_legacy_choices_and_effort_is_hidden() {
    for value in ["manual", "accept-edits", "unknown", "auto extra"] {
        assert!(
            darius_tui::commands::parse_invocation(&format!("/mode {value}")).is_err(),
            "{value}"
        );
    }
    assert!(darius_tui::commands::parse_invocation("/effort high").is_err());
    for cmd in darius_tui::commands::COMMANDS {
        assert!(!cmd.description.contains("manual"));
        assert_ne!(cmd.name, "/effort");
    }
}
