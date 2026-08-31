//! Lean durable memory engine — SQLite FTS5, pack bounds, JSONL import/export.

use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("body too large: {0} bytes (max 32768)")]
    BodyTooLarge(usize),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
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
    db_path: PathBuf,
}

impl MemoryEngine {
    pub fn open(profile_dir: &Path) -> Result<Self, MemoryError> {
        std::fs::create_dir_all(profile_dir)?;
        let db_path = profile_dir.join("memory.db");
        let conn = Connection::open(&db_path)?;
        let engine = Self { conn, db_path };
        engine.migrate()?;
        Ok(engine)
    }

    pub fn open_in_memory() -> Result<Self, MemoryError> {
        let conn = Connection::open_in_memory()?;
        let engine = Self {
            conn,
            db_path: PathBuf::from(":memory:"),
        };
        engine.migrate()?;
        Ok(engine)
    }

    fn migrate(&self) -> Result<(), MemoryError> {
        self.conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=MEMORY; PRAGMA mmap_size=268435456;")?;
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
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_hash ON records(content_hash)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_last_accessed ON records(last_accessed_at DESC)",
            [],
        )?;

        // FTS5 full-text search (standalone table, triggers keep in sync)
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS records_fts USING fts5(
               record_id,
               title,
               body,
               tags
             )",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS records_ai AFTER INSERT ON records BEGIN
               INSERT INTO records_fts(record_id, title, body, tags)
               VALUES (new.id, new.title, new.body, new.tags);
             END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS records_ad AFTER DELETE ON records BEGIN
               DELETE FROM records_fts WHERE record_id = old.id;
             END",
            [],
        )?;
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS records_au AFTER UPDATE ON records BEGIN
               DELETE FROM records_fts WHERE record_id = old.id;
               INSERT INTO records_fts(record_id, title, body, tags)
               VALUES (new.id, new.title, new.body, new.tags);
             END",
            [],
        )?;

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

        self.conn.execute(
            "INSERT INTO records (id, kind, title, body, tags, importance, content_hash, created_at, updated_at, last_accessed_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8, ?9)",
            rusqlite::params![
                id,
                record.kind.as_str(),
                record.title.as_deref().unwrap_or(""),
                record.body.as_str(),
                tags_json,
                record.importance as f64,
                content_hash,
                now,
                source,
            ],
        )?;

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
            let sql = format!(
                "SELECT r.id, r.kind, r.title, r.body, r.tags, r.importance, r.content_hash,
                        r.created_at, r.updated_at, r.last_accessed_at, r.source
                 FROM records_fts fts
                 JOIN records r ON r.id = fts.record_id
                 WHERE records_fts MATCH ?1{}
                 ORDER BY rank
                 LIMIT ?2",
                kind_filter
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let records = stmt.query_map(rusqlite::params![text, query.limit as i64], parse_row)?;
            let mut results = Vec::new();
            for record in records {
                results.push(record?);
            }
            Ok(results)
        } else if query.kinds.is_empty() {
            let sql = "SELECT id, kind, title, body, tags, importance, content_hash,
                              created_at, updated_at, last_accessed_at, source
                       FROM records
                       ORDER BY last_accessed_at DESC
                       LIMIT ?1";
            let mut stmt = self.conn.prepare(sql)?;
            let records = stmt.query_map([query.limit as i64], parse_row)?;
            let mut results = Vec::new();
            for record in records {
                results.push(record?);
            }
            Ok(results)
        } else {
            let kind_filter = query
                .kinds
                .iter()
                .map(|k| format!("kind = '{}'", k.as_str()))
                .collect::<Vec<_>>()
                .join(" OR ");
            let sql = format!(
                "SELECT id, kind, title, body, tags, importance, content_hash,
                        created_at, updated_at, last_accessed_at, source
                 FROM records
                 WHERE {}
                 ORDER BY last_accessed_at DESC
                 LIMIT ?1",
                kind_filter
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let records = stmt.query_map([query.limit as i64], parse_row)?;
            let mut results = Vec::new();
            for record in records {
                results.push(record?);
            }
            Ok(results)
        }
    }

    pub fn record_count(&self) -> Result<usize, MemoryError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn build_pack(&self, max_chars: usize, limit: usize) -> Result<MemoryPack, MemoryError> {
        let records = self.search(&SearchQuery {
            text: None,
            kinds: vec![],
            limit,
        })?;

        let mut plain = String::new();
        let mut record_ids = Vec::new();

        for record in &records {
            let line = format!(
                "- [{}] {}: {}\n",
                record.kind.as_str(),
                record.title.as_deref().unwrap_or("untitled"),
                record.body
            );

            if plain.len() + line.len() > max_chars {
                break;
            }

            plain.push_str(&line);
            record_ids.push(record.id.clone());
        }

        if plain.ends_with('\n') {
            plain.pop();
        }

        Ok(MemoryPack {
            version: 1,
            plain,
            record_ids,
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    pub fn distill_handoff(
        &self,
        handoff: &darius_core::SessionHandoff,
    ) -> Result<Vec<String>, MemoryError> {
        let mut ids = Vec::new();

        let goal_record = self.upsert(NewRecord {
            kind: RecordKind::Episode,
            title: Some("session goal".into()),
            body: handoff.goal.clone(),
            tags: vec![],
            importance: 0.7,
            source: Some("session_handoff".into()),
        })?;
        ids.push(goal_record.id);

        for decision in &handoff.prior_decisions {
            let decision_record = self.upsert(NewRecord {
                kind: RecordKind::Decision,
                title: Some(decision.context.clone()),
                body: decision.choice.clone(),
                tags: vec![],
                importance: 0.6,
                source: Some("session_handoff".into()),
            })?;
            ids.push(decision_record.id);
        }

        Ok(ids)
    }

    pub fn import_jsonl(&self, path: &Path) -> Result<(usize, usize), MemoryError> {
        let content = std::fs::read_to_string(path)?;
        let mut imported = 0;
        let mut skipped = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let record: NewRecord = match serde_json::from_str(line) {
                Ok(r) => r,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };

            let hash = blake3_hash(&record.body);
            if self.hash_exists(&hash)? {
                skipped += 1;
                continue;
            }

            self.upsert(record)?;
            imported += 1;
        }

        Ok((imported, skipped))
    }

    pub fn export_jsonl(&self, path: &Path) -> Result<usize, MemoryError> {
        let records = self.search(&SearchQuery::default())?;
        let mut content = String::new();

        for record in &records {
            if let Ok(json) = serde_json::to_string(&record) {
                content.push_str(&json);
                content.push('\n');
            }
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &content)?;

        Ok(records.len())
    }

    fn hash_exists(&self, hash: &str) -> Result<bool, MemoryError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM records WHERE content_hash = ?1",
            [hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

impl Clone for MemoryEngine {
    fn clone(&self) -> Self {
        if self.db_path.to_string_lossy() == ":memory:" {
            let conn =
                Connection::open_in_memory().expect("failed to clone in-memory MemoryEngine");
            Self {
                conn,
                db_path: self.db_path.clone(),
            }
        } else {
            let conn =
                Connection::open(&self.db_path).expect("failed to clone MemoryEngine connection");
            Self {
                conn,
                db_path: self.db_path.clone(),
            }
        }
    }
}

fn parse_row(row: &Row<'_>) -> Result<Record, rusqlite::Error> {
    let kind_str: String = row.get(1)?;
    let title_opt: Option<String> = row.get(2)?;
    let tags_str: String = row.get(4)?;
    let importance_f64: f64 = row.get(5)?;
    let source_opt: Option<String> = row.get(10)?;

    Ok(Record {
        id: row.get(0)?,
        kind: RecordKind::from_str(&kind_str).unwrap_or(RecordKind::Fact),
        title: title_opt.filter(|s| !s.is_empty()),
        body: row.get(3)?,
        tags: serde_json::from_str(&tags_str).unwrap_or_default(),
        importance: importance_f64 as f32,
        content_hash: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        last_accessed_at: row.get(9)?,
        source: source_opt.filter(|s| !s.is_empty()),
    })
}

fn blake3_hash(s: &str) -> String {
    // FNV-1a deterministic hash for content dedup
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
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
        engine
            .upsert(NewRecord {
                kind: RecordKind::Note,
                title: Some("test note".into()),
                body: "hello world foo".into(),
                tags: vec!["test".into()],
                importance: 0.5,
                source: None,
            })
            .unwrap();

        let results = engine
            .search(&SearchQuery {
                text: Some("foo".into()),
                kinds: vec![],
                limit: 12,
            })
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("test note"));
    }

    #[test]
    fn body_over_32_kib_rejected() {
        let engine = MemoryEngine::open_in_memory().unwrap();
        let big = "a".repeat(32_769);
        let result = engine.upsert(NewRecord {
            kind: RecordKind::Note,
            title: None,
            body: big,
            tags: vec![],
            importance: 0.5,
            source: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn distill_handoff_creates_records() {
        let engine = MemoryEngine::open_in_memory().unwrap();
        let handoff = darius_core::SessionHandoff {
            version: 1,
            goal: "test goal".into(),
            prior_decisions: vec![darius_core::Decision {
                context: "ctx".into(),
                choice: "opt1".into(),
                rationale: "test".into(),
            }],
            open_questions: vec![],
            constraints: vec![],
            artifact_refs: vec![],
        };

        let ids = engine.distill_handoff(&handoff).unwrap();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn memory_pack_respects_bounds() {
        let engine = MemoryEngine::open_in_memory().unwrap();
        for i in 0..5 {
            engine
                .upsert(NewRecord {
                    kind: RecordKind::Note,
                    title: Some(format!("note {i}")),
                    body: format!("content {i}"),
                    tags: vec![],
                    importance: 0.5,
                    source: None,
                })
                .unwrap();
        }

        let pack = engine.build_pack(100, 12).unwrap();
        assert!(pack.plain.len() <= 100);
    }

    #[test]
    fn import_jsonl_dedupes_duplicates() {
        let dir = std::env::temp_dir().join(format!("darius_jsonl_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let jsonl_path = dir.join("records.jsonl");
        let record = NewRecord {
            kind: RecordKind::Note,
            title: Some("test".into()),
            body: "duplicate content".into(),
            tags: vec![],
            importance: 0.5,
            source: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        std::fs::write(&jsonl_path, format!("{json}\n{json}\n")).unwrap();

        let engine = MemoryEngine::open_in_memory().unwrap();
        let (imported, skipped) = engine.import_jsonl(&jsonl_path).unwrap();
        assert_eq!(imported, 1);
        assert_eq!(skipped, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fts5_search_ranks_results() {
        let engine = MemoryEngine::open_in_memory().unwrap();
        engine
            .upsert(NewRecord {
                kind: RecordKind::Note,
                title: Some("first".into()),
                body: "the quick brown fox".into(),
                tags: vec![],
                importance: 0.5,
                source: None,
            })
            .unwrap();
        engine
            .upsert(NewRecord {
                kind: RecordKind::Note,
                title: Some("second".into()),
                body: "foxes are wild animals".into(),
                tags: vec![],
                importance: 0.5,
                source: None,
            })
            .unwrap();

        let results = engine
            .search(&SearchQuery {
                text: Some("fox".into()),
                kinds: vec![],
                limit: 12,
            })
            .unwrap();
        assert!(!results.is_empty());
    }
}
