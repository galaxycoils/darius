//! Cancellable workspace shell: arg parsing plus output shaping.
use crate::execution::{ExecutionContext, RunEnd, ToolExecutor};
use crate::{ToolCall, ToolOutcome, spec};
use std::path::PathBuf;

/// Blocking shell executor: `sh -c <command>` in `workspace`.
pub struct ShellExecutor {
    pub workspace: PathBuf,
    pub spill_dir: PathBuf,
    pub ceiling: usize,
}

impl ToolExecutor for ShellExecutor {
    fn execute(&self, call: &ToolCall, ctx: &ExecutionContext) -> ToolOutcome {
        let command = call
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if command.is_empty() {
            return ToolOutcome::Err {
                message: "command required".into(),
            };
        }
        match crate::process_group::run_process_group(command, &self.workspace, ctx) {
            Err(message) => ToolOutcome::Err { message },
            Ok(RunEnd::Done(code, stdout, stderr)) => {
                let mut full = String::from_utf8_lossy(&stdout).into_owned();
                let se = String::from_utf8_lossy(&stderr);
                if !se.is_empty() {
                    full.push_str(&format!("\n[stderr]\n{se}"));
                }
                match code {
                    Some(0) => spec::finalize(full, &self.spill_dir, self.ceiling),
                    Some(c) => match spec::finalize(full, &self.spill_dir, self.ceiling) {
                        ToolOutcome::Ok {
                            preview,
                            spilled_path,
                        } => ToolOutcome::Err {
                            message: match spilled_path {
                                Some(path) => format!(
                                    "shell exit {c}: {preview} [output spilled to {}]",
                                    path.display()
                                ),
                                None => format!("shell exit {c}: {preview}"),
                            },
                        },
                        outcome => outcome,
                    },
                    None => ToolOutcome::Err {
                        message: "shell killed by signal".into(),
                    },
                }
            }
            Ok(RunEnd::Interrupted) => ToolOutcome::Interrupted,
            Ok(RunEnd::TimedOut) => ToolOutcome::TimedOut,
        }
    }
}
