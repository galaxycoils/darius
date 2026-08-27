//! LSP + DAP coding surface — diagnostics, rename, format-on-write, debugger.

use darius_hashline::{apply_put, compute_anchor, EditOp, FileAnchor, Filesystem, InMemoryFilesystem};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LspError {
    #[error("hashline error: {0}")]
    Hashline(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("filesystem error: {0}")]
    Filesystem(#[from] darius_hashline::FilesystemError),
    #[error("not found: {0}")]
    NotFound(String),
}

/// LSP message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspMessage {
    pub method: String,
    pub params: serde_json::Value,
    pub id: Option<u64>,
}

/// A diagnostic issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Rename request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameRequest {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub new_name: String,
}

/// Format-on-write request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatRequest {
    pub path: String,
    pub content: String,
}

/// LSP server — handles rename, diagnostics, format-on-write.
pub struct LspServer {
    filesystem: Arc<Mutex<InMemoryFilesystem>>,
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            filesystem: Arc::new(Mutex::new(InMemoryFilesystem::new())),
        }
    }

    /// Load a file into the filesystem.
    pub fn load_file(&self, path: &str, content: &str) -> Result<(), LspError> {
        self.filesystem.lock().write(path, content)?;
        Ok(())
    }

    /// Get file content.
    pub fn get_file(&self, path: &str) -> Result<String, LspError> {
        self.filesystem.lock().read(path).map_err(|e| LspError::NotFound(e.to_string()))
    }

    /// Run diagnostics on a file.
    pub fn diagnostics(&self, path: &str) -> Result<Vec<Diagnostic>, LspError> {
        let content = self.get_file(path)?;
        let mut diagnostics = Vec::new();

        // Basic diagnostics: check for trailing whitespace.
        for (i, line) in content.lines().enumerate() {
            if line.ends_with(' ') || line.ends_with('\t') {
                diagnostics.push(Diagnostic {
                    path: path.to_string(),
                    line: i as u32 + 1,
                    column: line.len() as u32,
                    severity: DiagnosticSeverity::Warning,
                    message: "trailing whitespace".to_string(),
                });
            }
        }

        Ok(diagnostics)
    }

    pub fn rename(&self, request: &RenameRequest) -> Result<(), LspError> {
        let content = self.get_file(&request.path)?;
        let anchor = FileAnchor {
            path: request.path.clone(),
            hash: compute_anchor(&content),
            line_count: content.lines().count(),
            ast_boundary: None,
        };

        let op = EditOp {
            anchor,
            put_lines: vec![request.new_name.clone()],
            cut_range: Some((request.line as usize - 1, request.line as usize)),
        };

        let mut fs = self.filesystem.lock();
        apply_put(&mut *fs, &op, request.line as usize - 1, request.line as usize)
            .map_err(|e| LspError::Hashline(e.to_string()))?;
        Ok(())
    }

    /// Format-on-write: normalize whitespace.
    pub fn format(&self, request: &FormatRequest) -> Result<String, LspError> {
        let formatted = request
            .content
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(formatted)
    }
}

/// DAP breakpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: u64,
    pub path: String,
    pub line: u32,
    pub verified: bool,
}

/// DAP stack frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub line: u32,
}

/// DAP debugger — breakpoints, step, frames.
pub struct DapDebugger {
    breakpoints: Arc<Mutex<HashMap<u64, Breakpoint>>>,
    next_id: Arc<Mutex<u64>>,
    paused: Arc<Mutex<bool>>,
    call_stack: Arc<Mutex<Vec<StackFrame>>>,
}

impl DapDebugger {
    pub fn new() -> Self {
        Self {
            breakpoints: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
            paused: Arc::new(Mutex::new(false)),
            call_stack: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set a breakpoint.
    pub fn set_breakpoint(&self, path: &str, line: u32) -> Breakpoint {
        let mut id = self.next_id.lock();
        let bp = Breakpoint {
            id: *id,
            path: path.to_string(),
            line,
            verified: true,
        };
        self.breakpoints.lock().insert(*id, bp.clone());
        *id += 1;
        bp
    }

    /// Remove a breakpoint.
    pub fn remove_breakpoint(&self, id: u64) -> bool {
        self.breakpoints.lock().remove(&id).is_some()
    }

    /// List all breakpoints.
    pub fn list_breakpoints(&self) -> Vec<Breakpoint> {
        self.breakpoints.lock().values().cloned().collect()
    }

    /// Pause execution.
    pub fn pause(&self) {
        *self.paused.lock() = true;
    }

    /// Resume execution.
    pub fn resume(&self) {
        *self.paused.lock() = false;
    }

    /// Check if paused.
    pub fn is_paused(&self) -> bool {
        *self.paused.lock()
    }

    /// Step to next line.
    pub fn step(&self) -> Option<StackFrame> {
        let mut stack = self.call_stack.lock();
        let frame = StackFrame {
            id: stack.len() as u64 + 1,
            name: "main".to_string(),
            path: "main.rs".to_string(),
            line: (stack.len() as u32) + 1,
        };
        stack.push(frame.clone());
        Some(frame)
    }

    /// Get call stack.
    pub fn call_stack(&self) -> Vec<StackFrame> {
        self.call_stack.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_load_and_get_file() {
        let server = LspServer::new();
        server.load_file("test.rs", "fn main() {}").unwrap();
        let content = server.get_file("test.rs").unwrap();
        assert_eq!(content, "fn main() {}");
    }

    #[test]
    fn lsp_diagnostics_detects_trailing_whitespace() {
        let server = LspServer::new();
        server.load_file("test.rs", "fn main() {} \nlet x = 1;").unwrap();
        let diagnostics = server.diagnostics("test.rs").unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Warning);
    }

    #[test]
    fn lsp_format_trims_trailing_whitespace() {
        let server = LspServer::new();
        let request = FormatRequest {
            path: "test.rs".to_string(),
            content: "fn main() {} \nlet x = 1;".to_string(),
        };
        let formatted = server.format(&request).unwrap();
        assert_eq!(formatted, "fn main() {}\nlet x = 1;");
    }

    #[test]
    fn dap_set_and_remove_breakpoint() {
        let debugger = DapDebugger::new();
        let bp = debugger.set_breakpoint("main.rs", 10);
        assert_eq!(bp.line, 10);
        assert!(bp.verified);

        let breakpoints = debugger.list_breakpoints();
        assert_eq!(breakpoints.len(), 1);

        assert!(debugger.remove_breakpoint(bp.id));
        assert_eq!(debugger.list_breakpoints().len(), 0);
    }

    #[test]
    fn dap_pause_resume() {
        let debugger = DapDebugger::new();
        assert!(!debugger.is_paused());
        debugger.pause();
        assert!(debugger.is_paused());
        debugger.resume();
        assert!(!debugger.is_paused());
    }

    #[test]
    fn dap_step_builds_call_stack() {
        let debugger = DapDebugger::new();
        let frame = debugger.step().unwrap();
        assert_eq!(frame.line, 1);
        assert_eq!(debugger.call_stack().len(), 1);
    }
}
