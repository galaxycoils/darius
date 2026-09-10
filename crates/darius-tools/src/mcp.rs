//! MCP (Model Context Protocol) thin client and server registry.

use crate::{ToolCall, ToolOutcome, ToolRegistry, ToolRisk};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
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
