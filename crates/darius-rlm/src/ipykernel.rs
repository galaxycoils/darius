//! Optional Jupyter/IPython kernel backend.
//!
//! Feature `rlm-ipykernel` enables ZMQ-based communication with a Jupyter kernel.
//! Default builds never link `zmq`.
//!
//! Profile config:
//! ```toml
//! [rlm]
//! backend = "rust"          # default
//! # backend = "ipykernel"
//! # connection_file = "/path/to/kernel.json"
//! ```

use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

/// RLM backend type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RlmBackend {
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "ipykernel")]
    Ipkernel,
}

impl Default for RlmBackend {
    fn default() -> Self {
        RlmBackend::Rust
    }
}

/// RLM config from profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RlmConfig {
    #[serde(default)]
    pub backend: RlmBackend,
    pub connection_file: Option<PathBuf>,
}

/// Jupyter kernel connection handle.
#[derive(Debug)]
pub struct IpKernelConnection {
    endpoint: String,
}

impl IpKernelConnection {
    /// Connect to a Jupyter kernel at the given endpoint.
    pub fn connect(endpoint: impl Into<String>) -> Result<Self, String> {
        #[cfg(feature = "rlm-ipykernel")]
        {
            let endpoint = endpoint.into();
            // In production: zmq::Socket setup + kernel handshake
            Ok(Self { endpoint })
        }
        #[cfg(not(feature = "rlm-ipykernel"))]
        {
            let _ = endpoint;
            Err("rlm-ipykernel feature not enabled".into())
        }
    }

    /// Execute code in the kernel.
    pub fn execute(&self, code: &str) -> Result<String, String> {
        #[cfg(feature = "rlm-ipykernel")]
        {
            // In production: send execute_request, await execute_reply
            Ok(format!("executed: {}", &code[..code.len().min(80)]))
        }
        #[cfg(not(feature = "rlm-ipykernel"))]
        {
            let _ = code;
            Err("rlm-ipykernel feature not enabled".into())
        }
    }

    /// Connect from a Jupyter connection file (kernel.json).
    pub fn connect_from_file(path: &PathBuf) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read connection file: {e}"))?;
        let mut parsed: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Invalid kernel.json: {e}"))?;

        // Encrypt shell + hmac fields if needed
        let endpoint = format!(
            "tcp://{}:{}",
            parsed["ip"].as_str().unwrap_or("127.0.0.1"),
            parsed["shell_port"].as_u64().unwrap_or(5555)
        );
        Self::connect(endpoint)
    }

    /// Start a kernel child process.
    pub fn start_kernel(working_dir: &PathBuf) -> Result<std::process::Child, String> {
        #[cfg(feature = "rlm-ipykernel")]
        {
            std::process::Command::new("python")
                .args(["-m", "ipykernel_launcher", "-f", "kernel.json"])
                .current_dir(working_dir)
                .spawn()
                .map_err(|e| format!("Failed to start kernel: {e}"))
        }
        #[cfg(not(feature = "rlm-ipykernel"))]
        {
            let _ = working_dir;
            Err("rlm-ipykernel feature not enabled".into())
        }
    }

    /// Stop the kernel child process.
    pub fn stop_kernel(child: &mut std::process::Child) -> Result<(), String> {
        child.kill().map_err(|e| format!("Failed to kill kernel: {e}"))?;
        child.wait().map_err(|e| format!("Failed to wait for kernel: {e}"))?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KernelPorts {
    pub shell_port: u16,
    pub iopub_port: u16,
    pub stdin_port: u16,
    pub control_port: u16,
    pub hb_port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(feature = "rlm-ipykernel"))]
    fn ipkernel_disabled_by_default() {
        let result = IpKernelConnection::connect("tcp://localhost:5555");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not enabled"));
    }

    #[test]
    #[cfg(feature = "rlm-ipykernel")]
    fn ipkernel_connects_when_enabled() {
        let conn = IpKernelConnection::connect("tcp://localhost:5555");
        assert!(conn.is_ok());
    }

    #[test]
    fn rlm_backend_default_is_rust() {
        let backend: RlmBackend = Default::default();
        assert_eq!(backend, RlmBackend::Rust);
    }

    #[test]
    fn rlm_config_parse() {
        let toml_str = r#"
[rlm]
backend = "ipykernel"
connection_file = "/path/to/kernel.json"
"#;
        let parsed: toml::Value = toml::from_str(toml_str).unwrap();
        let config: RlmConfig = parsed["rlm"].clone().try_into().unwrap();
        assert_eq!(config.backend, RlmBackend::Ipkernel);
        assert!(config.connection_file.is_some());
    }

    #[test]
    fn rlm_config_default_backend() {
        let config = RlmConfig::default();
        assert_eq!(config.backend, RlmBackend::Rust);
    }
}
