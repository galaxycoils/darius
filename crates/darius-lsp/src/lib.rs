//! Darius LSP + DAP coding surface — diagnostics, rename, format-on-write, debugger.

pub struct LspMessage {
    pub method: String,
    pub params: serde_json::Value,
    pub id: Option<u64>,
}

pub struct LspClient;

pub struct LspServer {
    running: bool,
}

impl LspServer {
    pub fn new() -> Self { Self { running: false } }
    pub fn start(&mut self) { self.running = true; }
    pub fn is_running(&self) -> bool { self.running }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_starts() {
        let mut srv = LspServer::new();
        assert!(!srv.is_running());
        srv.start();
        assert!(srv.is_running());
    }
}
