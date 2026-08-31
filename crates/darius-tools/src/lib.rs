//! Minimal tool ACI — registry, spill, TOOL line protocol, builtins.

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
}

const DEFAULT_PREVIEW_CEILING: usize = 32_768;

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

/// Type alias for tool handler functions.
pub type ToolHandler = Box<dyn Fn(&ToolCall) -> Result<ToolOutcome, ToolError>>;

/// Tool registry with disk spill for large results.
pub struct ToolRegistry {
    spill_dir: PathBuf,
    preview_ceiling: usize,
    handlers: HashMap<String, ToolHandler>,
}

impl ToolRegistry {
    pub fn new(profile_dir: &Path) -> Result<Self, ToolError> {
        let spill_dir = profile_dir.join("tool_results");
        std::fs::create_dir_all(&spill_dir)?;
        Ok(Self {
            spill_dir,
            preview_ceiling: DEFAULT_PREVIEW_CEILING,
            handlers: HashMap::new(),
        })
    }

    pub fn register<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(&ToolCall) -> Result<ToolOutcome, ToolError> + 'static,
    {
        self.handlers.insert(name.into(), Box::new(handler));
    }

    pub fn execute(&self, call: &ToolCall) -> ToolOutcome {
        match self.handlers.get(&call.name) {
            Some(handler) => match handler(call) {
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

        let preview = content
            .chars()
            .take(self.preview_ceiling)
            .collect::<String>();
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
    registry.register("memory_search", move |call| {
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

        let mut preview = String::new();
        for record in &results {
            let line = format!(
                "- [{}] {}: {}\n",
                record.kind.as_str(),
                record.title.as_deref().unwrap_or("untitled"),
                record.body
            );
            if preview.len() + line.len() > 1000 {
                break;
            }
            preview.push_str(&line);
        }

        Ok(ToolOutcome::Ok {
            preview,
            spilled_path: None,
        })
    });

    let memory_pack = memory.clone();
    registry.register("memory_pack", move |_| {
        let pack = memory_pack.build_pack(3500, 12)?;
        Ok(ToolOutcome::Ok {
            preview: pack.plain,
            spilled_path: None,
        })
    });

    let memory_remember = memory.clone();
    registry.register("memory_remember", move |call| {
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
    registry.register("task_add", move |call| {
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
    registry.register("task_list", move |_| {
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
    registry.register("task_complete", move |call| {
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

/// Register coding builtins (shell, read_file, write_file, glob) on a tool registry.
pub fn register_coding_builtins(registry: &mut ToolRegistry) {
    registry.register("shell", |call| {
        let command = call
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if command.is_empty() {
            return Err(ToolError::InvalidArgs("command required".into()));
        }

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .map_err(ToolError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut preview = stdout;
        if !stderr.is_empty() {
            preview.push_str(&format!("\n[stderr]\n{stderr}"));
        }

        Ok(ToolOutcome::Ok {
            preview: preview.chars().take(2000).collect(),
            spilled_path: None,
        })
    });

    registry.register("read_file", |call| {
        let path = call
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if path.is_empty() {
            return Err(ToolError::InvalidArgs("path required".into()));
        }

        let content = std::fs::read_to_string(path)?;
        Ok(ToolOutcome::Ok {
            preview: content.chars().take(2000).collect(),
            spilled_path: None,
        })
    });

    registry.register("write_file", |call| {
        let path = call
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let content = call
            .arguments
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if path.is_empty() {
            return Err(ToolError::InvalidArgs("path required".into()));
        }

        std::fs::write(path, content)?;
        Ok(ToolOutcome::Ok {
            preview: format!("wrote {} bytes to {}", content.len(), path),
            spilled_path: None,
        })
    });

    registry.register("glob", |call| {
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
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let mut results = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_tool_executes_command() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut registry = ToolRegistry::new(&dir).unwrap();
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
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_file_tool_reads_file() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let mut registry = ToolRegistry::new(&dir).unwrap();
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
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_tool_writes_file() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("output.txt");

        let mut registry = ToolRegistry::new(&dir).unwrap();
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

        let mut registry = ToolRegistry::new(&dir).unwrap();
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
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_tool_returns_error() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let registry = ToolRegistry::new(&dir).unwrap();

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
        let mut registry = ToolRegistry::new(&dir).unwrap();
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
        let mut registry = ToolRegistry::new(&dir).unwrap();
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

        let mut registry = ToolRegistry::new(&dir).unwrap();
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
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn memory_remember_builtin_stores_record() {
        let dir = std::env::temp_dir().join(format!("darius_tools_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let memory = darius_memory::MemoryEngine::open(&dir).unwrap();

        let mut registry = ToolRegistry::new(&dir).unwrap();
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

        let mut registry = ToolRegistry::new(&dir).unwrap();
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
}
