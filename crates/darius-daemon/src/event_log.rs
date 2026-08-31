//! SQLite Event Log — append-only, WAL, synchronous=FULL, integrity on startup.
//!
//! Migrations live here only (owned by Task 3.5, not a standalone Task 3.6).

use rusqlite::{Connection, Result as SqliteResult};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventLogError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("integrity check failed: {0}")]
    Integrity(String),
    #[error("path error: {0}")]
    Path(#[from] std::io::Error),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub id: i64,
    pub session_id: String,
    pub ts: u64,
    pub kind: String,
    pub payload: String,
}

pub struct EventLog {
    conn: Connection,
}

impl EventLog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EventLogError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let exists = path.exists();
        let conn = Connection::open(path)?;
        let log = EventLog { conn };

        log.set_synchronous_full()?;

        if !exists {
            log.enable_wal()?;
            log.run_migrations()?;
        } else {
            log.check_integrity()?;
        }

        Ok(log)
    }

    fn enable_wal(&self) -> SqliteResult<()> {
        self.conn.pragma_update(None, "journal_mode", "wal")?;
        Ok(())
    }

    fn set_synchronous_full(&self) -> SqliteResult<()> {
        self.conn.pragma_update(None, "synchronous", 2)?;
        Ok(())
    }

    fn check_integrity(&self) -> Result<(), EventLogError> {
        let val: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if val != "ok" {
            return Err(EventLogError::Integrity(val));
        }
        Ok(())
    }

    fn run_migrations(&self) -> SqliteResult<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                ts INTEGER NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
            [],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO schema_version (version) VALUES (1)",
            [],
        )?;
        Ok(())
    }

    pub fn append(
        &self,
        session_id: &str,
        kind: &str,
        payload: &str,
    ) -> Result<i64, EventLogError> {
        let ts = current_timestamp();
        self.conn.execute(
            "INSERT INTO events (session_id, ts, kind, payload) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, ts as i64, kind, payload],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn append_batch(
        &self,
        events: Vec<(String, String, String)>,
    ) -> Result<Vec<i64>, EventLogError> {
        let tx = self.conn.unchecked_transaction()?;
        let mut ids = Vec::new();
        for (session_id, kind, payload) in events {
            let ts = current_timestamp();
            tx.execute(
                "INSERT INTO events (session_id, ts, kind, payload) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![&session_id, ts as i64, &kind, &payload],
            )?;
            ids.push(tx.last_insert_rowid());
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn replay(&self, session_id: &str) -> Result<Vec<Event>, EventLogError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, ts, kind, payload FROM events WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let events = stmt.query_map([session_id], |row| {
            Ok(Event {
                id: row.get(0)?,
                session_id: row.get(1)?,
                ts: row.get::<_, i64>(2)? as u64,
                kind: row.get(3)?,
                payload: row.get(4)?,
            })
        })?;
        let mut result = Vec::new();
        for event in events {
            result.push(event?);
        }
        Ok(result)
    }

    pub fn replay_all(&self) -> Result<Vec<Event>, EventLogError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, session_id, ts, kind, payload FROM events ORDER BY id ASC")?;
        let events = stmt.query_map([], |row| {
            Ok(Event {
                id: row.get(0)?,
                session_id: row.get(1)?,
                ts: row.get::<_, i64>(2)? as u64,
                kind: row.get(3)?,
                payload: row.get(4)?,
            })
        })?;
        let mut result = Vec::new();
        for event in events {
            result.push(event?);
        }
        Ok(result)
    }

    pub fn count(&self, session_id: &str) -> Result<i64, EventLogError> {
        let count = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn schema_version(&self) -> Result<i64, EventLogError> {
        let version = self.conn.query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
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

    fn scalar_string(conn: &Connection, sql: &str) -> String {
        conn.query_row(sql, [], |row| row.get(0)).unwrap()
    }

    fn temp_db_path() -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        let file = dir.join(format!("darius_eventlog_test_{}.db", uuid::Uuid::new_v4()));
        if file.exists() {
            std::fs::remove_file(&file).ok();
        }
        file
    }

    #[test]
    fn open_creates_db_with_wal_and_full() {
        let path = temp_db_path();
        let log = EventLog::open(&path).unwrap();
        drop(log);

        let conn = Connection::open(&path).unwrap();
        let wal = scalar_string(&conn, "PRAGMA journal_mode");
        assert_eq!(wal, "wal");
        let sync: i64 = conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 2);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn append_and_replay() {
        let path = temp_db_path();
        let log = EventLog::open(&path).unwrap();

        log.append("sess1", "started", r#"{"goal":"test"}"#)
            .unwrap();
        log.append("sess1", "message", r#"{"text":"hello"}"#)
            .unwrap();
        log.append("sess2", "started", r#"{"goal":"other"}"#)
            .unwrap();

        let replayed = log.replay("sess1").unwrap();
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].session_id, "sess1");
        assert_eq!(replayed[0].kind, "started");
        assert_eq!(replayed[1].kind, "message");

        let all = log.replay_all().unwrap();
        assert_eq!(all.len(), 3);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn append_batch_is_transactional() {
        let path = temp_db_path();
        let log = EventLog::open(&path).unwrap();

        let events = vec![
            (
                "sess1".to_string(),
                "started".to_string(),
                r#"{"goal":"a"}"#.to_string(),
            ),
            (
                "sess1".to_string(),
                "message".to_string(),
                r#"{"text":"b"}"#.to_string(),
            ),
        ];
        let ids = log.append_batch(events).unwrap();
        assert_eq!(ids.len(), 2);

        let replayed = log.replay("sess1").unwrap();
        assert_eq!(replayed.len(), 2);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn replay_preserves_insertion_order() {
        let path = temp_db_path();
        let log = EventLog::open(&path).unwrap();

        for i in 0..10 {
            log.append("sess1", "msg", &format!("{{\"i\":{}}}", i))
                .unwrap();
        }

        let replayed = log.replay("sess1").unwrap();
        assert_eq!(replayed.len(), 10);
        for (i, e) in replayed.iter().enumerate() {
            assert_eq!(e.id, (i as i64) + 1);
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn integrity_check_passes_on_clean_db() {
        let path = temp_db_path();
        let log = EventLog::open(&path).unwrap();
        log.append("sess1", "started", r#"{}"#).unwrap();
        log.check_integrity().unwrap();
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn schema_version_is_recorded() {
        let path = temp_db_path();
        let log = EventLog::open(&path).unwrap();
        assert_eq!(log.schema_version().unwrap(), 1);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reopen_preserves_events() {
        let path = temp_db_path();
        {
            let log = EventLog::open(&path).unwrap();
            log.append("sess-a", "started", r#"{"goal":"persistent"}"#)
                .unwrap();
        }
        let log2 = EventLog::open(&path).unwrap();
        let replayed = log2.replay("sess-a").unwrap();
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].payload, r#"{"goal":"persistent"}"#);
        std::fs::remove_file(&path).unwrap();
    }
}
