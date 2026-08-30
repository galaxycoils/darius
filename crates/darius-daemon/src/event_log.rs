//! SQLite Event Log — append-only, WAL, synchronous=FULL, integrity on startup.
//!
//! Migrations live here only (owned by Task 3.5, not a standalone Task 3.6).

use sqlite::{Connection, State};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventLogError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] sqlite::Error),
    #[error("integrity check failed: {0}")]
    Integrity(String),
    #[error("unexpected statement state")]
    UnexpectedState,
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

    fn enable_wal(&self) -> Result<(), EventLogError> {
        self.conn.execute("PRAGMA journal_mode=WAL")?;
        Ok(())
    }

    fn set_synchronous_full(&self) -> Result<(), EventLogError> {
        self.conn.execute("PRAGMA synchronous=FULL")?;
        Ok(())
    }

    fn check_integrity(&self) -> Result<(), EventLogError> {
        let val = self.scalar_string("PRAGMA integrity_check")?;
        if val != "ok" {
            return Err(EventLogError::Integrity(val));
        }
        Ok(())
    }

    fn run_migrations(&self) -> Result<(), EventLogError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                ts INTEGER NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL
            )",
        )?;
        self.conn
            .execute("CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id)")?;
        self.conn
            .execute("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)")?;
        self.conn
            .execute("INSERT OR IGNORE INTO schema_version (version) VALUES (1)")?;
        Ok(())
    }

    fn scalar_i64(&self, sql: &str) -> Result<i64, EventLogError> {
        let mut stmt = self.conn.prepare(sql)?;
        let state = stmt.next()?;
        if state != State::Row {
            return Err(EventLogError::UnexpectedState);
        }
        let val: i64 = stmt.read(0)?;
        Ok(val)
    }

    fn scalar_string(&self, sql: &str) -> Result<String, EventLogError> {
        let mut stmt = self.conn.prepare(sql)?;
        let state = stmt.next()?;
        if state != State::Row {
            return Err(EventLogError::UnexpectedState);
        }
        let val: String = stmt.read(0)?;
        Ok(val)
    }

    pub fn append(
        &self,
        session_id: &str,
        kind: &str,
        payload: &str,
    ) -> Result<i64, EventLogError> {
        let ts = current_timestamp();
        let mut stmt = self.conn.prepare(
            "INSERT INTO events (session_id, ts, kind, payload) VALUES (?1, ?2, ?3, ?4)",
        )?;
        stmt.bind((1, session_id))?;
        stmt.bind((2, ts as i64))?;
        stmt.bind((3, kind))?;
        stmt.bind((4, payload))?;
        let state = stmt.next()?;
        if state != State::Done {
            return Err(EventLogError::UnexpectedState);
        }
        drop(stmt);
        self.scalar_i64("SELECT last_insert_rowid()")
    }

    pub fn append_batch(
        &self,
        events: Vec<(String, String, String)>,
    ) -> Result<Vec<i64>, EventLogError> {
        self.conn.execute("BEGIN")?;
        let result: Result<Vec<i64>, EventLogError> = (|| {
            let mut ids = Vec::new();
            let mut stmt = self.conn.prepare(
                "INSERT INTO events (session_id, ts, kind, payload) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (session_id, kind, payload) in events {
                let ts = current_timestamp();
                stmt.reset()?;
                stmt.bind((1, &session_id[..]))?;
                stmt.bind((2, ts as i64))?;
                stmt.bind((3, &kind[..]))?;
                stmt.bind((4, &payload[..]))?;
                let state = stmt.next()?;
                if state != State::Done {
                    return Err(EventLogError::UnexpectedState);
                }
                ids.push(self.scalar_i64("SELECT last_insert_rowid()")?);
            }
            Ok(ids)
        })();
        match result {
            Ok(ids) => {
                self.conn.execute("COMMIT")?;
                Ok(ids)
            }
            Err(e) => {
                self.conn.execute("ROLLBACK")?;
                Err(e)
            }
        }
    }

    pub fn replay(&self, session_id: &str) -> Result<Vec<Event>, EventLogError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, ts, kind, payload FROM events WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        stmt.bind((1, session_id))?;
        let mut events = Vec::new();
        loop {
            let state = stmt.next()?;
            if state == State::Done {
                break;
            }
            if state != State::Row {
                return Err(EventLogError::UnexpectedState);
            }
            let id: i64 = stmt.read(0)?;
            let session_id: String = stmt.read(1)?;
            let ts: i64 = stmt.read(2)?;
            let kind: String = stmt.read(3)?;
            let payload: String = stmt.read(4)?;
            events.push(Event {
                id,
                session_id,
                ts: ts as u64,
                kind,
                payload,
            });
        }
        Ok(events)
    }

    pub fn replay_all(&self) -> Result<Vec<Event>, EventLogError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, session_id, ts, kind, payload FROM events ORDER BY id ASC")?;
        let mut events = Vec::new();
        loop {
            let state = stmt.next()?;
            if state == State::Done {
                break;
            }
            if state != State::Row {
                return Err(EventLogError::UnexpectedState);
            }
            let id: i64 = stmt.read(0)?;
            let session_id: String = stmt.read(1)?;
            let ts: i64 = stmt.read(2)?;
            let kind: String = stmt.read(3)?;
            let payload: String = stmt.read(4)?;
            events.push(Event {
                id,
                session_id,
                ts: ts as u64,
                kind,
                payload,
            });
        }
        Ok(events)
    }

    pub fn count(&self, session_id: &str) -> Result<i64, EventLogError> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM events WHERE session_id = ?1")?;
        stmt.bind((1, session_id))?;
        let state = stmt.next()?;
        if state != State::Row {
            return Ok(0);
        }
        let count: i64 = stmt.read(0)?;
        Ok(count)
    }

    pub fn schema_version(&self) -> Result<i64, EventLogError> {
        self.scalar_i64("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
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
        let mut stmt = conn.prepare(sql).unwrap();
        let state = stmt.next().unwrap();
        assert_eq!(state, State::Row);
        stmt.read::<String, _>(0).unwrap()
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

        let conn = sqlite::Connection::open(&path).unwrap();
        let wal = scalar_string(&conn, "PRAGMA journal_mode");
        assert_eq!(wal, "wal");
        let sync = scalar_string(&conn, "PRAGMA synchronous");
        assert_eq!(sync, "2");
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
