//! CLI event system for session tracking and Continual Harness.

use std::fs;
use std::path::PathBuf;
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
pub fn log_event(base_dir: &str, session_id: &str, event: &SessionEvent) -> Result<(), String> {
    let events_dir = get_events_dir(base_dir, session_id)?;
    let event_file = events_dir.join(format!("{}.log", event.timestamp));
    
    let event_type_str = match &event.event_type {
        EventType::Started => "started".to_string(),
        EventType::Stopped => "stopped".to_string(),
        EventType::Status => "status".to_string(),
        EventType::Error(e) => format!("error:{e}"),
        EventType::Message(m) => format!("message:{m}"),
    };
    
    let content = format!(
        "{}|{}|{}\n",
        event.timestamp,
        event_type_str,
        event.data
    );
    
    fs::write(&event_file, content).map_err(|e| e.to_string())?;
    Ok(())
}

/// Get the events directory for a session.
fn get_events_dir(base_dir: &str, session_id: &str) -> Result<PathBuf, String> {
    let session_dir = PathBuf::from(base_dir).join("sessions").join(session_id);
    fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;
    Ok(session_dir)
}

/// Get current timestamp in seconds since epoch.
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
