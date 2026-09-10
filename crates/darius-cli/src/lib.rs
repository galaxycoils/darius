//! Darius command-line entry points and retained runtime behavior.

use crate::args::{Cli, Command, ConfigCommand, MemoryCommand};
use crate::tui_runtime::TuiWorker;
use clap::{CommandFactory, Parser};
use darius_tui::{AppState, TuiController};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub mod args;
pub(crate) mod command_handler;
pub mod commands;
mod config;
mod config_error;
mod config_init;
pub mod config_probe;
mod config_publish;
mod diagnostics;
mod events;
pub mod paths;
mod permissions;
pub mod runtime;
mod runtime_selection;
mod runtime_selector;
mod safety;
pub mod tui_runtime;
pub mod web_bridge;

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
        Some(Command::Serve { host, port }) => {
            cmd_serve(host, port, &profile, cwd.as_deref(), offline)
        }
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
    let initial_profile = runtime.metadata.profile.clone();
    let initial_model = runtime.metadata.model.clone();
    let initial_workspace = runtime.workspace.clone();
    let initial_mode = runtime.mode;

    let (mut worker, event_rx) = TuiWorker::new(runtime);
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let worker_handle = std::thread::spawn(move || worker.run_loop(cmd_rx));
    let controller = TuiController {
        commands: cmd_tx,
        events: event_rx,
    };
    let state = AppState {
        profile: initial_profile,
        model: initial_model,
        cwd: Some(initial_workspace),
        mode: initial_mode,
        ..Default::default()
    };

    darius_tui::run_tui(state, controller)?;
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
    println!("Running agent loop with goal: {goal}");
    let workspace = runtime.workspace.to_string_lossy().to_string();
    let (tx, _rx) = std::sync::mpsc::channel();
    let sink = std::sync::Arc::new(darius_cognitive::ChannelEventSink::new(tx));
    let control = std::sync::Arc::new(permissions::HeadlessRunControl::default());
    let loopt = darius_cognitive::AgentLoop::new(sink, control.clone());
    let text = crate::runtime::block_on_turn(loopt.run_turn_with_extra_tools(
        &runtime.metadata,
        &runtime.policy,
        &goal,
        &mut runtime.conversation,
        runtime.model.as_mut(),
        &runtime.tools,
        &runtime.memory,
        &workspace,
        &runtime.dynamic_tool_specs,
    ));
    if control.0.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(
            "Mutation denied: run requires interactive approval; use the TUI (`darius tui`)".into(),
        );
    }
    let text = text?;
    println!("{text}");

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
        ConfigCommand::Probe => {
            crate::runtime::block_on_turn(crate::config_probe::run_config_probe(&paths, profile))?;
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
        ConfigCommand::Preset { name, force } => {
            let metadata =
                ProviderMetadata::from_preset_or_fields(Some(&name), None, None, None, None)?;
            let path = initialize_profile(&paths, profile, &metadata, force)?;
            println!(
                "Initialized profile '{profile}' from preset '{name}' at {}",
                path.display()
            );
        }
    }
    Ok(())
}

/// Evaluate the same closed-world risk policy used by model execution.
pub fn check_approval(tool: &str, args_val: &serde_json::Value) -> (bool, String, String) {
    use darius_tools::{ToolRisk, model_tools::model_tool_risk};
    let Some(risk) = model_tool_risk(tool) else {
        return (
            true,
            "Unknown".into(),
            "unknown or hidden tool; execution denied".into(),
        );
    };
    let reason = match risk {
        ToolRisk::ReadOnly => "read-only inspection tool",
        ToolRisk::Shell => "shell execution requires approval",
        ToolRisk::Mutating
            if tool == "write_file"
                && args_val
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|path| {
                        darius_safety::is_protected_path(std::path::Path::new(path))
                    }) =>
        {
            "protected instruction file; execution denied"
        }
        ToolRisk::Mutating => "mutation requires approval",
    };
    (
        risk != ToolRisk::ReadOnly,
        format!("{risk:?}"),
        reason.into(),
    )
}

fn cmd_serve(
    host: String,
    port: u16,
    profile: &str,
    cwd: Option<&Path>,
    offline: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = paths::DariusPaths::resolve(&paths::OsEnv, cwd)?;
    let state = web_bridge::server_state(paths, profile.to_owned(), offline)
        .map_err(std::io::Error::other)?;
    crate::runtime::block_on_turn(async {
        let router = darius_web::create_router(state);
        let addr = format!("{host}:{port}");
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        println!("Darius web server listening on {addr}");
        axum::serve(listener, router).await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}
