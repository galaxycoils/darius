#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    Help,
    Clear,
    Compact,
    Model,
    Mode,
    Effort,
    Permissions,
    Memory,
    Pack,
    Tasks,
    Plan,
    Status,
    Config,
    Skills,
    A2a,
    Serve,
    Stop,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    pub id: CommandId,
    pub name: String,
    pub args: String,
}

#[derive(Debug, Clone, Copy)]
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
        description: "Show or select the session model",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::Mode,
        name: "/mode",
        description: "Cycle or select auto/manual/accept-edits/plan",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::Effort,
        name: "/effort",
        description: "Select low/medium/high/xhigh/max/ultracode",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::Permissions,
        name: "/permissions",
        description: "Show the current permission policy",
        accepts_args: true,
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
        id: CommandId::Plan,
        name: "/plan",
        description: "Enter plan mode",
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
        id: CommandId::Skills,
        name: "/skills",
        description: "List or search installed skills",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::A2a,
        name: "/a2a",
        description: "Show A2A card and task status",
        accepts_args: true,
    },
    CommandSpec {
        id: CommandId::Serve,
        name: "/serve",
        description: "Start or show the local web/A2A server",
        accepts_args: true,
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

/// Filter commands by query (case-insensitive prefix/substring match).
pub fn filter(query: &str) -> Vec<&'static CommandSpec> {
    let q = query.to_lowercase();
    COMMANDS
        .iter()
        .filter(|cmd| cmd.name.contains(&q) || cmd.description.to_lowercase().contains(&q))
        .collect()
}

/// Parse input into a command, supporting both `/command` and `-command` aliases.
#[allow(clippy::question_mark)]
pub fn parse(input: &str) -> Option<&'static CommandSpec> {
    let trimmed = input.trim();

    // Strip leading / or - and extract the command name
    let rest = if let Some(s) = trimmed.strip_prefix('/') {
        s
    } else if let Some(s) = trimmed.strip_prefix('-') {
        s
    } else {
        return None;
    };

    // Extract command name (before any space)
    let cmd_name = rest.split_whitespace().next().unwrap_or("");

    // Build canonical /command name and look it up
    if cmd_name.is_empty() {
        return None;
    }

    // Manually check against canonical names (avoid temporary String)
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
    let spec = COMMANDS.iter().find(|item| item.name == name)
        .ok_or_else(|| format!("unknown command: {name}"))?;
    if !spec.accepts_args && !args.is_empty() {
        return Err(format!("{} does not accept arguments", spec.name));
    }
    Ok(CommandInvocation { id: spec.id, name: spec.name.into(), args })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
