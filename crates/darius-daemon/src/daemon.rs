//! Darius daemon — session manager, A2A hub, and service orchestrator.

use crate::a2a::{A2aServer, AgentCard};
use crate::event_log::EventLog;
use crate::handoff::HandoffStore;
use darius_core::SessionHandoff;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;

/// Daemon error types.
#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("daemon already running")]
    AlreadyRunning,
    #[error("daemon not running")]
    NotRunning,
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("event log error: {0}")]
    EventLog(String),
    #[error("handoff error: {0}")]
    Handoff(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Session state tracked by the daemon.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub profile: String,
    pub goal: String,
    pub running: bool,
    pub created_at: u64,
    pub last_activity: u64,
}

/// Daemon status information.
#[derive(Debug, Clone)]
pub struct DaemonStatus {
    pub running: bool,
    pub uptime_secs: u64,
    pub active_sessions: usize,
    pub total_sessions: usize,
    pub version: String,
}

/// Darius daemon — owns RLM lifecycle, sessions, A2A server, event log, handoff store.
pub struct Daemon {
    running: Arc<AtomicBool>,
    start_time: Option<std::time::Instant>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    event_log: Arc<Mutex<Option<EventLog>>>,
    handoff_store: Arc<Mutex<Option<HandoffStore>>>,
    a2a_server: Arc<A2aServer>,
    data_dir: PathBuf,
    #[allow(dead_code)]
    heartbeat_interval: Duration,
}

impl Daemon {
    /// Create a new daemon instance.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let card = AgentCard::new(
            "darius",
            env!("CARGO_PKG_VERSION"),
            "Open-source agent harness",
        )
        .with_capabilities(vec![
            "rlm".to_string(),
            "hashline".to_string(),
            "sessions".to_string(),
        ]);
        Self {
            running: Arc::new(AtomicBool::new(false)),
            start_time: None,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            event_log: Arc::new(Mutex::new(None)),
            handoff_store: Arc::new(Mutex::new(None)),
            a2a_server: Arc::new(A2aServer::new(card)),
            data_dir: data_dir.into(),
            heartbeat_interval: Duration::from_secs(30),
        }
    }

    /// Start the daemon.
    pub fn start(&mut self) -> Result<(), DaemonError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(DaemonError::AlreadyRunning);
        }

        // Initialize event log.
        let event_log_path = self.data_dir.join("events.db");
        let event_log =
            EventLog::open(&event_log_path).map_err(|e| DaemonError::EventLog(e.to_string()))?;
        *self.event_log.lock() = Some(event_log);

        // Initialize handoff store.
        let handoff_dir = self.data_dir.join("handoffs");
        let handoff_store =
            HandoffStore::new(&handoff_dir).map_err(|e| DaemonError::Handoff(e.to_string()))?;
        *self.handoff_store.lock() = Some(handoff_store);

        self.running.store(true, Ordering::SeqCst);
        self.start_time = Some(std::time::Instant::now());

        // Log daemon start.
        if let Some(log) = self.event_log.lock().as_ref() {
            let _ = log.append(
                "daemon",
                "started",
                &format!("version={}", env!("CARGO_PKG_VERSION")),
            );
        }

        Ok(())
    }

    /// Stop the daemon.
    pub fn stop(&mut self) -> Result<(), DaemonError> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(DaemonError::NotRunning);
        }

        // Emit handoffs for all active sessions.
        let sessions = self.sessions.lock().clone();
        for (id, session) in sessions.iter().filter(|(_, s)| s.running) {
            let _ = self.emit_handoff(id, &session.goal);
        }

        // Log daemon stop.
        if let Some(log) = self.event_log.lock().as_ref() {
            let _ = log.append("daemon", "stopped", "");
        }

        self.running.store(false, Ordering::SeqCst);
        self.start_time = None;
        Ok(())
    }

    /// Check if the daemon is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get daemon status.
    pub fn status(&self) -> DaemonStatus {
        let sessions = self.sessions.lock();
        let active = sessions.values().filter(|s| s.running).count();
        DaemonStatus {
            running: self.is_running(),
            uptime_secs: self.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0),
            active_sessions: active,
            total_sessions: sessions.len(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Create a new session.
    pub fn create_session(
        &self,
        profile: impl Into<String>,
        goal: impl Into<String>,
    ) -> Result<Session, DaemonError> {
        if !self.is_running() {
            return Err(DaemonError::NotRunning);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let ts = current_timestamp();
        let session = Session {
            id: id.clone(),
            profile: profile.into(),
            goal: goal.into(),
            running: true,
            created_at: ts,
            last_activity: ts,
        };

        self.sessions.lock().insert(id.clone(), session.clone());

        // Log session start.
        if let Some(log) = self.event_log.lock().as_ref() {
            let _ = log.append(
                &id,
                "session_started",
                &format!("profile={}", session.profile),
            );
        }

        // Create A2A task for the session.
        self.a2a_server.create_task(&id, &session.goal);

        Ok(session)
    }

    /// Get a session by ID.
    pub fn get_session(&self, id: &str) -> Result<Session, DaemonError> {
        self.sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| DaemonError::SessionNotFound(id.to_string()))
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<Session> {
        self.sessions.lock().values().cloned().collect()
    }

    /// Attach to a session (marks it as active).
    pub fn attach_session(&self, id: &str) -> Result<(), DaemonError> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| DaemonError::SessionNotFound(id.to_string()))?;
        session.running = true;
        session.last_activity = current_timestamp();

        // Log attach.
        if let Some(log) = self.event_log.lock().as_ref() {
            let _ = log.append(id, "session_attached", "");
        }

        Ok(())
    }

    /// Detach from a session (marks it as inactive but preserves state).
    pub fn detach_session(&self, id: &str) -> Result<(), DaemonError> {
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| DaemonError::SessionNotFound(id.to_string()))?;
        session.running = false;
        session.last_activity = current_timestamp();

        // Log detach.
        if let Some(log) = self.event_log.lock().as_ref() {
            let _ = log.append(id, "session_detached", "");
        }

        Ok(())
    }

    /// End a session — emits handoff and cleans up.
    pub fn end_session(&self, id: &str) -> Result<(), DaemonError> {
        let goal = {
            let mut sessions = self.sessions.lock();
            let session = sessions
                .get_mut(id)
                .ok_or_else(|| DaemonError::SessionNotFound(id.to_string()))?;
            session.running = false;
            session.last_activity = current_timestamp();
            session.goal.clone()
        };

        // Emit handoff.
        self.emit_handoff(id, &goal)?;

        // Log session end.
        if let Some(log) = self.event_log.lock().as_ref() {
            let _ = log.append(id, "session_ended", "");
        }

        Ok(())
    }

    /// Emit a SessionHandoff for a session.
    fn emit_handoff(&self, session_id: &str, goal: &str) -> Result<(), DaemonError> {
        let store = self.handoff_store.lock();
        let store = store
            .as_ref()
            .ok_or_else(|| DaemonError::Handoff("handoff store not initialized".to_string()))?;

        let handoff = SessionHandoff {
            version: 1,
            goal: goal.to_string(),
            prior_decisions: Vec::new(),
            open_questions: Vec::new(),
            constraints: Vec::new(),
            artifact_refs: Vec::new(),
        };

        store
            .save(session_id, &handoff)
            .map_err(|e| DaemonError::Handoff(e.to_string()))
    }

    /// Get the A2A server.
    pub fn a2a_server(&self) -> &A2aServer {
        &self.a2a_server
    }

    /// Get the event log.
    pub fn event_log(&self) -> Arc<Mutex<Option<EventLog>>> {
        self.event_log.clone()
    }

    /// Get the handoff store.
    pub fn handoff_store(&self) -> Arc<Mutex<Option<HandoffStore>>> {
        self.handoff_store.clone()
    }

    /// Get the data directory.
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_data_dir() -> PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("darius_daemon_test_{}", uuid::Uuid::new_v4()));
        if path.exists() {
            std::fs::remove_dir_all(&path).ok();
        }
        path
    }

    #[test]
    fn daemon_start_stop() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        assert!(!daemon.is_running());

        daemon.start().unwrap();
        assert!(daemon.is_running());

        daemon.stop().unwrap();
        assert!(!daemon.is_running());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn daemon_create_session() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();

        let session = daemon.create_session("default", "test goal").unwrap();
        assert!(session.running);
        assert_eq!(session.profile, "default");
        assert_eq!(session.goal, "test goal");

        let fetched = daemon.get_session(&session.id).unwrap();
        assert_eq!(fetched.id, session.id);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn created_session_survives_dropped_handle() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();

        let session = daemon.create_session("default", "durable goal").unwrap();
        let id = session.id.clone();
        drop(session);

        let persisted = daemon.get_session(&id).unwrap();
        assert_eq!(persisted.goal, "durable goal");
        assert!(persisted.running);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn daemon_attach_detach_session() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();

        let session = daemon.create_session("default", "goal").unwrap();
        let id = session.id.clone();

        daemon.detach_session(&id).unwrap();
        let s = daemon.get_session(&id).unwrap();
        assert!(!s.running);

        daemon.attach_session(&id).unwrap();
        let s = daemon.get_session(&id).unwrap();
        assert!(s.running);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn daemon_end_session_emits_handoff() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();

        let session = daemon.create_session("default", "my goal").unwrap();
        let id = session.id.clone();

        daemon.end_session(&id).unwrap();

        // Verify handoff was emitted.
        let store = daemon.handoff_store();
        let store = store.lock();
        let store = store.as_ref().unwrap();
        let handoff = store.load(&id).unwrap();
        assert_eq!(handoff.goal, "my goal");
        assert_eq!(handoff.version, 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn daemon_status() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();

        let _s1 = daemon.create_session("default", "g1").unwrap();
        let _s2 = daemon.create_session("default", "g2").unwrap();

        let status = daemon.status();
        assert!(status.running);
        assert_eq!(status.active_sessions, 2);
        assert_eq!(status.total_sessions, 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn daemon_list_sessions() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();

        daemon.create_session("default", "g1").unwrap();
        daemon.create_session("default", "g2").unwrap();

        let sessions = daemon.list_sessions();
        assert_eq!(sessions.len(), 2);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn daemon_a2a_server_serves_card() {
        let dir = temp_data_dir();
        let daemon = Daemon::new(&dir);
        let card_json = daemon.a2a_server().serve_card();
        assert!(card_json.contains("darius"));
        assert!(card_json.contains("rlm"));
    }

    #[test]
    fn daemon_double_start_fails() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();
        assert!(daemon.start().is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn daemon_stop_when_not_running_fails() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        assert!(daemon.stop().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
