use ratatui::style::Color;

/// Color mode detected from the terminal environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Truecolor,
    Ansi,
}

/// Semantic theme for the Darius TUI.
///
/// Every color has a truecolor (24-bit RGB) value and a fallback ANSI
/// value for terminals that do not advertise `COLORTERM=truecolor|24bit`.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub mode: ColorMode,
    pub brand: Color,
    pub text: Color,
    pub muted: Color,
    pub active: Color,
    pub rule: Color,
    pub auto_mode: Color,
    pub accept_edits: Color,
    pub plan_mode: Color,
    pub permission: Color,
    pub add: Color,
    pub delete: Color,
}

/// Environment reader for `Theme::detect`. Abstracted so tests can
/// inject fake env vars without touching the process environment.
pub trait Env {
    fn var(&self, key: &str) -> Option<String>;
}

/// Production environment reader wrapping `std::env::var_os`.
pub struct OsEnv;

impl Env for OsEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var_os(key).and_then(|v| v.into_string().ok())
    }
}

/// Test helper: a map-backed environment.
#[cfg(test)]
pub struct FakeEnv {
    pub vars: std::collections::HashMap<String, String>,
}

#[cfg(test)]
impl Env for FakeEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
}

impl Theme {
    /// Detect the color mode from the environment and build a theme.
    ///
    /// If `COLORTERM` is `truecolor` or `24bit`, full RGB is used.
    /// Otherwise the ANSI fallback palette is selected.
    pub fn detect(env: &impl Env) -> Self {
        let mode = match env.var("COLORTERM").as_deref() {
            Some("truecolor") | Some("24bit") => ColorMode::Truecolor,
            _ => ColorMode::Ansi,
        };
        Self::for_mode(mode)
    }

    /// Build a theme for a specific color mode.
    pub fn for_mode(mode: ColorMode) -> Self {
        match mode {
            ColorMode::Truecolor => Self::truecolor(),
            ColorMode::Ansi => Self::ansi(),
        }
    }

    /// Truecolor (24-bit RGB) palette.
    fn truecolor() -> Self {
        Self {
            mode: ColorMode::Truecolor,
            brand: Color::Rgb(0xe8, 0xa5, 0x4b),
            text: Color::Rgb(0xc0, 0xca, 0xf5),
            muted: Color::Rgb(0x94, 0x94, 0x94),
            active: Color::Rgb(0xaf, 0xd7, 0xff),
            rule: Color::Rgb(0x80, 0x80, 0x80),
            auto_mode: Color::Rgb(0xff, 0xd7, 0x00),
            accept_edits: Color::Rgb(0xaf, 0xaf, 0xd7),
            plan_mode: Color::Rgb(0x5f, 0xaf, 0xaf),
            permission: Color::Rgb(0xcd, 0x69, 0x4a),
            add: Color::Rgb(0x9e, 0xce, 0x6a),
            delete: Color::Rgb(0xf7, 0x76, 0x8e),
        }
    }

    /// ANSI fallback palette using named/indexed colors.
    fn ansi() -> Self {
        Self {
            mode: ColorMode::Ansi,
            brand: Color::Yellow,
            text: Color::White,
            muted: Color::Gray,
            active: Color::Cyan,
            rule: Color::DarkGray,
            auto_mode: Color::Yellow,
            accept_edits: Color::White,
            plan_mode: Color::Cyan,
            permission: Color::Red,
            add: Color::Green,
            delete: Color::Red,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_truecolor_from_truecolor() {
        let env = FakeEnv {
            vars: [("COLORTERM".to_string(), "truecolor".to_string())]
                .into_iter()
                .collect(),
        };
        let theme = Theme::detect(&env);
        assert_eq!(theme.mode, ColorMode::Truecolor);
    }

    #[test]
    fn detect_truecolor_from_24bit() {
        let env = FakeEnv {
            vars: [("COLORTERM".to_string(), "24bit".to_string())]
                .into_iter()
                .collect(),
        };
        let theme = Theme::detect(&env);
        assert_eq!(theme.mode, ColorMode::Truecolor);
    }

    #[test]
    fn detect_ansi_when_colorterm_missing() {
        let env = FakeEnv {
            vars: std::collections::HashMap::new(),
        };
        let theme = Theme::detect(&env);
        assert_eq!(theme.mode, ColorMode::Ansi);
    }

    #[test]
    fn detect_ansi_when_colorterm_unsupported() {
        let env = FakeEnv {
            vars: [("COLORTERM".to_string(), "8bit".to_string())]
                .into_iter()
                .collect(),
        };
        let theme = Theme::detect(&env);
        assert_eq!(theme.mode, ColorMode::Ansi);
    }

    #[test]
    fn truecolor_maps_exact_hex() {
        let theme = Theme::for_mode(ColorMode::Truecolor);
        assert_eq!(theme.brand, Color::Rgb(0xe8, 0xa5, 0x4b));
        assert_eq!(theme.text, Color::Rgb(0xc0, 0xca, 0xf5));
        assert_eq!(theme.muted, Color::Rgb(0x94, 0x94, 0x94));
        assert_eq!(theme.active, Color::Rgb(0xaf, 0xd7, 0xff));
        assert_eq!(theme.rule, Color::Rgb(0x80, 0x80, 0x80));
        assert_eq!(theme.auto_mode, Color::Rgb(0xff, 0xd7, 0x00));
        assert_eq!(theme.accept_edits, Color::Rgb(0xaf, 0xaf, 0xd7));
        assert_eq!(theme.plan_mode, Color::Rgb(0x5f, 0xaf, 0xaf));
        assert_eq!(theme.permission, Color::Rgb(0xcd, 0x69, 0x4a));
        assert_eq!(theme.add, Color::Rgb(0x9e, 0xce, 0x6a));
        assert_eq!(theme.delete, Color::Rgb(0xf7, 0x76, 0x8e));
    }

    #[test]
    fn fallback_maps_to_named_ansi_colors() {
        let theme = Theme::for_mode(ColorMode::Ansi);
        assert_eq!(theme.brand, Color::Yellow);
        assert_eq!(theme.text, Color::White);
        assert_eq!(theme.muted, Color::Gray);
        assert_eq!(theme.active, Color::Cyan);
        assert_eq!(theme.rule, Color::DarkGray);
        assert_eq!(theme.auto_mode, Color::Yellow);
        assert_eq!(theme.accept_edits, Color::White);
        assert_eq!(theme.plan_mode, Color::Cyan);
        assert_eq!(theme.permission, Color::Red);
        assert_eq!(theme.add, Color::Green);
        assert_eq!(theme.delete, Color::Red);
    }
}