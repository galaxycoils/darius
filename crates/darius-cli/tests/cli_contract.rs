use assert_cmd::Command as AssertCommand;
use clap::{Parser, error::ErrorKind};
use darius_cli::args::{Cli, Command, ConfigCommand, MemoryCommand};
use std::path::{Path, PathBuf};
use std::process::Output;

fn binary(args: &[&str]) -> Output {
    let mut command = AssertCommand::cargo_bin("darius").expect("darius binary not found");
    command.args(args).output().expect("spawn failed")
}

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("darius").chain(args.iter().copied()))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn public_run_entrypoint_retains_compatibility_signature() {
    let run: fn() -> Result<(), Box<dyn std::error::Error>> = darius_cli::run;
    let _ = run;
}

#[test]
fn version_flags_print_the_current_package_version() {
    let expected = format!("darius {}\n", env!("CARGO_PKG_VERSION"));

    for flag in ["--version", "-V"] {
        let output = binary(&[flag]);
        assert!(output.status.success(), "flag: {flag}");
        assert_eq!(stdout(&output), expected, "flag: {flag}");
    }
}

#[test]
fn public_help_is_exact_generated_surface() {
    let output = binary(&["--help"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "Local-first coding agent\n\nUsage: darius [OPTIONS] [COMMAND]\n\nCommands:\n  tui     \n  run     \n  config  \n  memory  \n\nOptions:\n      --profile <PROFILE>  [default: default]\n      --cwd <CWD>          \n      --offline            \n  -h, --help               Print help\n  -V, --version            Print version\n"
    );
}

#[test]
fn globals_parse_before_and_after_subcommand() {
    for args in [
        &["--profile", "work", "--cwd", "/tmp", "--offline", "tui"][..],
        &["tui", "--profile", "work", "--cwd", "/tmp", "--offline"][..],
    ] {
        let cli = parse(args).expect("global options should parse on either side");
        assert_eq!(cli.profile, "work");
        assert_eq!(cli.cwd, Some(PathBuf::from("/tmp")));
        assert!(cli.offline);
        assert!(matches!(cli.command, Some(Command::Tui)));
    }
}

#[test]
fn run_requires_one_or_more_goal_words() {
    assert_eq!(
        parse(&["run"]).err().unwrap().kind(),
        ErrorKind::MissingRequiredArgument
    );
    let cli = parse(&["run", "inspect", "this", "repo"]).unwrap();
    match cli.command {
        Some(Command::Run { goal }) => assert_eq!(goal, ["inspect", "this", "repo"]),
        _ => panic!("expected run command"),
    }
}

#[test]
fn config_init_requires_and_parses_all_provider_fields() {
    let complete = [
        "config",
        "init",
        "--provider",
        "openai",
        "--base-url",
        "https://api.example.com/v1",
        "--model",
        "model-1",
        "--key-env",
        "DARIUS_KEY",
    ];
    for missing in ["--provider", "--base-url", "--model", "--key-env"] {
        let mut args = complete.to_vec();
        let index = args.iter().position(|arg| *arg == missing).unwrap();
        args.drain(index..=index + 1);
        assert_eq!(
            parse(&args).err().unwrap().kind(),
            ErrorKind::MissingRequiredArgument
        );
    }

    let mut forced = complete.to_vec();
    forced.push("--force");
    let cli = parse(&forced).unwrap();
    match cli.command {
        Some(Command::Config {
            command:
                ConfigCommand::Init {
                    provider,
                    base_url,
                    model,
                    key_env,
                    force,
                },
        }) => {
            assert_eq!(provider, "openai");
            assert_eq!(base_url.as_str(), "https://api.example.com/v1");
            assert_eq!(model, "model-1");
            assert_eq!(key_env, "DARIUS_KEY");
            assert!(force);
        }
        _ => panic!("expected config init command"),
    }
}

#[test]
fn memory_arguments_are_explicit_and_required() {
    assert_eq!(
        parse(&["memory", "search"]).err().unwrap().kind(),
        ErrorKind::MissingRequiredArgument
    );
    for command in ["import", "export"] {
        assert_eq!(
            parse(&["memory", command]).err().unwrap().kind(),
            ErrorKind::MissingRequiredArgument
        );
    }

    let cli = parse(&["memory", "search", "rust", "cli"]).unwrap();
    match cli.command {
        Some(Command::Memory {
            command: MemoryCommand::Search { query },
        }) => {
            assert_eq!(query, ["rust", "cli"]);
        }
        _ => panic!("expected memory search command"),
    }
    assert!(matches!(
        parse(&["memory", "import", "records.jsonl"]).unwrap().command,
        Some(Command::Memory {
            command: MemoryCommand::Import { file }
        }) if file == Path::new("records.jsonl")
    ));
    assert!(matches!(
        parse(&["memory", "export", "records.jsonl"]).unwrap().command,
        Some(Command::Memory {
            command: MemoryCommand::Export { file }
        }) if file == Path::new("records.jsonl")
    ));
}

#[test]
fn no_command_on_non_tty_prints_generated_help_and_exits_zero() {
    let output = binary(&[]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), stdout(&binary(&["--help"])));
}

#[test]
fn parse_errors_unknown_and_legacy_commands_exit_two() {
    for args in [
        &["wat"][..],
        &["--session", "old"][..],
        &["run"][..],
        &["config"][..],
        &["config", "init", "--provider", "openai"][..],
        &["memory", "search"][..],
    ] {
        assert_eq!(binary(args).status.code(), Some(2), "args: {args:?}");
    }
    for token in [
        "daemon",
        "status",
        "start",
        "stop",
        "attach",
        "eval",
        "learn",
        "session-smoke",
        "serve",
        "a2a",
        "cron",
        "approval-check",
        "help",
    ] {
        assert_eq!(
            binary(&[token]).status.code(),
            Some(2),
            "legacy token: {token}"
        );
    }
}

#[test]
fn explicit_memory_and_config_variants_parse() {
    assert!(matches!(
        parse(&["config", "show"]).unwrap().command,
        Some(Command::Config {
            command: ConfigCommand::Show
        })
    ));
    for (name, expected) in [
        ("pack", MemoryCommand::Pack),
        ("stats", MemoryCommand::Stats),
    ] {
        let command = parse(&["memory", name]).unwrap().command;
        assert!(
            matches!(
                (command, expected),
                (
                    Some(Command::Memory {
                        command: MemoryCommand::Pack
                    }),
                    MemoryCommand::Pack
                ) | (
                    Some(Command::Memory {
                        command: MemoryCommand::Stats
                    }),
                    MemoryCommand::Stats
                )
            ),
            "variant: {name}"
        );
    }
}
