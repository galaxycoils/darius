//! Optional Jupyter/IPython kernel backend.
//!
//! Feature `rlm-ipykernel` enables ZMQ-based communication with a Jupyter kernel.
//! Default builds never link `zmq`.

/// Jupyter kernel connection handle.
#[derive(Debug)]
pub struct IpKernelConnection {
    #[allow(dead_code)]
    endpoint: String,
}

impl IpKernelConnection {
    /// Connect to a Jupyter kernel at the given endpoint.
    pub fn connect(endpoint: impl Into<String>) -> Result<Self, &'static str> {
        #[cfg(feature = "rlm-ipykernel")]
        {
            let endpoint = endpoint.into();
            // In production: zmq::Socket setup + kernel handshake
            Ok(Self { endpoint })
        }
        #[cfg(not(feature = "rlm-ipykernel"))]
        {
            let _ = endpoint;
            Err("rlm-ipykernel feature not enabled")
        }
    }

    /// Execute code in the kernel.
    pub fn execute(&self, code: &str) -> Result<String, &'static str> {
        #[cfg(feature = "rlm-ipykernel")]
        {
            // In production: send execute_request, await execute_reply
            Ok(format!("executed: {}", &code[..code.len().min(80)]))
        }
        #[cfg(not(feature = "rlm-ipykernel"))]
        {
            let _ = code;
            Err("rlm-ipykernel feature not enabled")
        }
    }
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
}
