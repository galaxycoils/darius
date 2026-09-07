//! Session-only approvals; canonical identity comes from the tool path authority.
use darius_tools::{PathPolicy, ToolCall};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
pub type SessionPermissions = Arc<Mutex<HashSet<(String, String)>>>;
/// Headless runs never wait for stdin or silently authorize a mutation.
#[derive(Default)]
pub struct HeadlessRunControl(pub std::sync::atomic::AtomicBool);
impl darius_cognitive::RunControl for HeadlessRunControl {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn approve_tool(
        &self,
        _: &ToolCall,
        _: darius_tools::ToolRisk,
    ) -> Result<darius_cognitive::PermissionChoice, darius_cognitive::CognitiveError> {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(darius_cognitive::PermissionChoice::Deny)
    }
}
pub fn key(call: &ToolCall, policy: &PathPolicy) -> Option<(String, String)> {
    let key = darius_tools::session_keys::allow_session_key(call, policy.root(), policy)?;
    Some((
        call.name.clone(),
        key.strip_prefix(&format!("{}:", call.name))?.into(),
    ))
}
