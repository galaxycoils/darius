//! Platform Adapters — thin clients to A2A for messaging surfaces.

use crate::a2a::A2aServer;
use parking_lot::Mutex;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("send failed: {0}")]
    SendFailed(String),
    #[error("platform error: {0}")]
    Platform(String),
}

/// Base trait for platform adapters.
pub trait PlatformAdapter: Send + Sync {
    /// Get the platform name.
    fn platform_name(&self) -> &str;

    /// Connect to the platform.
    fn connect(&mut self) -> Result<(), AdapterError>;

    /// Disconnect from the platform.
    fn disconnect(&mut self) -> Result<(), AdapterError>;

    /// Send a message to a channel/user.
    fn send_message(&self, channel: &str, message: &str) -> Result<(), AdapterError>;

    /// Receive messages (poll-based).
    fn receive_messages(&self) -> Result<Vec<IncomingMessage>, AdapterError>;

    /// Check if connected.
    fn is_connected(&self) -> bool;
}

/// An incoming message from any platform.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub platform: String,
    pub channel_id: String,
    pub user_id: String,
    pub username: String,
    pub content: String,
    pub timestamp: u64,
}

/// Telegram adapter.
pub struct TelegramAdapter {
    #[allow(dead_code)]
    bot_token: String,
    #[allow(dead_code)]
    api_url: String,
    connected: bool,
    a2a_server: Option<Arc<A2aServer>>,
}

impl TelegramAdapter {
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            api_url: "https://api.telegram.org".into(),
            connected: false,
            a2a_server: None,
        }
    }

    pub fn with_a2a_server(mut self, server: Arc<A2aServer>) -> Self {
        self.a2a_server = Some(server);
        self
    }
}

impl PlatformAdapter for TelegramAdapter {
    fn platform_name(&self) -> &str {
        "telegram"
    }

    fn connect(&mut self) -> Result<(), AdapterError> {
        // Stub: would validate bot token and establish webhook/long-poll.
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), AdapterError> {
        self.connected = false;
        Ok(())
    }

    fn send_message(&self, _channel: &str, _message: &str) -> Result<(), AdapterError> {
        if !self.connected {
            return Err(AdapterError::SendFailed("not connected".into()));
        }
        // Stub: would call Telegram Bot API.
        Ok(())
    }

    fn receive_messages(&self) -> Result<Vec<IncomingMessage>, AdapterError> {
        if !self.connected {
            return Err(AdapterError::Platform("not connected".into()));
        }
        // Stub: would poll Telegram Bot API for updates.
        Ok(Vec::new())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

/// Discord adapter.
pub struct DiscordAdapter {
    #[allow(dead_code)]
    bot_token: String,
    #[allow(dead_code)]
    api_url: String,
    connected: bool,
    a2a_server: Option<Arc<A2aServer>>,
}

impl DiscordAdapter {
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            api_url: "https://discord.com/api/v10".into(),
            connected: false,
            a2a_server: None,
        }
    }

    pub fn with_a2a_server(mut self, server: Arc<A2aServer>) -> Self {
        self.a2a_server = Some(server);
        self
    }
}

impl PlatformAdapter for DiscordAdapter {
    fn platform_name(&self) -> &str {
        "discord"
    }

    fn connect(&mut self) -> Result<(), AdapterError> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), AdapterError> {
        self.connected = false;
        Ok(())
    }

    fn send_message(&self, _channel: &str, _message: &str) -> Result<(), AdapterError> {
        if !self.connected {
            return Err(AdapterError::SendFailed("not connected".into()));
        }
        // Stub: would call Discord API.
        Ok(())
    }

    fn receive_messages(&self) -> Result<Vec<IncomingMessage>, AdapterError> {
        if !self.connected {
            return Err(AdapterError::Platform("not connected".into()));
        }
        Ok(Vec::new())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

/// Slack adapter.
pub struct SlackAdapter {
    #[allow(dead_code)]
    bot_token: String,
    #[allow(dead_code)]
    api_url: String,
    connected: bool,
    a2a_server: Option<Arc<A2aServer>>,
}

impl SlackAdapter {
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            api_url: "https://slack.com/api".into(),
            connected: false,
            a2a_server: None,
        }
    }

    pub fn with_a2a_server(mut self, server: Arc<A2aServer>) -> Self {
        self.a2a_server = Some(server);
        self
    }
}

impl PlatformAdapter for SlackAdapter {
    fn platform_name(&self) -> &str {
        "slack"
    }

    fn connect(&mut self) -> Result<(), AdapterError> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), AdapterError> {
        self.connected = false;
        Ok(())
    }

    fn send_message(&self, _channel: &str, _message: &str) -> Result<(), AdapterError> {
        if !self.connected {
            return Err(AdapterError::SendFailed("not connected".into()));
        }
        Ok(())
    }

    fn receive_messages(&self) -> Result<Vec<IncomingMessage>, AdapterError> {
        if !self.connected {
            return Err(AdapterError::Platform("not connected".into()));
        }
        Ok(Vec::new())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

/// Platform adapter manager — manages all adapters.
pub struct AdapterManager {
    adapters: Arc<Mutex<Vec<Box<dyn PlatformAdapter>>>>,
}

impl AdapterManager {
    pub fn new() -> Self {
        Self {
            adapters: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a new adapter.
    pub fn register(&self, adapter: Box<dyn PlatformAdapter>) {
        self.adapters.lock().push(adapter);
    }

    /// Connect all adapters.
    pub fn connect_all(&self) -> Vec<Result<(), AdapterError>> {
        let mut adapters = self.adapters.lock();
        adapters.iter_mut().map(|a| a.connect()).collect()
    }

    /// Disconnect all adapters.
    pub fn disconnect_all(&self) -> Vec<Result<(), AdapterError>> {
        let mut adapters = self.adapters.lock();
        adapters.iter_mut().map(|a| a.disconnect()).collect()
    }

    /// Get adapter by platform name.
    pub fn get_adapter(&self, _platform: &str) -> Option<&dyn PlatformAdapter> {
        // Stub: would search adapters by platform name.
        None
    }

    /// List all adapters.
    pub fn list_adapters(&self) -> Vec<&dyn PlatformAdapter> {
        // Stub: would return all adapters.
        Vec::new()
    }
}

impl Default for AdapterManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telegram_adapter_connect_disconnect() {
        let mut adapter = TelegramAdapter::new("test_token");
        assert!(!adapter.is_connected());
        adapter.connect().unwrap();
        assert!(adapter.is_connected());
        adapter.disconnect().unwrap();
        assert!(!adapter.is_connected());
    }

    #[test]
    fn telegram_adapter_send_when_disconnected_fails() {
        let adapter = TelegramAdapter::new("test_token");
        assert!(adapter.send_message("channel", "hello").is_err());
    }

    #[test]
    fn telegram_adapter_send_when_connected_succeeds() {
        let mut adapter = TelegramAdapter::new("test_token");
        adapter.connect().unwrap();
        assert!(adapter.send_message("channel", "hello").is_ok());
    }

    #[test]
    fn discord_adapter_platform_name() {
        let adapter = DiscordAdapter::new("test_token");
        assert_eq!(adapter.platform_name(), "discord");
    }

    #[test]
    fn slack_adapter_platform_name() {
        let adapter = SlackAdapter::new("test_token");
        assert_eq!(adapter.platform_name(), "slack");
    }

    #[test]
    fn telegram_adapter_receive_messages() {
        let mut adapter = TelegramAdapter::new("test_token");
        adapter.connect().unwrap();
        let messages = adapter.receive_messages().unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn incoming_message_fields() {
        let msg = IncomingMessage {
            platform: "telegram".into(),
            channel_id: "123".into(),
            user_id: "456".into(),
            username: "testuser".into(),
            content: "hello".into(),
            timestamp: 1234567890,
        };
        assert_eq!(msg.platform, "telegram");
        assert_eq!(msg.content, "hello");
    }
}
