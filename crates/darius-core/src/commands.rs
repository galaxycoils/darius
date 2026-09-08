//! Canonical registry of supported slash commands.
pub use crate::slash::{CommandId, CommandInvocation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: CommandId,
    pub name: &'static str,
    pub description: &'static str,
    pub accepts_args: bool,
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: CommandId::Help,
        name: "/help",
        description: "Show commands and keyboard shortcuts",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Clear,
        name: "/clear",
        description: "Clear the visible transcript",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Compact,
        name: "/compact",
        description: "Compact session context into memory",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Model,
        name: "/model",
        description: "Show or select provider/model",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::Mode,
        name: "/mode",
        description: "Cycle or select auto/plan",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::Permissions,
        name: "/permissions",
        description: "Show the current permission policy",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Memory,
        name: "/memory",
        description: "Search durable memory",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::Pack,
        name: "/pack",
        description: "Show the bounded MemoryPack",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Tasks,
        name: "/tasks",
        description: "Show the current task board",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Status,
        name: "/status",
        description: "Show profile/model/context/kernel status",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Config,
        name: "/config",
        description: "Show effective profile configuration",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Stop,
        name: "/stop",
        description: "Interrupt the active turn",
        accepts_args: false,
    },
    CommandSpec {
        id: CommandId::Quit,
        name: "/quit",
        description: "Exit Darius",
        accepts_args: false,
    },
];

/// Check if query matches target via fuzzy subsequence match (case-insensitive).
pub fn fuzzy_match(query: &str, target: &str) -> bool {
    let q_clean = query
        .trim_start_matches('/')
        .trim_start_matches('-')
        .to_lowercase();
    if q_clean.is_empty() {
        return true;
    }
    let t_clean = target.trim_start_matches('/').to_lowercase();

    if t_clean.contains(&q_clean) {
        return true;
    }

    let mut target_chars = t_clean.chars();
    for qc in q_clean.chars() {
        if !target_chars.any(|tc| tc == qc) {
            return false;
        }
    }

    true
}

/// Filter commands by query (fuzzy match across name, or description if not slash-prefixed).
pub fn filter(query: &str) -> Vec<&'static CommandSpec> {
    let q = query.trim();
    if q.is_empty() || q == "/" || q == "-" {
        return COMMANDS.iter().collect();
    }

    let is_slash = q.starts_with('/') || q.starts_with('-');
    let q_clean = q
        .trim_start_matches('/')
        .trim_start_matches('-')
        .to_lowercase();

    COMMANDS
        .iter()
        .filter(|cmd| {
            let name_clean = cmd.name.trim_start_matches('/');
            if is_slash {
                fuzzy_match(&q_clean, name_clean)
            } else {
                fuzzy_match(&q_clean, name_clean) || fuzzy_match(&q_clean, cmd.description)
            }
        })
        .collect()
}

/// Parse input into a command, supporting both `/command` and `-command` aliases.
#[allow(clippy::question_mark)]
pub fn parse(input: &str) -> Option<&'static CommandSpec> {
    let trimmed = input.trim();

    let rest = if let Some(s) = trimmed.strip_prefix('/') {
        s
    } else if let Some(s) = trimmed.strip_prefix('-') {
        s
    } else {
        return None;
    };

    let cmd_name = rest.split_whitespace().next().unwrap_or("");
    if cmd_name.is_empty() {
        return None;
    }

    COMMANDS.iter().find(|c| c.name.get(1..) == Some(cmd_name))
}

/// Convert dash alias to slash form.
pub fn dash_alias_to_slash(input: &str) -> String {
    if let Some(rest) = input.strip_prefix('-') {
        format!("/{}", rest)
    } else {
        input.to_string()
    }
}

/// Parse a full command invocation with arguments.
pub fn parse_invocation(input: &str) -> Result<CommandInvocation, String> {
    let canonical = dash_alias_to_slash(input.trim());
    let mut parts = canonical.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default();
    let args = parts.next().unwrap_or_default().trim().to_string();
    let spec = COMMANDS
        .iter()
        .find(|item| item.name == name)
        .ok_or_else(|| format!("unknown command: {name}"))?;
    if !spec.accepts_args && !args.is_empty() {
        return Err(format!("{} does not accept arguments", spec.name));
    }
    if spec.id == CommandId::Mode && !matches!(args.as_str(), "" | "auto" | "plan") {
        return Err("/mode accepts only auto or plan".into());
    }
    Ok(CommandInvocation {
        id: spec.id,
        name: spec.name.into(),
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_registry_exact_surface() {
        assert_eq!(COMMANDS.len(), 13);
        let names: Vec<_> = COMMANDS.iter().map(|c| c.name).collect();
        for expected in [
            "/help",
            "/clear",
            "/compact",
            "/model",
            "/mode",
            "/permissions",
            "/memory",
            "/pack",
            "/tasks",
            "/status",
            "/config",
            "/stop",
            "/quit",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
        for forbidden in ["/effort", "/skills", "/a2a", "/serve", "/plan"] {
            assert!(!names.contains(&forbidden), "must not contain {forbidden}");
        }
    }

    #[test]
    fn filter_by_prefix() {
        let results = filter("/mo");
        assert!(results.iter().any(|c| c.name == "/model"));
        assert!(results.iter().any(|c| c.name == "/mode"));
    }

    #[test]
    fn parse_slash_command() {
        assert_eq!(parse("/model").unwrap().name, "/model");
        assert_eq!(parse("/mode auto").unwrap().name, "/mode");
    }

    #[test]
    fn parse_dash_alias() {
        assert_eq!(parse("-status").unwrap().name, "/status");
        assert_eq!(parse("-model gpt-4").unwrap().name, "/model");
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert!(parse("/unknown").is_none());
        assert!(parse("hello").is_none());
    }

    #[test]
    fn dash_alias_conversion() {
        assert_eq!(dash_alias_to_slash("-model"), "/model");
        assert_eq!(dash_alias_to_slash("/model"), "/model");
        assert_eq!(dash_alias_to_slash("hello"), "hello");
    }

    #[test]
    fn slash_command_preserves_arguments() {
        assert_eq!(parse_invocation("/mode plan").unwrap().args, "plan");
        assert_eq!(parse_invocation("-memory brakes").unwrap().args, "brakes");
    }
    #[test]
    fn model_accepts_args() {
        let inv = parse_invocation("/model gpt-4o-mini").unwrap();
        assert_eq!(inv.id, CommandId::Model);
        assert_eq!(inv.args, "gpt-4o-mini");
    }

    #[test]
    fn fuzzy_subsequence_filter_matches() {
        let cpt_results = filter("cpt");
        assert!(cpt_results.iter().any(|c| c.name == "/compact"));

        let st_results = filter("st");
        assert!(st_results.iter().any(|c| c.name == "/status"));

        let tk_results = filter("tk");
        assert!(tk_results.iter().any(|c| c.name == "/tasks"));
    }
}
