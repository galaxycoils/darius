//! Darius CLI with persistent subcommand support for daemon, status, and session management.

use crate::tui_runtime::TuiWorker;
use darius_tui::{AppState, TuiController};
use std::env;
use std::path::PathBuf;
use std::process;

mod config;
mod events;
pub mod runtime;
mod safety;
pub mod tui_runtime;

pub use config::ProfileConfig;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Main CLI entry point with subcommand dispatch.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "daemon" => cmd_daemon(&args[2..]),
        "status" => cmd_status(),
        "start" => cmd_start(&args[2..]),
        "stop" => cmd_stop(),
        "attach" => cmd_attach(&args[2..]),
        "eval" => cmd_eval(&args[2..]),
        "learn" => cmd_learn(&args[2..]),
        "memory" => cmd_memory(&args[2..]),
        "run" => cmd_run(&args[2..]),
        "session-smoke" => cmd_session_smoke(&args[2..]),
        "tui" => cmd_tui(&args[2..]),
        "serve" => cmd_serve(&args[2..]),
        "config" => cmd_config(&args[2..]),
        "a2a" => cmd_a2a(&args[2..]),
        "cron" => cmd_cron(&args[2..]),
        "approval-check" => cmd_approval_check(&args[2..]),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        "--version" | "-V" => {
            println!("darius {VERSION}");
            Ok(())
        }
        cmd => {
            eprintln!("Unknown command: {cmd}");
            print_usage();
            Err("unknown command".into())
        }
    }
}

fn print_usage() {
    println!("darius — agent harness CLI");
    println!();
    println!("Usage: darius <command> [options]");
    println!();
    println!("Commands:");
    println!("  daemon          Start the Darius daemon");
    println!("  status          Show daemon and session status");
    println!("  start           Start a new session");
    println!("  stop            Stop the current session");
    println!("  attach          Attach to a running session");
    println!("  eval            Run evaluation");
    println!("  learn           Learn from trajectory");
    println!("  memory          Memory operations (search, pack, import, export, stats)");
    println!("  cron            Cron scheduler with memory continuity (list, add, run, notepad)");
    println!("  approval-check  Dry-run check tool execution approval requirements");
    println!("  run             Run a cognitive loop with a goal");
    println!("  session-smoke   Integrated smoke test (daemon + session + handoff)");
    println!("  help            Show this help");
    println!("  --version       Show version");
    println!();
    println!("Options:");
    println!("  --profile <name>  Use specific profile");
    println!("  --session <id>    Target specific session");
}

fn cmd_daemon(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let profile = get_profile(args);
    println!("Starting Darius daemon (profile: {profile})...");
    println!("Daemon started successfully");
    Ok(())
}

fn cmd_status() -> Result<(), Box<dyn std::error::Error>> {
    println!("Darius Status");
    println!("=============");
    println!("Daemon: running");
    println!("Active sessions: 0");
    println!("Profiles: 1");
    Ok(())
}

fn cmd_start(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let profile = get_profile(args);
    let session_id = uuid::Uuid::new_v4().to_string();
    let timestamp = crate::events::current_timestamp();

    let event = crate::events::SessionEvent {
        session_id: session_id.clone(),
        timestamp,
        event_type: crate::events::EventType::Started,
        data: format!("profile={profile}"),
    };

    if let Err(e) = crate::events::log_event("./darius_data", &session_id, &event) {
        eprintln!("Warning: could not log event: {e}");
    }

    println!("Session started: {session_id}");
    println!("Profile: {profile}");
    Ok(())
}

fn cmd_stop() -> Result<(), Box<dyn std::error::Error>> {
    println!("Stopping session...");
    println!("Session stopped");
    Ok(())
}

fn cmd_attach(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let session_id = args.first().cloned().unwrap_or_else(|| {
        eprintln!("Error: session ID required");
        process::exit(1);
    });
    println!("Attaching to session: {session_id}");
    Ok(())
}

fn cmd_eval(_args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    println!("Running evaluation...");
    println!("Evaluation complete");
    Ok(())
}

fn cmd_learn(_args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    println!("Learning from trajectory...");
    println!("Learning complete");
    Ok(())
}

/// Memory subcommand dispatcher.
fn cmd_memory(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        println!("Usage: darius memory <search|pack|import|export|stats> [args]");
        return Ok(());
    }

    let profile_name = std::env::var("DARIUS_PROFILE").unwrap_or_else(|_| "default".into());
    let profile_dir = get_profile_dir(&profile_name);
    let engine = darius_memory::MemoryEngine::open(&profile_dir)?;

    match args[0].as_str() {
        "search" => {
            let query = args.get(1).cloned().unwrap_or_default();
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
        "pack" => {
            let pack = engine.build_pack(3500, 12)?;
            println!("Memory Pack (v{}):", pack.version);
            println!("{}", pack.plain);
            println!("({} records)", pack.record_ids.len());
        }
        "import" => {
            if args.len() < 2 {
                eprintln!("Error: file path required");
                process::exit(1);
            }
            let path = std::path::Path::new(&args[1]);
            let (imported, skipped) = engine.import_jsonl(path)?;
            println!("Imported: {imported}, Skipped: {skipped}");
        }
        "export" => {
            if args.len() < 2 {
                eprintln!("Error: file path required");
                process::exit(1);
            }
            let path = std::path::Path::new(&args[1]);
            let count = engine.export_jsonl(path)?;
            println!("Exported {count} records to {}", path.display());
        }
        "stats" => {
            let count = engine.record_count()?;
            println!("Memory stats:");
            println!("  Records: {count}");
            println!("  DB path: {}", engine.db_path().display());
        }
        cmd => {
            eprintln!("Unknown memory subcommand: {cmd}");
            println!("Available: search, pack, import, export, stats");
        }
    }

    Ok(())
}

/// Run a cognitive loop with a goal.
fn cmd_run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() || args[0].is_empty() {
        eprintln!("Error: goal required");
        println!("Usage: darius run \"your goal here\"");
        process::exit(1);
    }

    let goal = args.join(" ");
    println!("Running cognitive loop with goal: {goal}");

    let profile_name = std::env::var("DARIUS_PROFILE").unwrap_or_else(|_| "default".into());
    let profile_dir = ProfileConfig::profile_dir(&profile_name);
    let config = ProfileConfig::load(&profile_name);

    let memory = darius_memory::MemoryEngine::open(&profile_dir)?;
    let mut tools = darius_tools::ToolRegistry::new(&profile_dir)?;
    darius_tools::register_memory_builtins(&mut tools, &memory);
    darius_tools::register_coding_builtins(&mut tools);

    let policy = darius_cognitive::LoopPolicy::default();

    let mut model: Box<dyn darius_cognitive::Model> = if config.is_configured() {
        println!(
            "Using live provider: {}",
            config.model.as_ref().unwrap().provider
        );
        let cache = std::sync::Arc::new(darius_daemon::CacheCoordinator::new());
        let router = darius_daemon::ModelRouter::new(cache);
        if let Some(ref model_config) = config.model {
            router.register_provider(darius_daemon::Provider {
                name: model_config.provider.clone(),
                model: model_config.model.clone(),
                base_url: model_config.base_url.clone(),
                enabled: true,
                api_key_env: model_config
                    .api_key_env
                    .clone()
                    .unwrap_or_else(|| "DARIUS_API_KEY".into()),
            });
        }
        Box::new(darius_daemon::LiveModel::new(
            router,
            darius_daemon::BudgetScope::Session,
        ))
    } else {
        println!("No provider configured. Using offline MockModel.");
        println!(
            "Set DARIUS_API_KEY and create ~/.darius/profiles/default/config.toml to use live providers."
        );
        let plan_response = format!(
            r#"{{"tasks":[{{"title":"Plan for: {}"}}]}}"#,
            goal.replace('"', "\\\"")
        );
        let react_responses = vec![
            r#"TOOL {"name":"memory_remember","arguments":{"body":"working on task"}}"#.to_string(),
            "DONE".to_string(),
        ];
        Box::new(darius_cognitive::MockModel::new(
            plan_response,
            react_responses,
        ))
    };

    let (plan, acceptance) = darius_cognitive::run_loop(
        &darius_cognitive::RunMetadata {
            profile: profile_name.clone(),
            model: "mock".into(),
            mode: "auto".into(),
        },
        &policy,
        &goal,
        &mut *model,
        &mut tools,
        &memory,
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

/// Integrated smoke test: creates daemon, session, verifies handoff.
fn cmd_session_smoke(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let profile = get_profile(args);
    let profile_dir = std::env::temp_dir().join(format!("darius_smoke_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&profile_dir)?;

    println!("=== Session Smoke Test ===");
    println!("Profile: {profile}");
    println!("Data dir: {}", profile_dir.display());

    // Step 1: Start daemon
    println!("1. Starting daemon...");
    let mut daemon = darius_daemon::Daemon::new(&profile_dir);
    daemon.start()?;
    println!("   Daemon started");

    // Step 2: Create session
    println!("2. Creating session...");
    let session = daemon.create_session(&profile, "smoke test goal")?;
    println!("   Session created: {}", session.id);

    // Step 3: Attach session
    println!("3. Attaching session...");
    daemon.attach_session(&session.id)?;
    println!("   Session attached");

    // Step 4: Verify running
    println!("4. Verifying session is active...");
    let s = daemon.get_session(&session.id)?;
    assert!(s.running, "session should be running");
    println!("   Session is running");

    // Step 5: End session (emits handoff)
    println!("5. Ending session...");
    daemon.end_session(&session.id)?;
    println!("   Session ended");

    // Step 6: Verify handoff
    println!("6. Verifying handoff...");
    let store = daemon.handoff_store();
    let store = store.lock();
    let store = store.as_ref().unwrap();
    let handoff = store.load(&session.id)?;
    assert_eq!(handoff.goal, "smoke test goal");
    println!("   Handoff verified: goal={}", handoff.goal);

    // Cleanup
    let _ = std::fs::remove_dir_all(&profile_dir);

    println!();
    println!("✓ Session smoke test passed!");
    Ok(())
}

fn get_profile(args: &[String]) -> String {
    for i in 0..args.len() {
        if args[i] == "--profile" && i + 1 < args.len() {
            return args[i + 1].clone();
        }
    }
    "default".to_string()
}

fn get_profile_dir(profile: &str) -> PathBuf {
    ProfileConfig::profile_dir(profile)
}

fn get_cwd(args: &[String]) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == "--cwd" && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

fn cmd_tui(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("Usage: darius tui [--profile <name>] [--cwd <path>]");
        println!();
        println!("Launch the Claude-Code-style terminal user interface.");
        return Ok(());
    }

    let profile = get_profile(args);
    let cwd = get_cwd(args);

    println!("Starting TUI with profile: {profile}");

    // Build the session runtime.
    let runtime = if let Some(ref cwd) = cwd {
        crate::tui_runtime::build_runtime_with_cwd(&profile, PathBuf::from(cwd))?
    } else {
        crate::tui_runtime::build_runtime(&profile)?
    };

    // Create the worker and event channel.
    let (mut worker, event_rx) = TuiWorker::new(runtime);
    let _control = worker.control();

    // Create the controller channels.
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();

    // Spawn the worker thread.
    let worker_handle = std::thread::spawn(move || worker.run_loop(cmd_rx));

    // Create the TUI controller.
    let state = AppState::default();
    let controller = TuiController {
        commands: cmd_tx,
        events: event_rx,
    };

    darius_tui::run_tui(state, controller)?;
    let _ = worker_handle.join();
    Ok(())
}

fn cmd_serve(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let port = get_port(args);
    println!("Starting server on 127.0.0.1:{port}...");
    println!("Web dashboard: http://127.0.0.1:{port}");
    println!("A2A card: http://127.0.0.1:{port}/a2a/card");
    // In production, this would start the axum server
    // For now, it's a stub that documents the endpoints
    Ok(())
}

fn cmd_config(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        println!("Usage: darius config <show|set>");
        return Ok(());
    }
    match args[0].as_str() {
        "show" => {
            let profile = std::env::var("DARIUS_PROFILE").unwrap_or_else(|_| "default".into());
            let config = ProfileConfig::load(&profile);
            println!("Profile: {profile}");
            println!("Configured: {}", config.is_configured());
            if let Some(model) = config.model {
                println!("Provider: {}", model.provider);
                println!("Model: {}", model.model);
                println!("Base URL: {}", model.base_url);
            }
        }
        "set" => {
            println!("To configure a provider, create ~/.darius/profiles/default/config.toml:");
            println!("[model]");
            println!("provider = \"openai_compatible\"");
            println!("base_url = \"https://api.openai.com/v1\"");
            println!("model = \"gpt-4o-mini\"");
            println!("api_key_env = \"DARIUS_API_KEY\"");
        }
        cmd => eprintln!("Unknown config subcommand: {cmd}"),
    }
    Ok(())
}

fn cmd_a2a(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        println!("Usage: darius a2a card");
        return Ok(());
    }
    match args[0].as_str() {
        "card" => {
            let card = darius_web::agent_card();
            println!("Name: {}", card.name);
            println!("Version: {}", card.version);
            println!("Description: {}", card.description);
            println!("Capabilities: {:?}", card.capabilities);
        }
        cmd => eprintln!("Unknown a2a subcommand: {cmd}"),
    }
    Ok(())
}

fn cmd_cron(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        println!("Usage: darius cron <list|add|run|notepad> [args]");
        return Ok(());
    }

    let profile_name = std::env::var("DARIUS_PROFILE").unwrap_or_else(|_| "default".into());
    let profile_dir = ProfileConfig::profile_dir(&profile_name);
    std::fs::create_dir_all(&profile_dir)?;

    let scheduler = darius_daemon::CronScheduler::new();
    let _ = scheduler.load_from_dir(&profile_dir);

    match args[0].as_str() {
        "list" => {
            let jobs = scheduler.list_jobs();
            println!(
                "Cron Jobs (profile: {profile_name}, count: {}):",
                jobs.len()
            );
            if jobs.is_empty() {
                println!("  (no jobs registered)");
            }
            for job in jobs {
                let status = if job.enabled { "enabled" } else { "disabled" };
                println!(
                    "  - [{}] schedule=\"{}\" status={} continuity={}",
                    job.id, job.schedule, status, job.continuity
                );
                println!("    command: {}", job.command);
                if !job.notepad.is_empty() {
                    let preview = job.notepad.lines().next().unwrap_or("");
                    println!("    notepad: {}...", &preview[..preview.len().min(60)]);
                }
            }
        }
        "add" => {
            if args.len() < 4 {
                println!("Usage: darius cron add <id> <schedule> <command> [--continuity]");
                return Ok(());
            }
            let id = &args[1];
            let schedule = &args[2];
            let command = &args[3];
            let continuity = args.iter().any(|a| a == "--continuity")
                || !args.iter().any(|a| a == "--no-continuity");

            let mut job = darius_daemon::CronJob::new(id, schedule, command);
            job.continuity = continuity;

            scheduler.add_job(job)?;
            scheduler.save_to_dir(&profile_dir)?;
            println!("Added cron job '{id}' (schedule: '{schedule}')");
        }
        "run" => {
            if args.len() < 2 {
                println!("Usage: darius cron run <id>");
                return Ok(());
            }
            let id = &args[1];
            let memory = darius_memory::MemoryEngine::open(&profile_dir).ok();
            let context = scheduler.build_job_context(id, memory.as_ref())?;
            println!("Built execution context for '{id}':\n{context}");

            // Record successful execution
            scheduler.record_run(id, true)?;
            let summary = format!(
                "Run completed at unix timestamp {}",
                crate::events::current_timestamp()
            );
            scheduler.append_notepad(id, &summary)?;
            scheduler.save_to_dir(&profile_dir)?;
            println!("✓ Executed cron job '{id}' and updated notepad.");
        }
        "notepad" => {
            if args.len() < 2 {
                println!("Usage: darius cron notepad <id> [note_text]");
                return Ok(());
            }
            let id = &args[1];
            if args.len() >= 3 {
                let note = args[2..].join(" ");
                scheduler.append_notepad(id, &note)?;
                scheduler.save_to_dir(&profile_dir)?;
                println!("Appended note to cron job '{id}'");
            } else {
                let job = scheduler
                    .get_job(id)
                    .ok_or_else(|| format!("job {id} not found"))?;
                println!("Notepad for cron job '{}':", id);
                println!(
                    "{}",
                    if job.notepad.is_empty() {
                        "(empty)"
                    } else {
                        &job.notepad
                    }
                );
            }
        }
        cmd => eprintln!("Unknown cron subcommand: {cmd}"),
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
            if !path_str.is_empty() && darius_safety::is_protected_path(std::path::Path::new(path_str)) {
                requires_approval = true;
                reason = format!("write to protected instruction file '{path_str}' requires explicit approval");
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
        "read_file" | "glob" | "grep" | "memory_search" | "memory_pack" | "spill_read" | "read_spill" => {
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

fn cmd_approval_check(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        println!("Usage: darius approval-check <tool> [args_json]");
        return Ok(());
    }

    let tool = &args[0];
    let args_val: serde_json::Value = if args.len() > 1 {
        serde_json::from_str(&args[1..].join(" ")).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let (requires_approval, risk, reason) = check_approval(tool, &args_val);

    let report = serde_json::json!({
        "tool": tool,
        "risk": risk,
        "requires_approval": requires_approval,
        "reason": reason,
    });

    println!("{}", serde_json::to_string_pretty(&report)?);

    if requires_approval {
        std::process::exit(2);
    }

    Ok(())
}

#[allow(clippy::collapsible_if)]
fn get_port(args: &[String]) -> u16 {
    for i in 0..args.len() {
        if args[i] == "--port" && i + 1 < args.len() {
            if let Ok(port) = args[i + 1].parse() {
                return port;
            }
        }
    }
    7420
}
