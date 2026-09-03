pub mod create_path;
pub mod ensure_dirs;
pub mod execution;
pub mod mcp;
pub mod path_policy;
pub mod process_group;
pub mod read_file;
pub mod search_files;
pub mod search_filter;
pub mod search_walk;
pub mod shell;
pub mod spec;
pub mod write_file;
pub use execution::{ExecutionContext, ToolExecutor};
pub use mcp::*;
pub use path_policy::PathPolicy;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("memory error: {0}")]
    Memory(#[from] darius_memory::MemoryError),
    #[error("task error: {0}")]
    Task(String),
    #[error("mcp error: {0}")]
    Mcp(#[from] McpError),
    #[error("execution error: {0}")]
    Execution(String),
}

impl From<String> for ToolError {
    fn from(s: String) -> Self {
        ToolError::Task(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolOutcome {
    Ok {
        preview: String,
        spilled_path: Option<PathBuf>,
    },
    Err {
        message: String,
    },
    /// Execution stopped via the call's cancellation token.
    Interrupted,
    /// Execution exceeded the call's deadline.
    TimedOut,
}

/// Single spill ceiling for tool previews (see `spec::SPILL_CEILING`).
pub use spec::SPILL_CEILING as PREVIEW_CEILING;

/// Task board status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

/// A task on the board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub evidence: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// In-memory task board (session-local).
pub struct TaskBoard {
    tasks: HashMap<String, Task>,
    max_tasks: usize,
    max_evidence_per_task: usize,
}

impl TaskBoard {
    pub fn new(max_tasks: usize) -> Self {
        Self {
            tasks: HashMap::new(),
            max_tasks,
            max_evidence_per_task: 5,
        }
    }

    pub fn add(&mut self, title: &str) -> Result<Task, String> {
        if self.tasks.len() >= self.max_tasks {
            return Err(format!("task board full (max {})", self.max_tasks));
        }

        let now = chrono::Utc::now().timestamp_millis();
        let id = uuid::Uuid::new_v4().to_string();
        let task = Task {
            id: id.clone(),
            title: title.to_string(),
            status: TaskStatus::Pending,
            evidence: Vec::new(),
            created_at: now,
            updated_at: now,
        };

        self.tasks.insert(id, task.clone());
        Ok(task)
    }

    pub fn list(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self.tasks.values().collect();
        tasks.sort_by_key(|t| t.created_at);
        tasks
    }

    pub fn complete(&mut self, id: &str) -> Result<(), String> {
        let task = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| format!("task {id} not found"))?;
        task.status = TaskStatus::Completed;
        task.updated_at = chrono::Utc::now().timestamp_millis();
        Ok(())
    }

    pub fn add_evidence(&mut self, id: &str, evidence: &str) -> Result<(), String> {
        let task = self
            .tasks
            .get_mut(id)
            .ok_or_else(|| format!("task {id} not found"))?;
        if task.evidence.len() < self.max_evidence_per_task {
            task.evidence.push(evidence.to_string());
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn size(&self) -> usize {
        self.tasks.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    ReadOnly,
    Mutating,
    Shell,
}

struct RegisteredTool {
    risk: ToolRisk,
    handler: ToolHandler,
}

/// Type alias for tool handler functions.
pub type ToolHandler = Box<dyn Fn(&ToolCall) -> Result<ToolOutcome, ToolError> + Send>;

/// Tool registry with disk spill for large results.
pub struct ToolRegistry {
    workspace_root: PathBuf,
    spill_dir: PathBuf,
    preview_ceiling: usize,
    handlers: HashMap<String, RegisteredTool>,
    policy: PathPolicy,
    shell_cancel: tokio_util::sync::CancellationToken,
    shell_timeout: std::time::Duration,
}

impl ToolRegistry {
    pub fn new(profile_dir: &Path) -> Result<Self, ToolError> {
        Self::new_with_roots(profile_dir, &profile_dir.join("tool_results"))
    }

    /// Bind the registry to an explicit workspace root and spill dir.
    pub fn new_with_roots(workspace_root: &Path, spill_dir: &Path) -> Result<Self, ToolError> {
        let policy = PathPolicy::new(workspace_root)?;
        std::fs::create_dir_all(spill_dir)?;
        Ok(Self {
            workspace_root: policy.root().to_path_buf(),
            spill_dir: spill_dir
                .canonicalize()
                .unwrap_or_else(|_| spill_dir.to_path_buf()),
            preview_ceiling: spec::SPILL_CEILING,
            handlers: HashMap::new(),
            policy,
            shell_cancel: tokio_util::sync::CancellationToken::new(),
            shell_timeout: std::time::Duration::from_secs(300),
        })
    }

    /// Shared cancellation token observed by registry shell calls.
    pub fn shell_cancel_token(&self) -> tokio_util::sync::CancellationToken {
        self.shell_cancel.clone()
    }

    /// Interrupt in-flight registry shell calls.
    pub fn cancel_shells(&self) {
        self.shell_cancel.cancel();
    }

    /// Default shell deadline for registry shell calls. Snapshotted by
    /// `register_coding_builtins`, so call this before registering.
    pub fn set_shell_timeout(&mut self, timeout: std::time::Duration) {
        self.shell_timeout = timeout;
    }

    /// Register a tool with explicit risk classification.
    pub fn register_with_risk<F>(&mut self, name: &str, risk: ToolRisk, handler: F)
    where
        F: Fn(&ToolCall) -> Result<ToolOutcome, ToolError> + Send + 'static,
    {
        self.handlers.insert(
            name.into(),
            RegisteredTool {
                risk,
                handler: Box::new(handler),
            },
        );
    }

    /// Register a tool defaulting to ReadOnly risk. Prefer `register_with_risk`.
    pub fn register<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(&ToolCall) -> Result<ToolOutcome, ToolError> + Send + 'static,
    {
        self.register_with_risk(name, ToolRisk::ReadOnly, handler);
    }

    /// Look up the risk classification for a registered tool.
    pub fn risk(&self, name: &str) -> Option<ToolRisk> {
        self.handlers.get(name).map(|t| t.risk)
    }

    pub fn execute(&self, call: &ToolCall) -> ToolOutcome {
        match self.handlers.get(&call.name) {
            Some(tool) => match (tool.handler)(call) {
                Ok(outcome) => outcome,
                Err(e) => ToolOutcome::Err {
                    message: e.to_string(),
                },
            },
            None => ToolOutcome::Err {
                message: format!("unknown tool: {}", call.name),
            },
        }
    }

    pub fn spill(&self, content: &str) -> (String, Option<PathBuf>) {
        if content.len() <= self.preview_ceiling {
            return (content.to_string(), None);
        }

        let preview = spec::truncate_preview(content, self.preview_ceiling);
        let filename = format!("tool_result_{}.txt", uuid::Uuid::new_v4());
        let path = self.spill_dir.join(&filename);

        match std::fs::write(&path, content) {
            Ok(()) => (preview, Some(path)),
            Err(_) => (preview, None),
        }
    }

    pub fn set_preview_ceiling(&mut self, ceiling: usize) {
        self.preview_ceiling = ceiling;
    }

    pub fn spill_dir(&self) -> &Path {
        &self.spill_dir
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn path_policy(&self) -> &PathPolicy {
        &self.policy
    }

    pub fn preview_ceiling(&self) -> usize {
        self.preview_ceiling
    }
}

/// Parse a TOOL line from model output.
pub fn parse_tool_line(line: &str) -> Option<ToolCall> {
    let line = line.trim();
    if !line.starts_with("TOOL ") {
        return None;
    }

    let json_str = &line[5..].trim();
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let name = value.get("name")?.as_str()?.to_string();
    let arguments = value
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    Some(ToolCall {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        arguments,
    })
}

/// Extract all TOOL calls from mixed prose.
pub fn extract_tool_calls(text: &str) -> Vec<ToolCall> {
    text.lines().filter_map(parse_tool_line).collect()
}

/// Register memory builtins on a tool registry.
pub fn register_memory_builtins(registry: &mut ToolRegistry, memory: &darius_memory::MemoryEngine) {
    let memory_search = memory.clone();
    let spill_dir = registry.spill_dir.clone();
    registry.register_with_risk("memory_search", ToolRisk::ReadOnly, move |call| {
        let query = call
            .arguments
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let results = memory_search.search(&darius_memory::SearchQuery {
            text: Some(query.to_string()),
            kinds: vec![],
            limit: 12,
        })?;

        let mut full_text = String::new();
        for record in &results {
            let line = format!(
                "- [{}] {}: {}\n",
                record.kind.as_str(),
                record.title.as_deref().unwrap_or("untitled"),
                record.body
            );
            full_text.push_str(&line);
        }

        if full_text.len() > 1000 {
            let preview = full_text.chars().take(1000).collect::<String>();
            let filename = format!("tool_result_{}.txt", uuid::Uuid::new_v4());
            let path = spill_dir.join(&filename);
            let _ = std::fs::write(&path, &full_text);
            Ok(ToolOutcome::Ok {
                preview,
                spilled_path: Some(path),
            })
        } else {
            Ok(ToolOutcome::Ok {
                preview: full_text,
                spilled_path: None,
            })
        }
    });

    let memory_pack = memory.clone();
    registry.register_with_risk("memory_pack", ToolRisk::ReadOnly, move |_| {
        let pack = memory_pack.build_pack(3500, 12)?;
        Ok(ToolOutcome::Ok {
            preview: pack.plain,
            spilled_path: None,
        })
    });

    let memory_remember = memory.clone();
    registry.register_with_risk("memory_remember", ToolRisk::Mutating, move |call| {
        let body = call
            .arguments
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if body.is_empty() {
            return Err(ToolError::InvalidArgs("body required".into()));
        }

        let kind = call
            .arguments
            .get("kind")
            .and_then(|v| v.as_str())
            .map(|k| match k {
                "fact" => darius_memory::RecordKind::Fact,
                "decision" => darius_memory::RecordKind::Decision,
                "preference" => darius_memory::RecordKind::Preference,
                "episode" => darius_memory::RecordKind::Episode,
                _ => darius_memory::RecordKind::Note,
            })
            .unwrap_or(darius_memory::RecordKind::Note);

        let title = call
            .arguments
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        memory_remember.upsert(darius_memory::NewRecord {
            kind,
            title,
            body: body.to_string(),
            tags: vec![],
            importance: 0.5,
            source: Some("tool".into()),
        })?;

        Ok(ToolOutcome::Ok {
            preview: format!("remembered: {}", &body[..body.len().min(80)]),
            spilled_path: None,
        })
    });
}

/// Register task board builtins on a tool registry.
pub fn register_task_builtins(
    registry: &mut ToolRegistry,
    board: std::sync::Arc<parking_lot::Mutex<TaskBoard>>,
) {
    let board_add = board.clone();
    registry.register_with_risk("task_add", ToolRisk::Mutating, move |call| {
        let title = call
            .arguments
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if title.is_empty() {
            return Err(ToolError::InvalidArgs("title required".into()));
        }

        let mut board = board_add.lock();
        let task = board.add(title)?;
        Ok(ToolOutcome::Ok {
            preview: format!("added task: {} [{}]", task.title, &task.id[..8]),
            spilled_path: None,
        })
    });

    let board_list = board.clone();
    registry.register_with_risk("task_list", ToolRisk::ReadOnly, move |_| {
        let board = board_list.lock();
        let tasks = board.list();
        let mut preview = String::new();

        for task in tasks {
            let status_symbol = match task.status {
                TaskStatus::Pending => "○",
                TaskStatus::InProgress => "◐",
                TaskStatus::Completed => "●",
                TaskStatus::Blocked => "⊘",
            };
            preview.push_str(&format!(
                "{} [{}] {}\n",
                status_symbol,
                &task.id[..8],
                task.title
            ));
        }

        Ok(ToolOutcome::Ok {
            preview,
            spilled_path: None,
        })
    });

    let board_complete = board.clone();
    registry.register_with_risk("task_complete", ToolRisk::Mutating, move |call| {
        let id = call
            .arguments
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if id.is_empty() {
            return Err(ToolError::InvalidArgs("id required".into()));
        }

        let mut board = board_complete.lock();
        let full_id = board
            .list()
            .into_iter()
            .find(|t| t.id.starts_with(id))
            .map(|t| t.id.clone());

        match full_id {
            Some(full_id) => {
                board.complete(&full_id)?;
                Ok(ToolOutcome::Ok {
                    preview: format!("completed: {}", &full_id[..8]),
                    spilled_path: None,
                })
            }
            None => Err(ToolError::InvalidArgs(format!("task {id} not found"))),
        }
    });
}

/// Register coding builtins (shell, read_file, search_files, write_file, glob, spill_read) on a tool registry.
pub fn register_coding_builtins(registry: &mut ToolRegistry) {
    let shell_executor = crate::shell::ShellExecutor {
        workspace: registry.policy.root().to_path_buf(),
        spill_dir: registry.spill_dir.clone(),
        ceiling: registry.preview_ceiling,
    };
    let shell_cancel = registry.shell_cancel.clone();
    let shell_timeout = registry.shell_timeout;
    registry.register_with_risk("shell", ToolRisk::Shell, move |call| {
        let ctx = crate::execution::ExecutionContext {
            cancel: shell_cancel.clone(),
            deadline: std::time::Instant::now() + shell_timeout,
        };
        Ok(shell_executor.execute(call, &ctx))
    });

    let spill_dir_read = registry.spill_dir.clone();
    let ceiling_read = registry.preview_ceiling;
    let policy_read = registry.policy.clone();
    registry.register_with_risk("read_file", ToolRisk::ReadOnly, move |call| {
        let path = call
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if path.is_empty() {
            return Err(ToolError::InvalidArgs("path required".into()));
        }
        let offset = call
            .arguments
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let limit = call
            .arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(read_file::DEFAULT_LIMIT);

        let content = read_file::read_paged(&policy_read, path, offset, limit)?;
        Ok(spec::finalize(content, &spill_dir_read, ceiling_read))
    });

    let policy_search = registry.policy.clone();
    let spill_dir_search = registry.spill_dir.clone();
    let ceiling_search = registry.preview_ceiling;
    registry.register_with_risk("search_files", ToolRisk::ReadOnly, move |call| {
        let dir = call
            .arguments
            .get("dir")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let name = call.arguments.get("pattern").and_then(|v| v.as_str());
        let content = call.arguments.get("content").and_then(|v| v.as_str());
        let limit = call
            .arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(search_files::MAX_RESULTS as u64) as usize;

        let hits = search_files::search(&policy_search, dir, name, content, limit)?;
        Ok(spec::finalize(
            hits.join("\n"),
            &spill_dir_search,
            ceiling_search,
        ))
    });

    register_spill_builtins(registry);

    let policy_write = registry.policy.clone();
    registry.register_with_risk("write_file", ToolRisk::Mutating, move |call| {
        let path_str = call
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let content = call
            .arguments
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if path_str.is_empty() {
            return Err(ToolError::InvalidArgs("path required".into()));
        }

        crate::ensure_dirs::ensure_parent_dirs(policy_write.root(), path_str)?;
        let path = policy_write.resolve(path_str, true)?;
        if darius_safety::is_protected_path(&path) {
            let approved = call
                .arguments
                .get("approved")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !approved {
                return Err(ToolError::InvalidArgs(format!(
                    "write to protected instruction file '{}' requires approval",
                    path.display()
                )));
            }
        }

        let bytes = write_file::write_atomic(&policy_write, path_str, content)?;
        Ok(ToolOutcome::Ok {
            preview: format!("wrote {bytes} bytes to {}", path.display()),
            spilled_path: None,
        })
    });

    registry.register_with_risk("peer_send", ToolRisk::Mutating, |call| {
        let recipient = call
            .arguments
            .get("recipient")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let intent = call
            .arguments
            .get("intent")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let authenticated = call
            .arguments
            .get("authenticated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if recipient.is_empty() || intent.is_empty() {
            return Err(ToolError::InvalidArgs(
                "recipient and intent required".into(),
            ));
        }

        if !authenticated {
            return Err(ToolError::Execution(
                "peer sending requires explicit peer discovery/auth prior step".into(),
            ));
        }

        Ok(ToolOutcome::Ok {
            preview: format!("sent peer message to {recipient} with intent {intent}"),
            spilled_path: None,
        })
    });

    let policy_glob = registry.policy.clone();
    registry.register_with_risk("glob", ToolRisk::ReadOnly, move |call| {
        let pattern = call
            .arguments
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if pattern.is_empty() {
            return Err(ToolError::InvalidArgs("pattern required".into()));
        }

        // Simple glob: walk directory and match against pattern
        let path = std::path::Path::new(pattern);
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        let parent_str = parent.to_str().unwrap_or(".");
        let parent_str = if parent_str.is_empty() {
            "."
        } else {
            parent_str
        };
        let resolved_parent = policy_glob.resolve(parent_str, false)?;
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let mut results = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&resolved_parent) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Simple wildcard match: * matches everything
                if name_str.contains(&file_name.replace('*', "")) || file_name == "*" {
                    results.push(entry.path().display().to_string());
                    if results.len() >= 50 {
                        break;
                    }
                }
            }
        }

        Ok(ToolOutcome::Ok {
            preview: results.join("\n"),
            spilled_path: None,
        })
    });
}

/// Register spill inspection tools (spill_read, read_spill).
pub fn register_spill_builtins(registry: &mut ToolRegistry) {
    let spill_dir = registry.spill_dir.clone();
    let handler = move |call: &ToolCall| {
        let path_str = call
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if path_str.is_empty() {
            return Err(ToolError::InvalidArgs("path required".into()));
        }

        let path = PathBuf::from(path_str);
        let canonical_spill = spill_dir
            .canonicalize()
            .unwrap_or_else(|_| spill_dir.clone());
        let canonical_target = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                if !path.starts_with(&spill_dir) {
                    return Err(ToolError::InvalidArgs(
                        "path must be inside tool_results/".into(),
                    ));
                }
                path.clone()
            }
        };

        if !canonical_target.starts_with(&canonical_spill) && !path.starts_with(&spill_dir) {
            return Err(ToolError::InvalidArgs(
                "path must be inside tool_results/".into(),
            ));
        }

        let content = std::fs::read_to_string(&canonical_target)
            .or_else(|_| std::fs::read_to_string(&path))
            .map_err(ToolError::Io)?;

        let offset = call
            .arguments
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let limit = call
            .arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(4000) as usize;

        let slice = if offset < content.len() {
            let remaining = &content[offset..];
            remaining.chars().take(limit).collect::<String>()
        } else {
            String::new()
        };

        Ok(ToolOutcome::Ok {
            preview: slice,
            spilled_path: None,
        })
    };

    let handler_arc = std::sync::Arc::new(handler);
    let h1 = handler_arc.clone();
    registry.register_with_risk("spill_read", ToolRisk::ReadOnly, move |call| h1(call));
    registry.register_with_risk("read_spill", ToolRisk::ReadOnly, move |call| {
        handler_arc(call)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn shell_tool_executes_command() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        let call = ToolCall {
            id: "test-1".into(),
            name: "shell".into(),
            arguments: serde_json::json!({"command": "echo hello world"}),
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => assert!(preview.contains("hello world")),
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_file_tool_reads_file() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        let call = ToolCall {
            id: "test-2".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": file_path.to_string_lossy()}),
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => assert!(preview.contains("test content")),
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_tool_writes_file() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("output.txt");

        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        let call = ToolCall {
            id: "test-3".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": file_path.to_string_lossy(), "content": "hello from write_file"}),
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => assert!(preview.contains("output.txt")),
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello from write_file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glob_tool_finds_files() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "").unwrap();
        std::fs::write(dir.join("b.rs"), "").unwrap();

        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        let call = ToolCall {
            id: "test-4".into(),
            name: "glob".into(),
            arguments: serde_json::json!({"pattern": dir.join("*").to_string_lossy()}),
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("a.txt"));
                assert!(preview.contains("b.rs"));
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_tool_returns_error() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();

        let call = ToolCall {
            id: "test-1".into(),
            name: "nonexistent".into(),
            arguments: serde_json::Value::Null,
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Err { message } => assert!(message.contains("unknown tool")),
            _ => panic!("expected error"),
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn large_payload_spills_to_disk() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        registry.set_preview_ceiling(100);

        let large_content = "x".repeat(500);
        let (preview, spilled_path) = registry.spill(&large_content);

        assert_eq!(preview.len(), 100);
        assert!(spilled_path.is_some());
        assert!(spilled_path.as_ref().unwrap().exists());

        let spilled_content = std::fs::read_to_string(spilled_path.as_ref().unwrap()).unwrap();
        assert_eq!(spilled_content.len(), 500);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn small_payload_does_not_spill() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        registry.set_preview_ceiling(100);

        let small_content = "hello world";
        let (preview, spilled_path) = registry.spill(small_content);

        assert_eq!(preview, small_content);
        assert!(spilled_path.is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_tool_line_valid() {
        let line = r#"TOOL {"name":"memory_search","arguments":{"text":"wal"}}"#;
        let call = parse_tool_line(line).unwrap();
        assert_eq!(call.name, "memory_search");
        assert_eq!(call.arguments.get("text").unwrap().as_str().unwrap(), "wal");
    }

    #[test]
    fn parse_tool_line_invalid() {
        assert!(parse_tool_line("not a tool line").is_none());
        assert!(parse_tool_line("TOOL not json").is_none());
    }

    #[test]
    fn extract_tool_calls_from_prose() {
        let text = r#"
Let me search for that information.
TOOL {"name":"memory_search","arguments":{"text":"wal"}}
Here are the results...
TOOL {"name":"memory_remember","arguments":{"body":"important fact"}}
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "memory_search");
        assert_eq!(calls[1].name, "memory_remember");
    }

    #[test]
    fn task_board_add_and_complete() {
        let mut board = TaskBoard::new(15);

        let task = board.add("test task").unwrap();
        assert_eq!(task.title, "test task");
        assert_eq!(task.status, TaskStatus::Pending);

        let tasks = board.list();
        assert_eq!(tasks.len(), 1);

        board.complete(&task.id).unwrap();
        let task = board.get(&task.id).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn task_board_rejects_overflow() {
        let mut board = TaskBoard::new(3);
        board.add("task 1").unwrap();
        board.add("task 2").unwrap();
        board.add("task 3").unwrap();

        let result = board.add("task 4");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("full"));
    }

    #[test]
    fn memory_search_builtin_returns_results() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();

        memory
            .upsert(darius_memory::NewRecord {
                kind: darius_memory::RecordKind::Fact,
                title: Some("test".into()),
                body: "wal memory test".into(),
                tags: vec![],
                importance: 0.5,
                source: None,
            })
            .unwrap();

        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_memory_builtins(&mut registry, &memory);

        let call = ToolCall {
            id: "test-1".into(),
            name: "memory_search".into(),
            arguments: serde_json::json!({"text": "wal"}),
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("wal memory test"));
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn memory_remember_builtin_stores_record() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();

        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_memory_builtins(&mut registry, &memory);

        let call = ToolCall {
            id: "test-1".into(),
            name: "memory_remember".into(),
            arguments: serde_json::json!({"body": "important fact", "kind": "fact"}),
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("remembered:"));
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        let count = memory.record_count().unwrap();
        assert_eq!(count, 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn task_builtins_workflow() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let board = std::sync::Arc::new(parking_lot::Mutex::new(TaskBoard::new(15)));
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();

        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_memory_builtins(&mut registry, &memory);
        register_task_builtins(&mut registry, board.clone());

        let add_call = ToolCall {
            id: "test-1".into(),
            name: "task_add".into(),
            arguments: serde_json::json!({"title": "my task"}),
        };
        registry.execute(&add_call);

        let list_call = ToolCall {
            id: "test-2".into(),
            name: "task_list".into(),
            arguments: serde_json::Value::Null,
        };
        let outcome = registry.execute(&list_call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("my task"));
            }
            _ => panic!("expected Ok"),
        }

        let task_id = {
            let board_guard = board.lock();
            board_guard.list()[0].id.clone()
        };
        let complete_call = ToolCall {
            id: "test-3".into(),
            name: "task_complete".into(),
            arguments: serde_json::json!({"id": task_id}),
        };
        registry.execute(&complete_call);

        let board_guard = board.lock();
        let task = board_guard.get(&task_id).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ── Tool risk classification tests ─────────────────────────────────

    #[test]
    fn tool_risk_memory_tools_are_read_only() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_memory_builtins(&mut registry, &memory);

        assert_eq!(registry.risk("memory_search"), Some(ToolRisk::ReadOnly));
        assert_eq!(registry.risk("memory_pack"), Some(ToolRisk::ReadOnly));
        assert_eq!(registry.risk("memory_remember"), Some(ToolRisk::Mutating));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_risk_task_tools_classification() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let board = std::sync::Arc::new(parking_lot::Mutex::new(TaskBoard::new(15)));
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_task_builtins(&mut registry, board);

        assert_eq!(registry.risk("task_list"), Some(ToolRisk::ReadOnly));
        assert_eq!(registry.risk("task_add"), Some(ToolRisk::Mutating));
        assert_eq!(registry.risk("task_complete"), Some(ToolRisk::Mutating));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_risk_coding_tools_classification() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        assert_eq!(registry.risk("shell"), Some(ToolRisk::Shell));
        assert_eq!(registry.risk("read_file"), Some(ToolRisk::ReadOnly));
        assert_eq!(registry.risk("write_file"), Some(ToolRisk::Mutating));
        assert_eq!(registry.risk("glob"), Some(ToolRisk::ReadOnly));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_risk_unknown_tool_returns_none() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();

        assert_eq!(registry.risk("nonexistent"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_risk_no_registered_tool_lacks_metadata() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();
        let board = std::sync::Arc::new(parking_lot::Mutex::new(TaskBoard::new(15)));
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_memory_builtins(&mut registry, &memory);
        register_task_builtins(&mut registry, board);
        register_coding_builtins(&mut registry);

        // Every registered tool must have risk metadata (not None).
        for name in registry.handlers.keys() {
            assert!(
                registry.risk(name).is_some(),
                "tool {name} lacks risk metadata"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_spill_recall_large_output() {
        let dir = std::env::temp_dir().join(format!(
            "darius_tools_spill_recall_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        registry.set_preview_ceiling(500);
        register_coding_builtins(&mut registry);

        // Create a large file (> 500 bytes)
        let large_content = "hello world oversized output ".repeat(50); // ~1450 bytes
        let file_path = dir.join("large.txt");
        std::fs::write(&file_path, &large_content).unwrap();

        let read_call = ToolCall {
            id: "call-read-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": file_path.to_str().unwrap()}),
        };

        let outcome = registry.execute(&read_call);
        let spilled_path = match outcome {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                assert_eq!(preview.len(), 500);
                assert!(spilled_path.is_some(), "expected spilled_path");
                spilled_path.unwrap()
            }
            ToolOutcome::Err { message } => panic!("read failed: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        };

        // Now recall via spill_read
        let recall_call = ToolCall {
            id: "call-recall-1".into(),
            name: "spill_read".into(),
            arguments: serde_json::json!({
                "path": spilled_path.to_str().unwrap(),
                "offset": 0,
                "limit": 100
            }),
        };

        let recall_outcome = registry.execute(&recall_call);
        match recall_outcome {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                assert_eq!(preview.len(), 100);
                assert!(spilled_path.is_none());
                assert!(preview.starts_with("hello world"));
            }
            ToolOutcome::Err { message } => panic!("recall failed: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_spill_read_gated_to_tool_results() {
        let dir =
            std::env::temp_dir().join(format!("darius_tools_spill_sec_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        let outside_file = dir.join("secret.txt");
        std::fs::write(&outside_file, "secret data").unwrap();

        let recall_call = ToolCall {
            id: "call-recall-unauth".into(),
            name: "spill_read".into(),
            arguments: serde_json::json!({"path": outside_file.to_str().unwrap()}),
        };

        let outcome = registry.execute(&recall_call);
        match outcome {
            ToolOutcome::Err { message } => {
                assert!(message.contains("inside tool_results"));
            }
            ToolOutcome::Ok { .. } => panic!("expected spill_read outside tool_results to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_to_agents_md_requires_approval() {
        let dir = std::env::temp_dir().join(format!("darius_tools_prot_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        let agents_file = dir.join("AGENTS.md");

        // 1. Without approval -> denied
        let unapproved_call = ToolCall {
            id: "call-unapproved".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": agents_file.to_str().unwrap(),
                "content": "# New Agents"
            }),
        };

        let outcome = registry.execute(&unapproved_call);
        match outcome {
            ToolOutcome::Err { message } => {
                assert!(message.contains("requires approval"));
            }
            ToolOutcome::Ok { .. } => {
                panic!("expected write to AGENTS.md without approval to fail")
            }
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }

        // 2. With approved: true -> allowed
        let approved_call = ToolCall {
            id: "call-approved".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": agents_file.to_str().unwrap(),
                "content": "# Approved Agents",
                "approved": true
            }),
        };

        let outcome = registry.execute(&approved_call);
        assert!(matches!(outcome, ToolOutcome::Ok { .. }));
        assert_eq!(
            std::fs::read_to_string(&agents_file).unwrap(),
            "# Approved Agents"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_peer_send_step_gated_auth() {
        let dir = std::env::temp_dir().join(format!("darius_tools_peer_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);

        // 1. Without authentication -> fails
        let unauth_call = ToolCall {
            id: "call-peer-unauth".into(),
            name: "peer_send".into(),
            arguments: serde_json::json!({
                "recipient": "agent-bob",
                "intent": "sync_status"
            }),
        };
        let outcome = registry.execute(&unauth_call);
        match outcome {
            ToolOutcome::Err { message } => {
                assert!(message.contains("discovery/auth"));
            }
            ToolOutcome::Ok { .. } => panic!("expected unauthenticated peer_send to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }

        // 2. With authentication -> succeeds
        let auth_call = ToolCall {
            id: "call-peer-auth".into(),
            name: "peer_send".into(),
            arguments: serde_json::json!({
                "recipient": "agent-bob",
                "intent": "sync_status",
                "authenticated": true
            }),
        };
        let outcome = registry.execute(&auth_call);
        assert!(matches!(outcome, ToolOutcome::Ok { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_policy_traversal_rejected() {
        let dir = std::env::temp_dir().join(format!(
            "darius_path_policy_traversal_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        assert!(policy.resolve("../escape.txt", false).is_err());
        assert!(policy.resolve("sub/../../escape.txt", true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_policy_symlink_escape_rejected() {
        let dir = std::env::temp_dir().join(format!(
            "darius_path_policy_symlink_{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "darius_path_policy_outside_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("link.txt")).unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        #[cfg(unix)]
        assert!(policy.resolve("link.txt", false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn path_policy_valid_create_allowed() {
        let dir = std::env::temp_dir().join(format!(
            "darius_path_policy_create_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        let created = policy.resolve("notes/new.txt", true).unwrap();
        assert!(created.starts_with(dir.canonicalize().unwrap()));
        assert!(policy.resolve("notes/new.txt", false).is_err());
        let abs = dir.join("notes/existing.txt");
        std::fs::write(&abs, "hi").unwrap();
        assert!(policy.resolve(abs.to_str().unwrap(), false).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_policy_explicit_cwd_contained() {
        let dir =
            std::env::temp_dir().join(format!("darius_path_policy_cwd_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        assert_eq!(policy.root(), &dir.canonicalize().unwrap());
        let resolved = policy.resolve("a.txt", true).unwrap();
        assert_eq!(resolved, policy.root().join("a.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_policy_new_creates_missing_root() {
        let dir = std::env::temp_dir().join(format!(
            "darius_path_policy_newroot_{}",
            uuid::Uuid::new_v4()
        ));
        let nested = dir.join("fresh").join("workspace");
        assert!(!nested.exists());
        let policy = path_policy::PathPolicy::new(&nested).unwrap();
        assert!(nested.exists());
        assert_eq!(policy.root(), &nested.canonicalize().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_policy_create_through_final_symlink_rejected() {
        let dir = std::env::temp_dir().join(format!(
            "darius_path_policy_finalsym_{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "darius_path_policy_finalsym_out_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("link.txt")).unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        #[cfg(unix)]
        assert!(policy.resolve("link.txt", true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn read_file_rejects_absolute_outside_root() {
        let dir =
            std::env::temp_dir().join(format!("darius_tools_policy_read_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        let call = ToolCall {
            id: "policy-read-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "/etc/passwd"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Err { message } => assert!(
                message.contains("escapes workspace")
                    || message.contains("must not contain")
                    || message.contains("does not exist"),
                "unexpected message: {message}"
            ),
            ToolOutcome::Ok { .. } => panic!("expected read_file /etc/passwd to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_rejects_outside_root() {
        let dir = std::env::temp_dir().join(format!(
            "darius_tools_policy_write_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let outside = std::env::temp_dir().join(format!(
            "darius_tools_policy_write_out_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&outside).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        let target = outside.join("evil.txt");
        let call = ToolCall {
            id: "policy-write-1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": target.to_str().unwrap(), "content": "evil"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Err { .. } => {}
            ToolOutcome::Ok { .. } => panic!("expected write_file outside root to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn write_file_rejects_final_symlink_plant() {
        let dir = std::env::temp_dir().join(format!(
            "darius_tools_policy_symwrite_{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "darius_tools_policy_symwrite_out_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.join("secret.txt"), dir.join("link.txt")).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        let call = ToolCall {
            id: "policy-symwrite-1".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": "link.txt", "content": "pwned"}),
        };
        #[cfg(unix)]
        match registry.execute(&call) {
            ToolOutcome::Err { .. } => {}
            ToolOutcome::Ok { .. } => panic!("expected write through final symlink to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_to_string(outside.join("secret.txt")).unwrap(),
            "secret"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    #[cfg(unix)]
    fn shell_runs_with_workspace_cwd() {
        let dir = std::env::temp_dir().join(format!(
            "darius_tools_policy_shell_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        let call = ToolCall {
            id: "policy-shell-1".into(),
            name: "shell".into(),
            arguments: serde_json::json!({"command": "pwd"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Ok { preview, .. } => assert_eq!(
                preview.trim(),
                dir.canonicalize().unwrap().to_string_lossy().trim()
            ),
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glob_rejects_outside_root() {
        let dir =
            std::env::temp_dir().join(format!("darius_tools_policy_glob_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        let call = ToolCall {
            id: "policy-glob-1".into(),
            name: "glob".into(),
            arguments: serde_json::json!({"pattern": "/etc/*"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Err { .. } => {}
            ToolOutcome::Ok { .. } => panic!("expected glob outside root to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn coding_file_tmp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("darius_coding_file_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn coding_file_registry(dir: &std::path::Path) -> ToolRegistry {
        let mut registry = ToolRegistry::new_with_roots(dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        registry
    }

    #[test]
    fn coding_file_read_pagination() {
        let dir = coding_file_tmp();
        let body = (1..=10)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(dir.join("paged.txt"), &body).unwrap();
        let registry = coding_file_registry(&dir);
        let call = ToolCall {
            id: "cf-1".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "paged.txt", "offset": 3, "limit": 4}),
        };
        match registry.execute(&call) {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                assert_eq!(preview, "line3\nline4\nline5\nline6");
                assert!(spilled_path.is_none());
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_read_rejects_binary() {
        let dir = coding_file_tmp();
        std::fs::write(dir.join("bin.dat"), b"ab\x00cd").unwrap();
        let registry = coding_file_registry(&dir);
        let call = ToolCall {
            id: "cf-2".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "bin.dat"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Err { message } => assert!(message.contains("binary")),
            ToolOutcome::Ok { .. } => panic!("expected binary read to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_read_containment() {
        let dir = coding_file_tmp();
        let registry = coding_file_registry(&dir);
        let call = ToolCall {
            id: "cf-3".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "../escape.txt"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Err { .. } => {}
            ToolOutcome::Ok { .. } => panic!("expected traversal read to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_search_recursive_name() {
        let dir = coding_file_tmp();
        std::fs::create_dir_all(dir.join("sub").join("deep")).unwrap();
        std::fs::write(dir.join("sub").join("a.txt"), "alpha").unwrap();
        std::fs::write(dir.join("sub").join("deep").join("b.txt"), "beta").unwrap();
        let registry = coding_file_registry(&dir);
        let call = ToolCall {
            id: "cf-4".into(),
            name: "search_files".into(),
            arguments: serde_json::json!({"pattern": "b.txt"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("b.txt"), "missing nested hit: {preview}");
                assert!(!preview.contains("a.txt"), "unexpected hit: {preview}");
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_search_content() {
        let dir = coding_file_tmp();
        std::fs::write(dir.join("yes.txt"), "the needle is here").unwrap();
        std::fs::write(dir.join("no.txt"), "nothing relevant").unwrap();
        let registry = coding_file_registry(&dir);
        let call = ToolCall {
            id: "cf-5".into(),
            name: "search_files".into(),
            arguments: serde_json::json!({"content": "needle"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Ok { preview, .. } => {
                assert!(
                    preview.contains("yes.txt"),
                    "missing content hit: {preview}"
                );
                assert!(!preview.contains("no.txt"), "unexpected hit: {preview}");
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_search_caps_results() {
        let dir = coding_file_tmp();
        for i in 0..60 {
            std::fs::write(dir.join(format!("f{i:02}.txt")), "x").unwrap();
        }
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        let hits = crate::search_files::search(&policy, ".", Some(".txt"), None, 10_000).unwrap();
        assert_eq!(hits.len(), 50);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_write_atomic() {
        let dir = coding_file_tmp();
        std::fs::create_dir_all(dir.join("notes")).unwrap();
        let registry = coding_file_registry(&dir);
        let call = ToolCall {
            id: "cf-7".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": "notes/out.txt", "content": "atomic-ok"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Ok { preview, .. } => assert!(preview.contains("out.txt")),
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("notes").join("out.txt")).unwrap(),
            "atomic-ok"
        );
        let entries: Vec<_> = std::fs::read_dir(dir.join("notes")).unwrap().collect();
        assert_eq!(entries.len(), 1, "temp leftovers after atomic write");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_write_containment_and_binary() {
        let dir = coding_file_tmp();
        let outside = std::env::temp_dir().join(format!("darius_cf_out_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).unwrap();
        let registry = coding_file_registry(&dir);
        let target = outside.join("evil.txt");
        let call = ToolCall {
            id: "cf-8".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": target.to_str().unwrap(), "content": "evil"}),
        };
        assert!(matches!(registry.execute(&call), ToolOutcome::Err { .. }));
        assert!(!target.exists());
        let binary = ToolCall {
            id: "cf-9".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": "bin.txt", "content": "ab\x00cd"}),
        };
        match registry.execute(&binary) {
            ToolOutcome::Err { message } => assert!(message.contains("binary")),
            ToolOutcome::Ok { .. } => panic!("expected binary write to fail"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("expected error, got terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn coding_file_spill_finalizer_enforces_32kib() {
        let dir = coding_file_tmp();
        let spill_dir = dir.join("tool_results");
        std::fs::create_dir_all(&spill_dir).unwrap();
        let big = "y".repeat(crate::spec::SPILL_CEILING + 100);
        match crate::spec::finalize(big.clone(), &spill_dir, crate::spec::SPILL_CEILING) {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                assert_eq!(preview.len(), crate::spec::SPILL_CEILING);
                let spilled = spilled_path.expect("expected spill");
                assert_eq!(std::fs::read_to_string(&spilled).unwrap(), big);
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        assert_eq!(crate::spec::SPILL_CEILING, 32 * 1024);
        match crate::spec::finalize("small".into(), &spill_dir, crate::spec::SPILL_CEILING) {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                assert_eq!(preview, "small");
                assert!(spilled_path.is_none());
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coding_file_tool_schemas_cover_file_ops() {
        let schemas = crate::spec::tool_schemas();
        let names: Vec<_> = schemas
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();
        for expected in ["read_file", "search_files", "write_file"] {
            assert!(names.contains(&expected), "missing schema: {expected}");
        }
        for schema in &schemas {
            assert!(
                schema.get("parameters").is_some(),
                "schema lacks parameters"
            );
        }
    }

    // ── RED tests for Task 2.2 caps-contract fixes ──────────────────────

    #[test]
    fn red_search_skips_oversized_content_file() {
        let dir = coding_file_tmp();
        let big = format!("needle-{}", "x".repeat(600 * 1024));
        std::fs::write(dir.join("big.txt"), &big).unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        let hits = crate::search_files::search(&policy, ".", None, Some("needle"), 50).unwrap();
        assert!(
            hits.iter().all(|h| !h.contains("big.txt")),
            "oversized file scanned: {hits:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn red_read_paged_rejects_oversized_file() {
        let dir = coding_file_tmp();
        std::fs::write(dir.join("huge.txt"), "y".repeat(2 * 1024 * 1024)).unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        match crate::read_file::read_paged(&policy, "huge.txt", 1, 10) {
            Err(crate::ToolError::InvalidArgs(msg)) => assert!(
                msg.contains("exceeds") || msg.contains("MiB") || msg.contains("bound"),
                "unexpected message: {msg}"
            ),
            Err(e) => panic!("wrong error kind: {e}"),
            Ok(_) => panic!("expected oversized read to fail"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn red_write_creates_nested_ancestors() {
        let dir = coding_file_tmp();
        let registry = coding_file_registry(&dir);
        let call = ToolCall {
            id: "red-nested".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({"path": "newdir/sub/nested.txt", "content": "nested-ok"}),
        };
        match registry.execute(&call) {
            ToolOutcome::Ok { .. } => {}
            ToolOutcome::Err { message } => panic!("nested write failed: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        assert_eq!(
            std::fs::read_to_string(dir.join("newdir/sub/nested.txt")).unwrap(),
            "nested-ok"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn red_search_depth_budget_terminates() {
        let dir = coding_file_tmp();
        let mut cur = dir.clone();
        for i in 0..60 {
            cur = cur.join(format!("d{i:02}"));
            std::fs::create_dir_all(&cur).unwrap();
        }
        std::fs::write(cur.join("bottom.txt"), "deep").unwrap();
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        let hits = crate::search_files::search(&policy, ".", Some("bottom.txt"), None, 50).unwrap();
        assert!(
            hits.iter().all(|h| !h.contains("bottom.txt")),
            "depth budget not enforced: {hits:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn red_spill_preview_truncates_by_bytes() {
        let dir = coding_file_tmp();
        let spill_dir = dir.join("tool_results");
        std::fs::create_dir_all(&spill_dir).unwrap();
        let full = "é".repeat(100); // 200 bytes, 100 chars
        match crate::spec::finalize(full, &spill_dir, 10) {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                assert!(
                    preview.len() <= 10,
                    "preview exceeds byte ceiling: {} bytes",
                    preview.len()
                );
                assert!(spilled_path.is_some());
            }
            ToolOutcome::Err { message } => panic!("unexpected error: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_budget_caps_visited_entries() {
        let dir = coding_file_tmp();
        for i in 0..60 {
            std::fs::write(dir.join(format!("g{i:02}.txt")), "x").unwrap();
        }
        let policy = path_policy::PathPolicy::new(&dir).unwrap();
        let (hits, visited) =
            crate::search_files::search_with_budget(&policy, ".", Some(".txt"), None, 50, 10, 24)
                .unwrap();
        assert!(visited <= 10, "visited budget exceeded: {visited}");
        assert!(hits.len() <= 10);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spill_preview_stays_on_char_boundary() {
        assert_eq!(crate::spec::truncate_preview("ééé", 5), "éé");
        assert_eq!(crate::spec::truncate_preview("abc", 10), "abc");
        assert_eq!(crate::spec::SPILL_CEILING, 32 * 1024);
        assert_eq!(crate::PREVIEW_CEILING, crate::spec::SPILL_CEILING);
    }

    // ── Task 2.3 cancellable shell (RED first) ───────────────────────

    fn shell_tmp(prefix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("tool_results")).unwrap();
        dir
    }

    fn shell_executor(dir: &std::path::Path) -> crate::shell::ShellExecutor {
        crate::shell::ShellExecutor {
            workspace: dir.to_path_buf(),
            spill_dir: dir.join("tool_results"),
            ceiling: crate::spec::SPILL_CEILING,
        }
    }

    fn shell_call(id: &str, command: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: "shell".into(),
            arguments: serde_json::json!({"command": command}),
        }
    }

    fn shell_ctx(millis: u64) -> ExecutionContext {
        ExecutionContext {
            cancel: tokio_util::sync::CancellationToken::new(),
            deadline: std::time::Instant::now() + std::time::Duration::from_millis(millis),
        }
    }

    #[test]
    #[cfg(unix)]
    fn shell_cwd_is_workspace() {
        let dir = shell_tmp("darius_shell_cwd");
        let ex = shell_executor(&dir);
        match ex.execute(&shell_call("cwd-1", "pwd"), &shell_ctx(10_000)) {
            ToolOutcome::Ok { preview, .. } => assert_eq!(
                preview.trim(),
                dir.canonicalize().unwrap().to_string_lossy().trim()
            ),
            other => panic!("unexpected outcome: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_success_captures_output() {
        let dir = shell_tmp("darius_shell_ok");
        let ex = shell_executor(&dir);
        match ex.execute(&shell_call("ok-1", "echo hello-shell"), &shell_ctx(10_000)) {
            ToolOutcome::Ok { preview, .. } => assert!(preview.contains("hello-shell")),
            other => panic!("unexpected outcome: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_nonzero_is_error() {
        let dir = shell_tmp("darius_shell_exit");
        let ex = shell_executor(&dir);
        match ex.execute(&shell_call("exit-1", "exit 3"), &shell_ctx(10_000)) {
            ToolOutcome::Err { message } => {
                assert!(message.contains('3'), "missing exit code: {message}")
            }
            other => panic!("nonzero exit must not succeed: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_stderr_captured() {
        let dir = shell_tmp("darius_shell_stderr");
        let ex = shell_executor(&dir);
        match ex.execute(&shell_call("stderr-1", "echo oops >&2"), &shell_ctx(10_000)) {
            ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("oops"), "missing stderr: {preview}")
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_timeout_kills_process() {
        let dir = shell_tmp("darius_shell_timeout");
        let ex = shell_executor(&dir);
        let start = std::time::Instant::now();
        let outcome = ex.execute(&shell_call("timeout-1", "sleep 30"), &shell_ctx(300));
        assert!(
            matches!(outcome, ToolOutcome::TimedOut),
            "expected timeout: {outcome:?}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "kill too slow"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_cancellation_interrupts_long_command() {
        let dir = shell_tmp("darius_shell_cancel");
        let ex = shell_executor(&dir);
        let cancel = tokio_util::sync::CancellationToken::new();
        let ctx = ExecutionContext {
            cancel: cancel.clone(),
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(30),
        };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            cancel.cancel();
        });
        let start = std::time::Instant::now();
        let outcome = ex.execute(&shell_call("cancel-1", "sleep 30"), &ctx);
        assert!(
            matches!(outcome, ToolOutcome::Interrupted),
            "expected interrupt: {outcome:?}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "cancel too slow"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_child_group_reaped() {
        let dir = shell_tmp("darius_shell_reap");
        let watch = dir.join(format!("reap-{}.log", uuid::Uuid::new_v4()));
        std::fs::write(&watch, "start\n").unwrap();
        let needle = watch.file_name().unwrap().to_string_lossy().to_string();
        let ex = shell_executor(&dir);
        let cancel = tokio_util::sync::CancellationToken::new();
        let ctx = ExecutionContext {
            cancel: cancel.clone(),
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(30),
        };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            cancel.cancel();
        });
        let outcome = ex.execute(
            &shell_call("reap-1", &format!("tail -f {} & wait", watch.display())),
            &ctx,
        );
        assert!(
            matches!(outcome, ToolOutcome::Interrupted),
            "expected interrupt: {outcome:?}"
        );
        let start = std::time::Instant::now();
        loop {
            let ps = std::process::Command::new("ps")
                .args(["ax", "-o", "pid,command"])
                .output()
                .expect("ps failed");
            let out = String::from_utf8_lossy(&ps.stdout);
            if !out.contains(&needle) {
                break;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(5),
                "child survived killpg:\n{out}"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_large_output_spills() {
        let dir = shell_tmp("darius_shell_spill");
        let ex = shell_executor(&dir);
        let cmd = "awk 'BEGIN{for(i=0;i<50000;i++) print \"line-\" i}'";
        match ex.execute(&shell_call("spill-1", cmd), &shell_ctx(30_000)) {
            ToolOutcome::Ok {
                preview,
                spilled_path,
            } => {
                assert!(
                    preview.len() <= crate::spec::SPILL_CEILING,
                    "preview over ceiling: {}",
                    preview.len()
                );
                let spilled = spilled_path.expect("large output must spill");
                let full = std::fs::read_to_string(&spilled).unwrap();
                assert!(full.len() > crate::spec::SPILL_CEILING);
                assert!(full.contains("line-49999"));
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_nonzero_output_spills() {
        let dir = shell_tmp("darius_shell_err_spill");
        let ex = shell_executor(&dir);
        let cmd = "awk 'BEGIN{for(i=0;i<50000;i++) print \"line-\" i}'; exit 4";
        match ex.execute(&shell_call("err-spill-1", cmd), &shell_ctx(30_000)) {
            ToolOutcome::Err { message } => {
                assert!(message.contains("shell exit 4"), "missing exit code");
                assert!(
                    message.len() <= crate::spec::SPILL_CEILING + 512,
                    "unbounded error: {} bytes",
                    message.len()
                );
                assert!(message.contains("spilled"), "missing spill note");
            }
            other => panic!("nonzero exit must error: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    fn shell_registry(dir: &std::path::Path) -> ToolRegistry {
        let mut registry = ToolRegistry::new_with_roots(dir, &dir.join("tool_results")).unwrap();
        register_coding_builtins(&mut registry);
        registry
    }

    #[test]
    #[cfg(unix)]
    fn shell_registry_cancel_interrupts() {
        let dir = shell_tmp("darius_shell_reg_cancel");
        let registry = shell_registry(&dir);
        registry.shell_cancel_token().cancel();
        let start = std::time::Instant::now();
        let outcome = registry.execute(&shell_call("reg-cancel-1", "sleep 5"));
        assert!(
            matches!(outcome, ToolOutcome::Interrupted),
            "expected interrupt: {outcome:?}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "cancel too slow"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn shell_registry_timeout_applies() {
        let dir = shell_tmp("darius_shell_reg_timeout");
        let mut registry = ToolRegistry::new_with_roots(&dir, &dir.join("tool_results")).unwrap();
        // Timeout is snapshotted when builtins register, so set it first.
        registry.set_shell_timeout(std::time::Duration::from_millis(300));
        register_coding_builtins(&mut registry);
        let start = std::time::Instant::now();
        let outcome = registry.execute(&shell_call("reg-timeout-1", "sleep 10"));
        assert!(
            matches!(outcome, ToolOutcome::TimedOut),
            "expected timeout: {outcome:?}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "kill too slow"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
