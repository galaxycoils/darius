use darius_tools::{HealthStatus, McpError, McpTransportConfig, StdioMcpClient, ToolOutcome};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

fn mock_mcp_config() -> McpTransportConfig {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mock_mcp.py")
        .canonicalize()
        .expect("mock_mcp.py must exist");

    McpTransportConfig::Stdio {
        command: "python3".into(),
        args: vec![script.to_string_lossy().to_string()],
        env: HashMap::new(),
    }
}

#[test]
fn stdio_mcp_client_connect_and_list_tools() {
    let cfg = mock_mcp_config();
    let client = StdioMcpClient::connect(&cfg, Duration::from_secs(5)).expect("connect failed");

    let tools = client.list_tools().expect("list_tools failed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[0].description, "Echo back input text");

    let ping = client.ping(Duration::from_secs(2)).expect("ping failed");
    assert_eq!(ping, HealthStatus::Healthy);

    client.shutdown().expect("shutdown failed");
}

#[test]
fn stdio_mcp_client_call_tool_echo_success() {
    let cfg = mock_mcp_config();
    let client = StdioMcpClient::connect(&cfg, Duration::from_secs(5)).expect("connect failed");

    let args = serde_json::json!({"text": "HELLO_MCP_WORLD"});
    let outcome = client.call_tool("echo", &args).expect("call_tool failed");

    match outcome {
        ToolOutcome::Ok {
            preview,
            spilled_path,
        } => {
            assert!(preview.contains("HELLO_MCP_WORLD"));
            assert!(spilled_path.is_none());
        }
        other => panic!("expected ToolOutcome::Ok, got {other:?}"),
    }

    client.shutdown().expect("shutdown failed");
}

#[test]
fn stdio_mcp_client_call_unknown_tool_returns_server_error() {
    let cfg = mock_mcp_config();
    let client = StdioMcpClient::connect(&cfg, Duration::from_secs(5)).expect("connect failed");

    let err = client
        .call_tool("nonexistent_tool", &serde_json::json!({}))
        .unwrap_err();

    match err {
        McpError::Server(msg) => {
            assert!(
                msg.contains("tool 'nonexistent_tool' not found on MCP server"),
                "error message was: {msg}"
            );
        }
        other => panic!("expected McpError::Server, got {other:?}"),
    }

    client.shutdown().expect("shutdown failed");
}

#[test]
fn stdio_mcp_client_spill_large_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cfg = mock_mcp_config();
    let client = StdioMcpClient::connect(&cfg, Duration::from_secs(5))
        .expect("connect failed")
        .with_spill_dir(temp_dir.path().to_path_buf());

    // Generate > 32 KiB payload for echo
    let large_text = "x".repeat(35 * 1024);
    let outcome = client
        .call_tool("echo", &serde_json::json!({"text": large_text}))
        .expect("call_tool failed");

    match outcome {
        ToolOutcome::Ok {
            preview,
            spilled_path,
        } => {
            assert!(preview.len() <= 32 * 1024);
            assert!(spilled_path.is_some(), "expected large output to spill");
            let path = spilled_path.unwrap();
            assert!(path.exists());
            let on_disk = std::fs::read_to_string(path).unwrap();
            assert_eq!(on_disk.len(), 35 * 1024);
        }
        other => panic!("expected ToolOutcome::Ok with spill, got {other:?}"),
    }

    client.shutdown().expect("shutdown failed");
}
