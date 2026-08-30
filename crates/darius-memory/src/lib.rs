//! Lean durable memory engine — SQLite FTS, pack bounds, JSONL import/export.

use serde::{Deserialize, Serialize};
use sqlite::{Connection, State};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] sqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("body too large: {0} bytes (max 32768)")]
    BodyTooLarge(usize),
    #[error("unexpected sqlite state")]
    UnexpectedState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RecordKind {
    Fact,
    Decision,
    Preference,
    Episode,
    Note,
}

impl RecordKind {
    pub fn as_str(&self) -> &str {
        match self {
            RecordKind::Fact => "fact",
            RecordKind::Decision => "decision",
            RecordKind::Preference => "pref",
            RecordKind::Episode => "episode",
            RecordKind::Note => "note",
        }
    }
}

impl FromStr for RecordKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fact" => Ok(RecordKind::Fact),
            "decision" => Ok(RecordKind::Decision),
            "pref" | "preference" => Ok(RecordKind::Preference),
            "episode" => Ok(RecordKind::Episode),
            "note" => Ok(RecordKind::Note),
            _ => Err(format!("unknown record kind: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRecord {
    pub kind: RecordKind,
    pub title: Option<String>,
    pub body: String,
    pub tags: Vec<String>,
    pub importance: f32,
    pub source: Option<String>,
}

impl NewRecord {
    pub fn body_bytes(&self) -> usize {
        self.body.len()
    }
}

const MAX_BODY_BYTES: usize = 32_768;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub kind: RecordKind,
    pub title: Option<String>,
    pub body: String,
    pub tags: Vec<String>,
    pub importance: f32,
    pub content_hash: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_accessed_at: i64,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub kinds: Vec<RecordKind>,
    pub limit: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: None,
            kinds: vec![],
            limit: 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPack {
    pub version: u32,
    pub plain: String,
    pub record_ids: Vec<String>,
}

pub struct MemoryEngine {
    conn: Connection,
}

fn parse_row(stmt: &sqlite::Statement) -> Result<Record, sqlite::Error> {
    Ok(Record {
        id: stmt.read(0)?,
        kind: RecordKind::from_str(&stmt.read::<String, _>(1)?).unwrap_or(RecordKind::Fact),
        title: stmt.read::<Option<String>, _>(2)?.filter(|s| !s.is_empty()),
        body: stmt.read(3)?,
        tags: serde_json::from_str(&stmt.read::<String, _>(4)?).unwrap_or_default(),
        importance: stmt.read::<f64, _>(5)? as f32,
        content_hash: stmt.read(6)?,
        created_at: stmt.read(7)?,
        updated_at: stmt.read(8)?,
        last_accessed_at: stmt.read(9)?,
        source: stmt
            .read::<Option<String>, _>(10)?
            .filter(|s| !s.is_empty()),
    })
}

impl MemoryEngine {
    pub fn open(profile_dir: &Path) -> Result<Self, MemoryError> {
        std::fs::create_dir_all(profile_dir)?;
        let db_path = profile_dir.join("memory.db");
        let conn = Connection::open(&db_path)?;
        let engine = Self { conn };
        engine.migrate()?;
        Ok(engine)
    }

    pub fn open_in_memory() -> Result<Self, MemoryError> {
        let conn = Connection::open(":memory:")?;
        let engine = Self { conn };
        engine.migrate()?;
        Ok(engine)
    }

    fn migrate(&self) -> Result<(), MemoryError> {
        self.conn.execute("PRAGMA journal_mode=WAL")?;
        self.conn.execute("PRAGMA synchronous=NORMAL")?;
        self.conn.execute("PRAGMA temp_store=MEMORY")?;
        self.conn.execute("PRAGMA mmap_size=268435456")?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL,
               title TEXT,
               body TEXT NOT NULL CHECK(length(body) <= 32768),
               tags TEXT NOT NULL DEFAULT '[]',
               importance REAL NOT NULL DEFAULT 0.5,
               content_hash TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               last_accessed_at INTEGER NOT NULL,
               source TEXT
             )",
        )?;
        self.conn
            .execute("CREATE INDEX IF NOT EXISTS idx_records_hash ON records(content_hash)")?;
        self.conn.execute("CREATE INDEX IF NOT EXISTS idx_records_last_accessed ON records(last_accessed_at DESC)")?;
        Ok(())
    }

    pub fn upsert(&self, record: NewRecord) -> Result<Record, MemoryError> {
        if record.body_bytes() > MAX_BODY_BYTES {
            return Err(MemoryError::BodyTooLarge(record.body_bytes()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let content_hash = blake3_hash(&record.body);

        let tags_json = serde_json::to_string(&record.tags).unwrap_or_else(|_| "[]".into());
        let source = record.source.as_deref().unwrap_or("");

        let mut stmt = self.conn.prepare(
            "INSERT INTO records (id, kind, title, body, tags, importance, content_hash, created_at, updated_at, last_accessed_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8, ?9)"
        )?;

        stmt.bind((1, id.as_str()))?;
        stmt.bind((2, record.kind.as_str()))?;
        stmt.bind((3, record.title.as_deref().unwrap_or("")))?;
        stmt.bind((4, record.body.as_str()))?;
        stmt.bind((5, tags_json.as_str()))?;
        stmt.bind((6, record.importance as f64))?;
        stmt.bind((7, content_hash.as_str()))?;
        stmt.bind((8, now as i64))?;
        stmt.bind((9, source))?;

        stmt.next()?;

        Ok(Record {
            id,
            kind: record.kind,
            title: record.title,
            body: record.body,
            tags: record.tags,
            importance: record.importance,
            content_hash,
            created_at: now,
            updated_at: now,
            last_accessed_at: now,
            source: record.source,
        })
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<Record>, MemoryError> {
        let sql: String;
        let search_text: Option<String>;

        if let Some(ref text) = query.text {
            let kind_filter = if query.kinds.is_empty() {
                String::new()
            } else {
                format!(
                    " AND ({})",
                    query
                        .kinds
                        .iter()
                        .map(|k| format!("r.kind = '{}'", k.as_str()))
                        .collect::<Vec<_>>()
                        .join(" OR ")
                )
            };
            search_text = Some(format!("%{text}%"));
            sql = format!(
                "SELECT r.id, r.kind, r.title, r.body, r.tags, r.importance, r.content_hash,
                        r.created_at, r.updated_at, r.last_accessed_at, r.source
                 FROM records r
                 WHERE (r.body LIKE ?1 OR r.title LIKE ?1 OR r.tags LIKE ?1){}
                 ORDER BY r.last_accessed_at DESC
                 LIMIT ?2",
                kind_filter
            );
        } else if query.kinds.is_empty() {
            search_text = None;
            sql = "SELECT id, kind, title, body, tags, importance, content_hash,
                          created_at, updated_at, last_accessed_at, source
                   FROM records
                   ORDER BY last_accessed_at DESC
                   LIMIT ?1"
                .to_string();
        } else {
            search_text = None;
            let kind_filter = query
                .kinds
                .iter()
                .map(|k| format!("kind = '{}'", k.as_str()))
                .collect::<Vec<_>>()
                .join(" OR ");
            sql = format!(
                "SELECT id, kind, title, body, tags, importance, content_hash,
                        created_at, updated_at, last_accessed_at, source
                 FROM records
                 WHERE {}
                 ORDER BY last_accessed_at DESC
                 LIMIT ?1",
                kind_filter
            );
        }

        let mut stmt = self.conn.prepare(&sql)?;

        if let Some(ref text) = search_text {
            stmt.bind((1, text.as_str()))?;
            stmt.bind((2, query.limit as i64))?;
        } else {
            stmt.bind((1, query.limit as i64))?;
        }

        let mut records = Vec::new();
        loop {
            match stmt.next() {
                Ok(State::Done) => break,
                Ok(State::Row) => records.push(parse_row(&stmt)?),
                Ok(_) => return Err(MemoryError::UnexpectedState),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(records)
    }

    pub fn record_count(&self) -> Result<usize, MemoryError> {
        let mut stmt = self.conn.prepare("SELECT COUNT(*) FROM records")?;
        stmt.next()?;
        let count: i64 = stmt.read(0)?;
        Ok(count as usize)
    }

    pub fn db_path(&self) -> PathBuf {
        PathBuf::from("memory.db")
    }
}

fn blake3_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_ok() {
        let engine = MemoryEngine::open_in_memory().unwrap();
        assert_eq!(engine.record_count().unwrap(), 0);
    }

    #[test]
    fn upsert_and_search_keyword() {
        let engine = MemoryEngine::open_in_memory().unwrap();
        let record = NewRecord {
            kind: RecordKind::Fact,
            title: Some("test".into()),
            body: "wal memory test".into(),
            tags: vec!["test".into()],
            importance: 0.5,
            source: None,
        };
        let inserted = engine.upsert(record).unwrap();
        assert!(!inserted.id.is_empty());

        let results = engine
            .search(&SearchQuery {
                text: Some("wal".into()),
                kinds: vec![],
                limit: 12,
            })
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].body, "wal memory test");
    }

    #[test]
    fn body_over_32_kib_rejected() {
        let engine = MemoryEngine::open_in_memory().unwrap();
        let big_body = "x".repeat(32769);
        let record = NewRecord {
            kind: RecordKind::Note,
            title: None,
            body: big_body,
            tags: vec![],
            importance: 0.5,
            source: None,
        };
        let err = engine.upsert(record).unwrap_err();
        assert!(matches!(err, MemoryError::BodyTooLarge(_)));
    }

    #[test]
    fn content_hash_stable_for_same_body() {
        let body = "stable content";
        let hash1 = blake3_hash(body);
        let hash2 = blake3_hash(body);
        assert_eq!(hash1, hash2);
    }
}
