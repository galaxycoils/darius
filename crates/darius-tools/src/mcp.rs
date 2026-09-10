//! MCP (Model Context Protocol) thin client and server registry.

use crate::{ToolCall, ToolOutcome, ToolRegistry, ToolRisk};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum McpError {
    #[error("mcp server error: {0}")]
    Server(String),
    #[error("mcp transport error: {0}")]
    Transport(String),
    #[error("mcp timeout after {0:?}")]
    Timeout(Duration),
    #[error("mcp schema parse error: {0}")]
    Parse(String),
    #[error("step gate unsatisfied: tool '{0}' requires previous step success")]
    StepGateFailed(String),
}

/// Transport config for an MCP server (stdio or SSE).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum McpTransportConfig {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
    #[serde(rename = "sse")]
    Sse {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
}

/// Config entry for an MCP server, flattening transport options and metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerEntry {
    pub name: String,
    #[serde(flatten)]
    pub transport: McpTransportConfig,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Metadata for an MCP tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub requires_prior_success: bool,
}

/// Health status of an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unreachable(String),
}

/// MCP client trait for mocking and live transports.
pub trait McpClient: Send + Sync {
    fn ping(&self, timeout: Duration) -> Result<HealthStatus, McpError>;
    fn list_tools(&self) -> Result<Vec<McpToolDef>, McpError>;
    fn call_tool(&self, name: &str, arguments: &serde_json::Value)
    -> Result<ToolOutcome, McpError>;
}

/// In-memory mock / thin client for testing and local adapters.
pub struct LocalMcpClient {
    tools: Arc<Mutex<HashMap<String, McpToolDef>>>,
    healthy: Arc<Mutex<bool>>,
    prior_step_succeeded: Arc<Mutex<bool>>,
}

impl LocalMcpClient {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(Mutex::new(HashMap::new())),
            healthy: Arc::new(Mutex::new(true)),
            prior_step_succeeded: Arc::new(Mutex::new(true)),
        }
    }

    pub fn add_tool(&self, tool: McpToolDef) {
        self.tools.lock().insert(tool.name.clone(), tool);
    }

    pub fn set_healthy(&self, healthy: bool) {
        *self.healthy.lock() = healthy;
    }

    pub fn set_prior_step_succeeded(&self, ok: bool) {
        *self.prior_step_succeeded.lock() = ok;
    }
}

impl Default for LocalMcpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl McpClient for LocalMcpClient {
    fn ping(&self, timeout: Duration) -> Result<HealthStatus, McpError> {
        if timeout.as_secs() == 0 {
            return Err(McpError::Timeout(timeout));
        }
        if *self.healthy.lock() {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unreachable("server offline".into()))
        }
    }

    fn list_tools(&self) -> Result<Vec<McpToolDef>, McpError> {
        let tools = self.tools.lock().values().cloned().collect();
        Ok(tools)
    }

    fn call_tool(
        &self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolOutcome, McpError> {
        let tools = self.tools.lock();
        let def = tools
            .get(name)
            .ok_or_else(|| McpError::Server(format!("tool '{name}' not found on MCP server")))?;

        if def.requires_prior_success && !*self.prior_step_succeeded.lock() {
            return Err(McpError::StepGateFailed(name.to_string()));
        }

        Ok(ToolOutcome::Ok {
            preview: format!("MCP[{name}] called with args: {arguments}"),
            spilled_path: None,
        })
    }
}

struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    stdout_rx: Receiver<String>,
    next_id: u64,
}

impl StdioTransport {
    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, McpError> {
        self.next_id += 1;
        let id = self.next_id;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{req}")
            .map_err(|e| McpError::Transport(format!("failed to write to stdin: {e}")))?;
        self.stdin
            .flush()
            .map_err(|e| McpError::Transport(format!("failed to flush stdin: {e}")))?;

        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(McpError::Timeout(timeout));
            }
            match self.stdout_rx.recv_timeout(remaining) {
                Ok(line) => {
                    let val: serde_json::Value = serde_json::from_str(&line)
                        .map_err(|e| McpError::Parse(format!("invalid JSON from MCP stdout: {e}")))?;
                    if val.get("id").and_then(|v| v.as_u64()) == Some(id) {
                        if let Some(err) = val.get("error") {
                            let msg = err
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("unknown error");
                            return Err(McpError::Server(msg.to_string()));
                        }
                        if let Some(res) = val.get("result") {
                            return Ok(res.clone());
                        }
                        return Ok(val);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(McpError::Timeout(timeout));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(McpError::Transport(
                        "MCP child closed stdout unexpectedly".into(),
                    ));
                }
            }
        }
    }

    fn notify(&mut self, method: &str, params: serde_json::Value) -> Result<(), McpError> {
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{notif}")
            .map_err(|e| McpError::Transport(format!("failed to write notification: {e}")))?;
        self.stdin
            .flush()
            .map_err(|e| McpError::Transport(format!("failed to flush notification: {e}")))?;
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A real child-process stdio MCP client implementing `McpClient`.
pub struct StdioMcpClient {
    transport: Arc<Mutex<StdioTransport>>,
    spill_dir: PathBuf,
    default_timeout: Duration,
}

impl StdioMcpClient {
    pub fn connect(cfg: &McpTransportConfig, timeout: Duration) -> Result<Self, McpError> {
        let (command, args, env) = match cfg {
            McpTransportConfig::Stdio { command, args, env } => (command, args, env),
            McpTransportConfig::Sse { .. } => {
                return Err(McpError::Transport(
                    "SSE transport not supported for StdioMcpClient".into(),
                ));
            }
        };

        let mut cmd = Command::new(command);
        cmd.args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| McpError::Transport(format!("failed to spawn '{command}': {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Transport("stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Transport("stdout unavailable".into()))?;
        let stderr = child.stderr.take();

        let (tx, stdout_rx) = channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() && tx.send(trimmed).is_err() {
                    break;
                }
                line.clear();
            }
        });

        if let Some(stderr) = stderr {
            thread::spawn(move || {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    line.clear();
                }
            });
        }

        let mut transport = StdioTransport {
            child,
            stdin,
            stdout_rx,
            next_id: 0,
        };

        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "darius",
                "version": "1.4.0"
            }
        });
        let _ = transport.request("initialize", init_params, timeout)?;
        let _ = transport.notify("notifications/initialized", serde_json::json!({}));

        let spill_dir =
            std::env::temp_dir().join(format!("darius_mcp_spill_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&spill_dir);

        Ok(Self {
            transport: Arc::new(Mutex::new(transport)),
            spill_dir,
            default_timeout: timeout,
        })
    }

    pub fn with_spill_dir(mut self, spill_dir: PathBuf) -> Self {
        self.spill_dir = spill_dir;
        self
    }

    pub fn shutdown(&self) -> Result<(), McpError> {
        let mut t = self.transport.lock();
        let _ = t.child.kill();
        let _ = t.child.wait();
        Ok(())
    }

    pub fn ping(&self, timeout: Duration) -> Result<HealthStatus, McpError> {
        <Self as McpClient>::ping(self, timeout)
    }

    pub fn list_tools(&self) -> Result<Vec<McpToolDef>, McpError> {
        <Self as McpClient>::list_tools(self)
    }

    pub fn call_tool(
        &self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolOutcome, McpError> {
        <Self as McpClient>::call_tool(self, name, arguments)
    }
}

impl McpClient for StdioMcpClient {
    fn ping(&self, timeout: Duration) -> Result<HealthStatus, McpError> {
        if timeout.as_secs() == 0 && timeout.subsec_nanos() == 0 {
            return Err(McpError::Timeout(timeout));
        }
        let mut t = self.transport.lock();
        if let Ok(Some(_)) = t.child.try_wait() {
            return Ok(HealthStatus::Unreachable("child process exited".into()));
        }
        match t.request("ping", serde_json::json!({}), timeout) {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(McpError::Timeout(d)) => Err(McpError::Timeout(d)),
            Err(_) => {
                if let Ok(None) = t.child.try_wait() {
                    Ok(HealthStatus::Healthy)
                } else {
                    Ok(HealthStatus::Unreachable("server unreachable".into()))
                }
            }
        }
    }

    fn list_tools(&self) -> Result<Vec<McpToolDef>, McpError> {
        let mut t = self.transport.lock();
        let res = t.request("tools/list", serde_json::json!({}), self.default_timeout)?;
        let mut list = Vec::new();
        if let Some(tools) = res.get("tools").and_then(|t| t.as_array()) {
            for tool in tools {
                let name = tool
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let description = tool
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_schema = tool
                    .get("inputSchema")
                    .or_else(|| tool.get("input_schema"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object"}));
                let requires_prior_success = tool
                    .get("requires_prior_success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                list.push(McpToolDef {
                    name,
                    description,
                    input_schema,
                    requires_prior_success,
                });
            }
        }
        list.truncate(64);
        Ok(list)
    }

    fn call_tool(
        &self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolOutcome, McpError> {
        let mut t = self.transport.lock();
        let res = t.request(
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
            self.default_timeout,
        )?;
        let text = if let Some(arr) = res.get("content").and_then(|c| c.as_array()) {
            let mut parts = Vec::new();
            for block in arr {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    parts.push(t.to_string());
                } else {
                    parts.push(block.to_string());
                }
            }
            parts.join("\n")
        } else if let Some(t) = res.get("text").and_then(|v| v.as_str()) {
            t.to_string()
        } else {
            res.to_string()
        };

        let outcome = if text.len() > crate::spec::SPILL_CEILING {
            let _ = std::fs::create_dir_all(&self.spill_dir);
            let path = self
                .spill_dir
                .join(format!("tool_result_{}.txt", uuid::Uuid::new_v4()));
            let _ = std::fs::write(&path, &text);
            ToolOutcome::Ok {
                preview: crate::spec::truncate_preview(&text, crate::spec::SPILL_CEILING),
                spilled_path: Some(path),
            }
        } else {
            ToolOutcome::Ok {
                preview: text,
                spilled_path: None,
            }
        };

        Ok(outcome)
    }
}

/// Register discovered tools from an MCP client into the Darius ToolRegistry.
pub fn register_mcp_tools(
    registry: &mut ToolRegistry,
    client: Arc<dyn McpClient>,
) -> Result<usize, McpError> {
    let tools = client.list_tools()?;
    let count = tools.len();

    for tool in tools {
        let c = client.clone();
        let name = tool.name.clone();
        let tool_name = name.clone();

        registry.register_with_risk(&name, ToolRisk::Mutating, move |call: &ToolCall| {
            c.call_tool(&tool_name, &call.arguments)
                .map_err(|e| crate::ToolError::Execution(e.to_string()))
        });
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mcp_configs() {
        let stdio_json = r#"{
            "type": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-postgres"],
            "env": {"DATABASE_URL": "postgresql://localhost/db"}
        }"#;
        let stdio_cfg: McpTransportConfig = serde_json::from_str(stdio_json).unwrap();
        match stdio_cfg {
            McpTransportConfig::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args.len(), 2);
                assert_eq!(
                    env.get("DATABASE_URL").unwrap(),
                    "postgresql://localhost/db"
                );
            }
            _ => panic!("expected stdio config"),
        }

        let sse_json = r#"{
            "type": "sse",
            "url": "http://localhost:8080/sse",
            "headers": {"Authorization": "Bearer tok"}
        }"#;
        let sse_cfg: McpTransportConfig = serde_json::from_str(sse_json).unwrap();
        match sse_cfg {
            McpTransportConfig::Sse { url, headers } => {
                assert_eq!(url, "http://localhost:8080/sse");
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer tok");
            }
            _ => panic!("expected sse config"),
        }
    }

    #[test]
    fn mcp_health_check_ping_with_timeout() {
        let client = LocalMcpClient::new();
        assert_eq!(
            client.ping(Duration::from_secs(3)).unwrap(),
            HealthStatus::Healthy
        );

        client.set_healthy(false);
        assert_eq!(
            client.ping(Duration::from_secs(3)).unwrap(),
            HealthStatus::Unreachable("server offline".into())
        );

        // Zero timeout returns timeout error
        assert!(matches!(
            client.ping(Duration::from_secs(0)),
            Err(McpError::Timeout(_))
        ));
    }

    #[test]
    fn mcp_discover_register_and_execute() {
        let temp_dir =
            std::env::temp_dir().join(format!("darius_mcp_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let mut registry =
            ToolRegistry::new_with_roots(&temp_dir, &temp_dir.join("tool_results")).unwrap();

        let client = Arc::new(LocalMcpClient::new());
        client.add_tool(McpToolDef {
            name: "query_db".into(),
            description: "Query PostgreSQL database".into(),
            input_schema: serde_json::json!({"type": "object"}),
            requires_prior_success: false,
        });

        let count = register_mcp_tools(&mut registry, client.clone()).unwrap();
        assert_eq!(count, 1);

        let call = ToolCall {
            id: "mcp-call-1".into(),
            name: "query_db".into(),
            arguments: serde_json::json!({"sql": "SELECT 1"}),
        };

        let outcome = registry.execute(&call);
        match outcome {
            ToolOutcome::Ok { preview, .. } => {
                assert!(preview.contains("MCP[query_db]"));
                assert!(preview.contains("SELECT 1"));
            }
            ToolOutcome::Err { message } => panic!("mcp tool call failed: {message}"),
            ToolOutcome::Interrupted | ToolOutcome::TimedOut => {
                panic!("unexpected terminal outcome")
            }
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn mcp_step_gated_tool_enforcement() {
        let client = LocalMcpClient::new();
        client.add_tool(McpToolDef {
            name: "deploy_prod".into(),
            description: "Deploy artifact to production".into(),
            input_schema: serde_json::json!({}),
            requires_prior_success: true,
        });

        // 1. When prior step succeeded -> allowed
        client.set_prior_step_succeeded(true);
        let outcome = client.call_tool("deploy_prod", &serde_json::json!({}));
        assert!(outcome.is_ok());

        // 2. When prior step failed -> denied with StepGateFailed
        client.set_prior_step_succeeded(false);
        let outcome = client.call_tool("deploy_prod", &serde_json::json!({}));
        assert!(matches!(outcome, Err(McpError::StepGateFailed(_))));
    }
}
