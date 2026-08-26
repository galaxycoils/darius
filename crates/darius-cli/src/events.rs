//! CLI event system for session tracking and Continual Harness.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Session event tracking.
pub struct SessionEvent {
    pub session_id: String,
    pub timestamp: u64,
    pub event_type: EventType,
    pub data: String,
}

#[derive(Debug, Clone)]
pub enum EventType {
    Started,
    Stopped,
    Status,
    Error(String),
    Message(String),
}

/// Write a session event to the event log.
pub fn log_event(session_id: &str, event: &SessionEvent) -> Result<(), String> {
    let events_dir = get_events_dir(session_id)?;
    let event_file = events_dir.join(format!("{}.log", event.timestamp));
    
    let content = format!(
        "{timestamp}|{event_type}|{data}\n",
        timestamp = event.timestamp,
        event_type = match &event.event_type {
            EventType::Started => "started",
            EventType::Stopped => "stopped",
            EventType::Status => "status",
            EventType::Error(e) => format!("error:{e}"),
            EventType::Message(m) => format!("message:{m}"),
        },
        data = event.data
    );
    
    fs::write(&event_file, content).map_err(|e| e.to_string())?;
    Ok(())
}

/// Get the events directory for a session.
fn get_events_dir(session_id: &str) -> Result<PathBuf, String> {
    let base_dir = dirs::data_local_dir()
        .ok_or_else(|| "Could not find local data directory".to_string())?;
    let session_dir = base_dir.join("darius").join("sessions").join(session_id);
    fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;
    Ok(session_dir)
}

/// Get current timestamp in seconds since epoch.
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
