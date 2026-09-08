use crate::tui_runtime::{SessionActor, State};
use darius_cognitive::{TaskSnapshot, UiEvent};
use darius_core::commands::{COMMANDS, CommandId, CommandInvocation};

pub(crate) fn handle_slash(actor: &mut SessionActor, inv: &CommandInvocation) {
    if matches!(actor.state, State::Running(_)) {
        match inv.id {
            CommandId::Stop => actor.interrupt(),
            CommandId::Quit => unreachable!("Quit must be handled in dispatch"),
            CommandId::Status => {
                if let State::Running(turn) = &actor.state {
                    for line in &turn.view.lines {
                        actor.status(line);
                    }
                }
                actor.status("Running: true");
            }
            CommandId::Permissions => actor.permissions(),
            _ => {
                actor.emit(UiEvent::Busy {
                    message: "Busy: turn running".into(),
                });
            }
        }
        return;
    }

    match inv.id {
        CommandId::Help => {
            let mut help = String::from("Available commands:\n");
            for cmd in COMMANDS {
                help.push_str(&format!("  {:14} - {}\n", cmd.name, cmd.description));
            }
            help.push_str("\nKeyboard shortcuts:\n");
            help.push_str(
                "  Enter: Submit | Esc: Palette | Shift+Tab: Cycle Mode | Ctrl+C: Interrupt",
            );
            actor.status(help);
        }
        CommandId::Clear => {
            actor.emit(UiEvent::ClearTranscript);
        }
        CommandId::Compact => {
            let (before, after, count) = {
                let State::Idle(runtime) = &mut actor.state else {
                    return;
                };
                let before = runtime.conversation.len();
                let _ = runtime.compact_conversation();
                let after = runtime.conversation.len();
                let count = runtime.memory.record_count().unwrap_or(0);
                (before, after, count)
            };
            actor.status(format!(
                "Compacted conversation: {before} -> {after} messages (memory records: {count})"
            ));
        }
        CommandId::Model => {
            let model = {
                let State::Idle(runtime) = &actor.state else {
                    return;
                };
                runtime.metadata.model.clone()
            };
            actor.status(format!("Provider/Model: {model}"));
        }
        CommandId::Mode => {
            actor.set_mode(&inv.args);
        }
        CommandId::Permissions => {
            actor.permissions();
        }
        CommandId::Memory => {
            let query = inv.args.trim();
            if query.is_empty() {
                let count = {
                    let State::Idle(runtime) = &actor.state else {
                        return;
                    };
                    runtime.memory.record_count().unwrap_or(0)
                };
                actor.status(format!("Durable memory: {count} records"));
            } else {
                let res = {
                    let State::Idle(runtime) = &actor.state else {
                        return;
                    };
                    runtime.memory.search(&darius_memory::SearchQuery {
                        text: Some(query.to_string()),
                        kinds: vec![],
                        limit: 10,
                    })
                };
                match res {
                    Ok(records) if records.is_empty() => {
                        actor.status(format!("No memory records matching \"{query}\""));
                    }
                    Ok(records) => {
                        actor.status(format!(
                            "Memory search for \"{query}\" ({} matches):",
                            records.len()
                        ));
                        for r in records {
                            actor.status(format!(
                                "  [{}] {}: {}",
                                r.kind.as_str(),
                                r.title.as_deref().unwrap_or("untitled"),
                                r.body
                            ));
                        }
                    }
                    Err(e) => {
                        actor.emit(UiEvent::Error {
                            message: format!("Memory search error: {e}"),
                        });
                    }
                }
            }
        }
        CommandId::Pack => {
            let res = {
                let State::Idle(runtime) = &actor.state else {
                    return;
                };
                runtime.memory.build_pack(4000, 20)
            };
            match res {
                Ok(pack) => {
                    actor.status(format!(
                        "MemoryPack: {} chars across {} records",
                        pack.plain.len(),
                        pack.record_ids.len()
                    ));
                    if !pack.plain.is_empty() {
                        for line in pack.plain.lines() {
                            actor.status(format!("  {line}"));
                        }
                    }
                }
                Err(e) => {
                    actor.emit(UiEvent::Error {
                        message: format!("Pack build error: {e}"),
                    });
                }
            }
        }
        CommandId::Tasks => {
            let (lines, task_views) = {
                let State::Idle(runtime) = &actor.state else {
                    return;
                };
                let board = runtime.task_board.lock();
                let tasks = board.list();
                let lines = if tasks.is_empty() {
                    vec!["Task board is empty".to_string()]
                } else {
                    let mut l = vec![format!("Task board ({} tasks):", tasks.len())];
                    for t in &tasks {
                        l.push(format!("  [{:?}] {}", t.status, t.title));
                    }
                    l
                };
                let views: Vec<TaskSnapshot> = tasks
                    .iter()
                    .map(|t| TaskSnapshot {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        status: match t.status {
                            darius_tools::TaskStatus::Pending => "pending".into(),
                            darius_tools::TaskStatus::InProgress => "active".into(),
                            darius_tools::TaskStatus::Completed => "done".into(),
                            darius_tools::TaskStatus::Blocked => "blocked".into(),
                        },
                    })
                    .collect();
                (lines, views)
            };
            for line in lines {
                actor.status(line);
            }
            actor.emit(UiEvent::TaskBoard(task_views));
        }
        CommandId::Status => {
            let lines = match &actor.state {
                State::Idle(runtime) => {
                    let mut l = runtime.diagnostics().to_vec();
                    l.push("Running: false".to_string());
                    l
                }
                State::Running(turn) => {
                    let mut l = turn.view.lines.clone();
                    l.push("Running: true".to_string());
                    l
                }
                State::Stopped => vec![],
            };
            for line in lines {
                actor.status(line);
            }
        }
        CommandId::Config => {
            let lines = {
                let State::Idle(runtime) = &actor.state else {
                    return;
                };
                let mut l = vec![
                    format!("Profile: {}", runtime.config.profile),
                    format!(
                        "Profile directory: {}",
                        runtime.config.profile_dir.display()
                    ),
                    format!("Workspace: {}", runtime.workspace.display()),
                ];
                if let Some(model) = &runtime.profile_config.model {
                    l.push(format!("Model provider: {}", model.provider));
                    l.push(format!("Model name: {}", model.model));
                    l.push(format!(
                        "Base URL: {}",
                        crate::diagnostics::strip_url_secrets(&model.base_url)
                    ));
                    if let Some(env) = &model.api_key_env {
                        l.push(format!(
                            "API key env: {env} (set: {})",
                            std::env::var(env).is_ok()
                        ));
                    }
                } else {
                    l.push("Model: not configured".to_string());
                }
                l
            };
            for line in lines {
                actor.status(line);
            }
        }
        CommandId::Stop => {
            // Idle; no-op
        }
        CommandId::Quit => unreachable!("Quit must be handled in dispatch"),
    }
}
