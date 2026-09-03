//! Darius command-line entry points and retained runtime behavior.

use crate::args::{Cli, Command, ConfigCommand, MemoryCommand};
use crate::tui_runtime::TuiWorker;
use clap::{CommandFactory, Parser};
use darius_tui::{AppState, TuiController};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub mod args;
mod config;
mod config_error;
mod config_init;
mod config_publish;
mod diagnostics;
mod events;
pub mod paths;
pub mod runtime;
mod runtime_selection;
mod runtime_selector;
mod safety;
pub mod tui_runtime;

pub use config::ProfileConfig;
pub use config_error::ConfigError;
pub use config_init::{ProviderMetadata, initialize_profile};

#[derive(Clone, Copy)]
pub struct IoCaps {
    pub stdin_is_terminal: bool,
    pub stdout_is_terminal: bool,
}

impl IoCaps {
    pub fn new(stdin_is_terminal: bool, stdout_is_terminal: bool) -> Self {
        Self {
            stdin_is_terminal,
            stdout_is_terminal,
        }
    }
}

/// Parse process arguments and run the CLI using the current terminal capabilities.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_with(
        Cli::parse(),
        IoCaps::new(
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
        ),
    )
}

pub fn run_with(cli: Cli, io: IoCaps) -> Result<(), Box<dyn std::error::Error>> {
    let Cli {
        profile,
        cwd,
        offline,
        command,
    } = cli;
    match command {
        Some(Command::Tui) => cmd_tui(&profile, cwd.as_deref(), offline),
        Some(Command::Run { goal }) => cmd_run(goal, &profile, cwd.as_deref(), offline),
        Some(Command::Config { command }) => cmd_config(command, &profile, cwd.as_deref()),
        Some(Command::Memory { command }) => cmd_memory(command, &profile, cwd.as_deref()),
        None if io.stdin_is_terminal && io.stdout_is_terminal => {
            cmd_tui(&profile, cwd.as_deref(), offline)
        }
        None => {
            Cli::command().print_help()?;
            Ok(())
        }
    }
}

fn cmd_tui(
    profile: &str,
    cwd: Option<&Path>,
    offline: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = if let Some(cwd) = cwd {
        crate::tui_runtime::build_runtime_with_cwd(profile, cwd.to_path_buf(), offline)?
    } else {
        crate::tui_runtime::build_runtime(profile, offline)?
    };
    let (mut worker, event_rx) = TuiWorker::new(runtime);
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
    let worker_handle = std::thread::spawn(move || worker.run_loop(cmd_rx));
    let controller = TuiController {
        commands: cmd_tx,
        events: event_rx,
    };
    darius_tui::run_tui(AppState::default(), controller)?;
    let _ = worker_handle.join();
    Ok(())
}

fn cmd_memory(
    command: MemoryCommand,
    profile: &str,
    cwd: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = paths::DariusPaths::resolve(&paths::OsEnv, cwd)?;
    let profile_dir = get_profile_dir(&paths, profile)?;
    let engine = darius_memory::MemoryEngine::open(&profile_dir)?;

    match command {
        MemoryCommand::Search { query } => {
            let query = query.join(" ");
            let results = engine.search(&darius_memory::SearchQuery {
                text: Some(query.clone()),
                kinds: vec![],
                limit: 12,
            })?;
            println!("Search results for '{}':", query);
            for record in &results {
                println!(
                    "  - [{}] {}: {}",
                    record.kind.as_str(),
                    record.title.as_deref().unwrap_or("untitled"),
                    record.body
                );
            }
            println!("Found {} results", results.len());
        }
        MemoryCommand::Pack => {
            let pack = engine.build_pack(3500, 12)?;
            println!("Memory Pack (v{}):", pack.version);
            println!("{}", pack.plain);
            println!("({} records)", pack.record_ids.len());
        }
        MemoryCommand::Import { file } => {
            let (imported, skipped) = engine.import_jsonl(&file)?;
            println!("Imported: {imported}, Skipped: {skipped}");
        }
        MemoryCommand::Export { file } => {
            let count = engine.export_jsonl(&file)?;
            println!("Exported {count} records to {}", file.display());
        }
        MemoryCommand::Stats => {
            let count = engine.record_count()?;
            println!("Memory stats:");
            println!("  Records: {count}");
            println!("  DB path: {}", engine.db_path().display());
        }
    }
    Ok(())
}

/// Run a cognitive loop with a goal.
fn cmd_run(
    goal: Vec<String>,
    profile_name: &str,
    cwd: Option<&Path>,
    offline: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let goal = goal.join(" ");
    let paths = paths::DariusPaths::resolve(&paths::OsEnv, cwd)?;
    let mut runtime = crate::runtime::SessionRuntime::from_options(
        &paths,
        profile_name,
        crate::runtime::RuntimeOptions { offline },
    )?;
    if runtime.is_offline_demo() {
        println!("Runtime state: offline-demo");
        println!("Offline demo: no real file analysis or completion was performed.");
        return Ok(());
    }
    if runtime.is_setup() {
        println!("Runtime state: setup");
        println!("Setup required: set DARIUS_API_KEY or OPENAI_API_KEY, then run config init.");
        println!("No goal was run; no completion was claimed.");
        return Ok(());
    }
    println!("Running cognitive loop with goal: {goal}");
    let (plan, acceptance) = darius_cognitive::run_loop(
        &runtime.metadata,
        &runtime.policy,
        &goal,
        runtime.model.as_mut(),
        &mut runtime.tools,
        &runtime.memory,
    )?;

    println!("Plan: {} tasks", plan.tasks.len());
    match acceptance {
        darius_cognitive::Acceptance::Accepted => {
            println!("✓ Cognitive loop completed successfully!");
        }
        darius_cognitive::Acceptance::Rejected(reason) => {
            println!("✗ Cognitive loop rejected: {reason}");
        }
    }

    Ok(())
}

fn get_profile_dir(paths: &paths::DariusPaths, profile: &str) -> Result<PathBuf, paths::PathError> {
    paths.profile(profile)
}

fn cmd_config(
    command: ConfigCommand,
    profile: &str,
    cwd: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = paths::DariusPaths::resolve(&paths::OsEnv, cwd)?;
    match command {
        ConfigCommand::Show => {
            for line in crate::runtime::SessionRuntime::diagnostics_for(
                &paths,
                profile,
                crate::runtime::RuntimeOptions::default(),
            )? {
                println!("{line}");
            }
        }
        ConfigCommand::Init {
            provider,
            base_url,
            model,
            key_env,
            force,
        } => {
            let metadata = ProviderMetadata {
                provider,
                base_url: base_url.to_string(),
                model,
                api_key_env: Some(key_env),
            };
            let path = initialize_profile(&paths, profile, &metadata, force)?;
            println!("Initialized profile '{profile}' at {}", path.display());
        }
    }
    Ok(())
}

/// Evaluates tool risk and approval requirement without execution.
pub fn check_approval(tool: &str, args_val: &serde_json::Value) -> (bool, String, String) {
    let risk_str;
    let requires_approval;
    let reason;

    match tool {
        "shell" | "bash" => {
            risk_str = "Mutating".to_string();
            requires_approval = true;
            reason = "shell execution requires approval".to_string();
        }
        "write_file" | "hashline" => {
            risk_str = "Mutating".to_string();
            let path_str = args_val.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if !path_str.is_empty()
                && darius_safety::is_protected_path(std::path::Path::new(path_str))
            {
                requires_approval = true;
                reason = format!(
                    "write to protected instruction file '{path_str}' requires explicit approval"
                );
            } else {
                requires_approval = true;
                reason = "file mutation requires approval".to_string();
            }
        }
        "subagent_spawn" | "peer_send" => {
            risk_str = "Mutating".to_string();
            requires_approval = true;
            reason = "external agent spawn or peer send requires approval".to_string();
        }
        "read_file" | "glob" | "grep" | "memory_search" | "memory_pack" | "spill_read"
        | "read_spill" => {
            risk_str = "ReadOnly".to_string();
            requires_approval = false;
            reason = "read-only inspection tool".to_string();
        }
        _ => {
            risk_str = "Unknown".to_string();
            requires_approval = true;
            reason = format!("unrecognized tool '{tool}' defaults to requiring approval");
        }
    }

    (requires_approval, risk_str, reason)
}
