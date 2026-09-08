use darius_core::commands::{COMMANDS, filter};

#[test]
fn generated_slash_palette_has_only_supported_commands() {
    let expected = [
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
    ];
    let palette = filter("/");
    assert_eq!(palette.iter().map(|c| c.name).collect::<Vec<_>>(), expected);
    assert_eq!(COMMANDS.len(), expected.len());
    let help = COMMANDS
        .iter()
        .map(|c| format!("{} {}", c.name, c.description))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    for hidden in [
        "cron",
        "approval-check",
        "peer_send",
        "mcp",
        "subagent",
        "worktree",
        "rollback",
        "a2a",
    ] {
        assert!(
            !help.contains(hidden),
            "unsupported generated claim: {hidden}"
        );
    }
}
