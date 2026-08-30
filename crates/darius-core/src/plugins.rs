//! Extension Points — plugin trait, registry, hooks, and MCP server discovery.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Plugin metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

/// Plugin capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilities {
    pub tools: Vec<String>,
    pub hooks: Vec<String>,
}

/// Sandbox policy for plugins.
#[derive(Debug, Clone)]
pub enum SandboxPolicy {
    Native { allow_list: Vec<String> },
    Wasm { limits: WasmLimits },
    Python { tier: super::IsolationTier },
}

#[derive(Debug, Clone)]
pub struct WasmLimits {
    pub memory_bytes: u64,
    pub cpu_time_ms: u64,
    pub denied_apis: Vec<String>,
}

/// Hook types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookType {
    PreToolCall,
    PostToolCall,
    PreSessionStart,
    PostSessionEnd,
    OnError,
}

/// Hook set — maps hook types to handler names.
#[derive(Debug, Clone, Default)]
pub struct HookSet {
    pub hooks: HashMap<HookType, Vec<String>>,
}

impl HookSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_hook(&mut self, hook_type: HookType, handler: String) {
        self.hooks.entry(hook_type).or_default().push(handler);
    }

    pub fn get_handlers(&self, hook_type: &HookType) -> &[String] {
        self.hooks
            .get(hook_type)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Plugin trait — implemented by all plugins.
pub trait Plugin: Send + Sync {
    /// Get plugin metadata.
    fn metadata(&self) -> PluginMetadata;

    /// Get plugin capabilities.
    fn capabilities(&self) -> PluginCapabilities;

    /// Initialize the plugin.
    fn init(&mut self) -> Result<(), String>;

    /// Get the hook set for this plugin.
    fn hooks(&self) -> HookSet;

    /// Shutdown the plugin.
    fn shutdown(&self) -> Result<(), String>;
}

/// Plugin context — passed to plugins during initialization.
#[derive(Debug, Clone)]
pub struct PluginContext {
    pub data_dir: String,
    pub config: HashMap<String, String>,
}

/// Plugin registry — manages plugin lifecycle.
pub struct PluginRegistry {
    plugins: Arc<Mutex<HashMap<String, Box<dyn Plugin>>>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a plugin.
    pub fn register(&self, plugin: Box<dyn Plugin>) -> Result<(), String> {
        let mut plugins = self.plugins.lock();
        let name = plugin.metadata().name.clone();
        if plugins.contains_key(&name) {
            return Err(format!("plugin {name} already registered"));
        }
        plugins.insert(name, plugin);
        Ok(())
    }

    /// Get a plugin by name.
    pub fn get(&self, _name: &str) -> Option<&dyn Plugin> {
        // This is a limitation of the trait object — we can't return a reference
        // from a MutexGuard. In a real implementation, we'd use Arc<dyn Plugin>.
        None
    }

    /// List all registered plugins.
    pub fn list(&self) -> Vec<PluginMetadata> {
        self.plugins.lock().values().map(|p| p.metadata()).collect()
    }

    /// Initialize all plugins.
    pub fn init_all(&self) -> Result<(), String> {
        let mut plugins = self.plugins.lock();
        for plugin in plugins.values_mut() {
            plugin.init()?;
        }
        Ok(())
    }

    /// Shutdown all plugins.
    pub fn shutdown_all(&self) -> Result<(), String> {
        let plugins = self.plugins.lock();
        for plugin in plugins.values() {
            plugin.shutdown()?;
        }
        Ok(())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP server plugin — wraps an MCP server as a plugin.
pub struct McpServerPlugin {
    metadata: PluginMetadata,
    capabilities: PluginCapabilities,
    #[allow(dead_code)]
    server_url: String,
}

impl McpServerPlugin {
    pub fn new(name: impl Into<String>, server_url: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            metadata: PluginMetadata {
                name: name.clone(),
                version: "0.1.0".into(),
                description: format!("MCP server plugin: {name}"),
                author: "darius".into(),
            },
            capabilities: PluginCapabilities {
                tools: vec![format!("mcp_{}_*", name)],
                hooks: vec![],
            },
            server_url: server_url.into(),
        }
    }
}

impl Plugin for McpServerPlugin {
    fn metadata(&self) -> PluginMetadata {
        self.metadata.clone()
    }

    fn capabilities(&self) -> PluginCapabilities {
        self.capabilities.clone()
    }

    fn init(&mut self) -> Result<(), String> {
        // In a real implementation, this would connect to the MCP server.
        Ok(())
    }

    fn hooks(&self) -> HookSet {
        HookSet::new()
    }

    fn shutdown(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Discover MCP servers and wrap them as plugins.
pub fn discover_mcp_servers(_config_dir: &str) -> Vec<McpServerPlugin> {
    // Stub: in a real implementation, this would scan for MCP server configs.
    vec![
        McpServerPlugin::new("filesystem", "http://localhost:3001"),
        McpServerPlugin::new("browser", "http://localhost:3002"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_registry_register_and_list() {
        let registry = PluginRegistry::new();
        let plugin = McpServerPlugin::new("test", "http://localhost:3000");
        registry.register(Box::new(plugin)).unwrap();

        let plugins = registry.list();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "test");
    }

    #[test]
    fn plugin_registry_duplicate_fails() {
        let registry = PluginRegistry::new();
        let plugin1 = McpServerPlugin::new("test", "http://localhost:3000");
        let plugin2 = McpServerPlugin::new("test", "http://localhost:3001");

        registry.register(Box::new(plugin1)).unwrap();
        assert!(registry.register(Box::new(plugin2)).is_err());
    }

    #[test]
    fn mcp_server_plugin_metadata() {
        let plugin = McpServerPlugin::new("my_server", "http://localhost:3000");
        let metadata = plugin.metadata();
        assert_eq!(metadata.name, "my_server");
        assert!(metadata.description.contains("MCP server"));
    }

    #[test]
    fn discover_mcp_servers_returns_plugins() {
        let servers = discover_mcp_servers("/tmp/config");
        assert!(!servers.is_empty());
    }

    #[test]
    fn hook_set_add_and_get() {
        let mut hooks = HookSet::new();
        hooks.add_hook(HookType::PreToolCall, "validator".into());
        hooks.add_hook(HookType::PostToolCall, "logger".into());

        assert_eq!(hooks.get_handlers(&HookType::PreToolCall).len(), 1);
        assert_eq!(hooks.get_handlers(&HookType::PostToolCall).len(), 1);
        assert_eq!(hooks.get_handlers(&HookType::OnError).len(), 0);
    }
}
