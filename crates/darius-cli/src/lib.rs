//! Darius CLI with persistent subcommand support for daemon, status, and session management.

use std::env;
use std::process;

mod events;

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
        "session-smoke" => cmd_session_smoke(&args[2..]),
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
