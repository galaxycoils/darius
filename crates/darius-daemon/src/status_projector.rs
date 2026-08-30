//! Status Projector — projects daemon status for humans/CLI.

use crate::daemon::Daemon;
use parking_lot::Mutex;
use std::sync::Arc;

/// Status projector — formats daemon status for display.
pub struct StatusProjector {
    daemon: Arc<Mutex<Daemon>>,
}

impl StatusProjector {
    pub fn new(daemon: Arc<Mutex<Daemon>>) -> Self {
        Self { daemon }
    }

    /// Project status as a human-readable string.
    pub fn project(&self) -> String {
        let daemon = self.daemon.lock();
        let status = daemon.status();

        let uptime = if status.uptime_secs > 0 {
            format!("{}s", status.uptime_secs)
        } else {
            "not running".into()
        };

        let status_str = if status.running { "running" } else { "stopped" };

        format!(
            "Darius Daemon v{} — {}\n  Uptime: {}\n  Active sessions: {}\n  Total sessions: {}",
            status.version, status_str, uptime, status.active_sessions, status.total_sessions
        )
    }

    /// Project session details.
    pub fn project_sessions(&self) -> String {
        let daemon = self.daemon.lock();
        let sessions = daemon.list_sessions();
        if sessions.is_empty() {
            return "No sessions".into();
        }

        let mut output = String::from("Sessions:\n");
        for s in sessions {
            let status_str = if s.running { "active" } else { "inactive" };
            output.push_str(&format!(
                "  [{}] {} — {} (profile: {})\n",
                &s.id[..8],
                status_str,
                s.goal,
                s.profile
            ));
        }
        output
    }

    /// Project a single session.
    pub fn project_session(&self, session_id: &str) -> Option<String> {
        let daemon = self.daemon.lock();
        daemon.get_session(session_id).ok().map(|s| {
            let status_str = if s.running { "active" } else { "inactive" };
            format!(
                "Session {}:\n  Status: {}\n  Profile: {}\n  Goal: {}\n  Created: {}\n  Last activity: {}",
                s.id,
                status_str,
                s.profile,
                s.goal,
                s.created_at,
                s.last_activity
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_data_dir() -> PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("darius_status_proj_test_{}", uuid::Uuid::new_v4()));
        if path.exists() {
            std::fs::remove_dir_all(&path).ok();
        }
        path
    }

    #[test]
    fn project_stopped_daemon() {
        let dir = temp_data_dir();
        let daemon = Daemon::new(&dir);
        let projector = StatusProjector::new(Arc::new(Mutex::new(daemon)));

        let output = projector.project();
        assert!(output.contains("stopped"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_running_daemon_with_sessions() {
        let dir = temp_data_dir();
        let mut daemon = Daemon::new(&dir);
        daemon.start().unwrap();
        daemon.create_session("default", "test goal").unwrap();

        let projector = StatusProjector::new(Arc::new(Mutex::new(daemon)));

        let output = projector.project();
        assert!(output.contains("running"));
        assert!(output.contains("Active sessions: 1"));

        let sessions_output = projector.project_sessions();
        assert!(sessions_output.contains("test goal"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
