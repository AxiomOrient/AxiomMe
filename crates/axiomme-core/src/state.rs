use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value};

use crate::error::{AxiomError, Result};
use crate::models::{
    IndexRecord, OutboxEvent, QueueCheckpoint, QueueCounts, QueueLaneStatus, QueueStatus,
    SearchFilter, TraceIndexEntry,
};

#[derive(Clone)]
pub struct SqliteStateStore {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SqliteSearchHit {
    pub uri: String,
    pub score: f32,
    pub context_type: String,
    pub abstract_text: String,
}

impl std::fmt::Debug for SqliteStateStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStateStore").finish_non_exhaustive()
    }
}

impl SqliteStateStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn migrate(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS index_state (
                uri TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                indexed_at TEXT NOT NULL,
                status TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                uri TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                next_attempt_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS queue_checkpoint (
                worker_name TEXT PRIMARY KEY,
                last_event_id INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS reconcile_runs (
                run_id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                drift_count INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS trace_index (
                trace_id TEXT PRIMARY KEY,
                uri TEXT NOT NULL,
                request_type TEXT NOT NULL,
                query TEXT NOT NULL,
                target_uri TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_trace_index_created_at
            ON trace_index(created_at DESC);

            CREATE TABLE IF NOT EXISTS system_kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS search_docs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                uri TEXT NOT NULL UNIQUE,
                parent_uri TEXT,
                is_leaf INTEGER NOT NULL,
                context_type TEXT NOT NULL,
                name TEXT NOT NULL,
                abstract_text TEXT NOT NULL,
                content TEXT NOT NULL,
                tags_text TEXT NOT NULL,
                mime TEXT,
                updated_at TEXT NOT NULL,
                depth INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS search_doc_tags (
                doc_id INTEGER NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (doc_id, tag),
                FOREIGN KEY (doc_id) REFERENCES search_docs(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_search_docs_uri ON search_docs(uri);
            CREATE INDEX IF NOT EXISTS idx_search_docs_parent_uri ON search_docs(parent_uri);
            CREATE INDEX IF NOT EXISTS idx_search_docs_mime ON search_docs(mime);
            CREATE INDEX IF NOT EXISTS idx_search_doc_tags_tag ON search_doc_tags(tag);

            DROP TABLE IF EXISTS search_docs_fts;
            CREATE VIRTUAL TABLE search_docs_fts
            USING fts5(
                name,
                abstract_text,
                content,
                tags_text,
                tokenize='unicode61 remove_diacritics 2',
                prefix='2 3'
            );
            "#,
        )?;
        ensure_required_column(
            &conn,
            "outbox",
            "next_attempt_at",
            "unsupported outbox schema: next_attempt_at is missing; reset workspace state database",
        )?;
        conn.execute(
            r#"
            INSERT INTO search_docs_fts(rowid, name, abstract_text, content, tags_text)
            SELECT id, name, abstract_text, content, tags_text
            FROM search_docs
            "#,
            [],
        )?;
        Ok(())
    }

    pub fn get_system_value(&self, key: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let value = conn
            .query_row(
                "SELECT value FROM system_kv WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_system_value(&self, key: &str, value: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute(
            r#"
            INSERT INTO system_kv(key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET
              value = excluded.value,
              updated_at = excluded.updated_at
            "#,
            params![key, value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn upsert_index_state(
        &self,
        uri: &str,
        content_hash: &str,
        mtime: i64,
        status: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute(
            r#"
            INSERT INTO index_state(uri, content_hash, mtime, indexed_at, status)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(uri) DO UPDATE SET
              content_hash=excluded.content_hash,
              mtime=excluded.mtime,
              indexed_at=excluded.indexed_at,
              status=excluded.status
            "#,
            params![uri, content_hash, mtime, now, status],
        )?;
        Ok(())
    }

    pub fn get_index_state_hash(&self, uri: &str) -> Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let value = conn
            .query_row(
                "SELECT content_hash FROM index_state WHERE uri = ?1",
                params![uri],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn list_index_state_uris(&self) -> Result<Vec<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let mut stmt = conn.prepare("SELECT uri FROM index_state ORDER BY uri ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_index_state_entries(&self) -> Result<Vec<(String, i64)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let mut stmt = conn.prepare("SELECT uri, mtime FROM index_state ORDER BY uri ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn remove_index_state(&self, uri: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let affected = conn.execute("DELETE FROM index_state WHERE uri = ?1", params![uri])?;
        Ok(affected > 0)
    }

    pub fn enqueue(
        &self,
        event_type: &str,
        uri: &str,
        payload_json: serde_json::Value,
    ) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            r#"
            INSERT INTO outbox(event_type, uri, payload_json, created_at, status, next_attempt_at)
            VALUES (?1, ?2, ?3, ?4, 'new', ?4)
            "#,
            params![event_type, uri, payload_json.to_string(), now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn fetch_outbox(&self, status: &str, limit: usize) -> Result<Vec<OutboxEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let now = Utc::now().to_rfc3339();

        let mut stmt = conn.prepare(
            r#"
            SELECT id, event_type, uri, payload_json, status, attempt_count, next_attempt_at
            FROM outbox
            WHERE status = ?1
              AND (?1 <> 'new' OR COALESCE(next_attempt_at, created_at) <= ?3)
            ORDER BY id ASC
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![status, limit as i64, now], |row| {
            let payload: String = row.get(3)?;
            let value = serde_json::from_str::<serde_json::Value>(&payload)
                .unwrap_or(serde_json::Value::Null);
            Ok(OutboxEvent {
                id: row.get(0)?,
                event_type: row.get(1)?,
                uri: row.get(2)?,
                payload_json: value,
                status: row.get(4)?,
                attempt_count: row.get::<_, i64>(5)? as u32,
                next_attempt_at: row.get(6)?,
            })
        })?;

        let mut events = Vec::new();
        for event in rows {
            events.push(event?);
        }
        Ok(events)
    }

    pub fn get_outbox_event(&self, id: i64) -> Result<Option<OutboxEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, event_type, uri, payload_json, status, attempt_count, next_attempt_at
            FROM outbox
            WHERE id = ?1
            "#,
        )?;

        let row = stmt
            .query_row(params![id], |row| {
                let payload: String = row.get(3)?;
                let value = serde_json::from_str::<serde_json::Value>(&payload)
                    .unwrap_or(serde_json::Value::Null);
                Ok(OutboxEvent {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    uri: row.get(2)?,
                    payload_json: value,
                    status: row.get(4)?,
                    attempt_count: row.get::<_, i64>(5)? as u32,
                    next_attempt_at: row.get(6)?,
                })
            })
            .optional()?;

        Ok(row)
    }

    pub fn mark_outbox_status(&self, id: i64, status: &str, increment_attempt: bool) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        if increment_attempt {
            conn.execute(
                "UPDATE outbox SET status = ?1, attempt_count = attempt_count + 1 WHERE id = ?2",
                params![status, id],
            )?;
        } else {
            conn.execute(
                "UPDATE outbox SET status = ?1 WHERE id = ?2",
                params![status, id],
            )?;
        }
        Ok(())
    }

    pub fn requeue_outbox_with_delay(&self, id: i64, delay_seconds: i64) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let next_attempt = (Utc::now() + Duration::seconds(delay_seconds.max(0))).to_rfc3339();
        conn.execute(
            "UPDATE outbox SET status = 'new', next_attempt_at = ?1 WHERE id = ?2",
            params![next_attempt, id],
        )?;
        Ok(())
    }

    pub fn force_outbox_due_now(&self, id: i64) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE outbox SET next_attempt_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    pub fn get_checkpoint(&self, worker_name: &str) -> Result<Option<i64>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let value = conn
            .query_row(
                "SELECT last_event_id FROM queue_checkpoint WHERE worker_name = ?1",
                params![worker_name],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_checkpoint(&self, worker_name: &str, last_event_id: i64) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute(
            r#"
            INSERT INTO queue_checkpoint(worker_name, last_event_id, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(worker_name) DO UPDATE SET
              last_event_id=excluded.last_event_id,
              updated_at=excluded.updated_at
            "#,
            params![worker_name, last_event_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn start_reconcile_run(&self, run_id: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute(
            r#"
            INSERT OR REPLACE INTO reconcile_runs(run_id, started_at, drift_count, status)
            VALUES (?1, ?2, 0, 'running')
            "#,
            params![run_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn finish_reconcile_run(
        &self,
        run_id: &str,
        drift_count: usize,
        status: &str,
    ) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute(
            r#"
            UPDATE reconcile_runs
            SET ended_at = ?2, drift_count = ?3, status = ?4
            WHERE run_id = ?1
            "#,
            params![run_id, Utc::now().to_rfc3339(), drift_count as i64, status],
        )?;
        Ok(())
    }

    pub fn queue_status(&self) -> Result<QueueStatus> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;

        let processed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'done'",
            [],
            |row| row.get(0),
        )?;
        let failed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'dead_letter'",
            [],
            |row| row.get(0),
        )?;

        let lane = QueueLaneStatus {
            processed: processed as u64,
            error_count: failed as u64,
            errors: Vec::new(),
        };

        Ok(QueueStatus {
            semantic: lane.clone(),
            embedding: lane,
        })
    }

    pub fn queue_counts(&self) -> Result<QueueCounts> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let now = Utc::now().to_rfc3339();

        let new_total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'new'",
            [],
            |row| row.get(0),
        )?;
        let new_due: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'new' AND COALESCE(next_attempt_at, created_at) <= ?1",
            params![now],
            |row| row.get(0),
        )?;
        let processing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'processing'",
            [],
            |row| row.get(0),
        )?;
        let done: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'done'",
            [],
            |row| row.get(0),
        )?;
        let dead_letter: i64 = conn.query_row(
            "SELECT COUNT(*) FROM outbox WHERE status = 'dead_letter'",
            [],
            |row| row.get(0),
        )?;
        let earliest_next_attempt_at = conn
            .query_row(
                "SELECT MIN(COALESCE(next_attempt_at, created_at)) FROM outbox WHERE status = 'new'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();

        Ok(QueueCounts {
            new_total: new_total as u64,
            new_due: new_due as u64,
            processing: processing as u64,
            done: done as u64,
            dead_letter: dead_letter as u64,
            earliest_next_attempt_at,
        })
    }

    pub fn list_checkpoints(&self) -> Result<Vec<QueueCheckpoint>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;

        let mut stmt = conn.prepare(
            "SELECT worker_name, last_event_id, updated_at FROM queue_checkpoint ORDER BY worker_name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(QueueCheckpoint {
                worker_name: row.get(0)?,
                last_event_id: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_trace_index(&self, entry: &TraceIndexEntry) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute(
            r#"
            INSERT INTO trace_index(trace_id, uri, request_type, query, target_uri, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(trace_id) DO UPDATE SET
              uri=excluded.uri,
              request_type=excluded.request_type,
              query=excluded.query,
              target_uri=excluded.target_uri,
              created_at=excluded.created_at
            "#,
            params![
                entry.trace_id,
                entry.uri,
                entry.request_type,
                entry.query,
                entry.target_uri,
                entry.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_trace_index(&self, trace_id: &str) -> Result<Option<TraceIndexEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;

        let row = conn
            .query_row(
                r#"
                SELECT trace_id, uri, request_type, query, target_uri, created_at
                FROM trace_index
                WHERE trace_id = ?1
                "#,
                params![trace_id],
                |row| {
                    Ok(TraceIndexEntry {
                        trace_id: row.get(0)?,
                        uri: row.get(1)?,
                        request_type: row.get(2)?,
                        query: row.get(3)?,
                        target_uri: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()?;

        Ok(row)
    }

    pub fn list_trace_index(&self, limit: usize) -> Result<Vec<TraceIndexEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let mut stmt = conn.prepare(
            r#"
            SELECT trace_id, uri, request_type, query, target_uri, created_at
            FROM trace_index
            ORDER BY created_at DESC, trace_id ASC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(TraceIndexEntry {
                trace_id: row.get(0)?,
                uri: row.get(1)?,
                request_type: row.get(2)?,
                query: row.get(3)?,
                target_uri: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_search_documents(&self) -> Result<Vec<IndexRecord>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT
              d.uri,
              d.parent_uri,
              d.is_leaf,
              d.context_type,
              d.name,
              d.abstract_text,
              d.content,
              d.updated_at,
              d.depth,
              COALESCE(
                (
                    SELECT group_concat(tag, ' ')
                    FROM search_doc_tags t
                    WHERE t.doc_id = d.id
                ),
                d.tags_text,
                ''
              ) AS tags_text
            FROM search_docs d
            ORDER BY d.depth ASC, d.uri ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let uri = row.get::<_, String>(0)?;
            let updated_raw = row.get::<_, String>(7)?;
            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_raw)
                .map(|x| x.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let tags = row
                .get::<_, String>(9)?
                .split_whitespace()
                .map(ToString::to_string)
                .collect::<Vec<_>>();

            Ok(IndexRecord {
                id: blake3::hash(uri.as_bytes()).to_hex().to_string(),
                uri,
                parent_uri: row.get(1)?,
                is_leaf: row.get::<_, i64>(2)? != 0,
                context_type: row.get(3)?,
                name: row.get(4)?,
                abstract_text: row.get(5)?,
                content: row.get(6)?,
                tags,
                updated_at,
                depth: row.get::<_, i64>(8)?.max(0) as usize,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn clear_search_index(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        conn.execute("DELETE FROM search_doc_tags", [])?;
        conn.execute("DELETE FROM search_docs_fts", [])?;
        conn.execute("DELETE FROM search_docs", [])?;
        let _ = conn.execute(
            "INSERT INTO search_docs_fts(search_docs_fts) VALUES('optimize')",
            [],
        );
        Ok(())
    }

    pub fn upsert_search_document(&self, record: &IndexRecord) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let tx = conn.transaction()?;

        let tags = normalize_tags(&record.tags);
        let tags_text = tags.join(" ");
        let mime = infer_mime(record);

        tx.execute(
            r#"
            INSERT INTO search_docs(
                uri, parent_uri, is_leaf, context_type, name, abstract_text, content,
                tags_text, mime, updated_at, depth
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(uri) DO UPDATE SET
              parent_uri=excluded.parent_uri,
              is_leaf=excluded.is_leaf,
              context_type=excluded.context_type,
              name=excluded.name,
              abstract_text=excluded.abstract_text,
              content=excluded.content,
              tags_text=excluded.tags_text,
              mime=excluded.mime,
              updated_at=excluded.updated_at,
              depth=excluded.depth
            "#,
            params![
                record.uri.as_str(),
                record.parent_uri.as_deref(),
                record.is_leaf as i64,
                record.context_type.as_str(),
                record.name.as_str(),
                record.abstract_text.as_str(),
                record.content.as_str(),
                tags_text,
                mime,
                record.updated_at.to_rfc3339(),
                record.depth as i64,
            ],
        )?;

        let doc_id: i64 = tx.query_row(
            "SELECT id FROM search_docs WHERE uri = ?1",
            params![record.uri],
            |row| row.get(0),
        )?;

        tx.execute(
            "DELETE FROM search_doc_tags WHERE doc_id = ?1",
            params![doc_id],
        )?;
        for tag in &tags {
            tx.execute(
                "INSERT OR IGNORE INTO search_doc_tags(doc_id, tag) VALUES (?1, ?2)",
                params![doc_id, tag],
            )?;
        }

        tx.execute(
            "DELETE FROM search_docs_fts WHERE rowid = ?1",
            params![doc_id],
        )?;
        tx.execute(
            "INSERT INTO search_docs_fts(rowid, name, abstract_text, content, tags_text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                doc_id,
                record.name.as_str(),
                record.abstract_text.as_str(),
                record.content.as_str(),
                tags.join(" ")
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn remove_search_document(&self, uri: &str) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let tx = conn.transaction()?;
        let doc_id = tx
            .query_row(
                "SELECT id FROM search_docs WHERE uri = ?1",
                params![uri],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;

        if let Some(doc_id) = doc_id {
            tx.execute(
                "DELETE FROM search_doc_tags WHERE doc_id = ?1",
                params![doc_id],
            )?;
            tx.execute(
                "DELETE FROM search_docs_fts WHERE rowid = ?1",
                params![doc_id],
            )?;
            tx.execute("DELETE FROM search_docs WHERE id = ?1", params![doc_id])?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn remove_search_documents_with_prefix(&self, uri_prefix: &str) -> Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let tx = conn.transaction()?;

        let mut stmt = tx.prepare(
            r#"
            SELECT id FROM search_docs
            WHERE uri = ?1 OR uri LIKE ?2
            "#,
        )?;
        let rows = stmt.query_map(params![uri_prefix, format!("{uri_prefix}/%")], |row| {
            row.get::<_, i64>(0)
        })?;
        let mut doc_ids = Vec::<i64>::new();
        for row in rows {
            doc_ids.push(row?);
        }
        drop(stmt);

        for doc_id in doc_ids {
            tx.execute(
                "DELETE FROM search_doc_tags WHERE doc_id = ?1",
                params![doc_id],
            )?;
            tx.execute(
                "DELETE FROM search_docs_fts WHERE rowid = ?1",
                params![doc_id],
            )?;
            tx.execute("DELETE FROM search_docs WHERE id = ?1", params![doc_id])?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn search_documents_fts(
        &self,
        query: &str,
        target_prefix: Option<&str>,
        filter: Option<&SearchFilter>,
        max_depth: Option<usize>,
        limit: usize,
    ) -> Result<Vec<SqliteSearchHit>> {
        let fts_query = normalize_fts_query(query);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut sql = String::from(
            r#"
            SELECT
              d.uri,
              d.context_type,
              d.abstract_text,
              bm25(search_docs_fts) AS rank
            FROM search_docs_fts
            JOIN search_docs d ON d.id = search_docs_fts.rowid
            WHERE search_docs_fts MATCH ?1
            "#,
        );
        let mut values = vec![Value::Text(fts_query)];
        let mut param_idx = 2usize;

        if let Some(prefix) = target_prefix {
            sql.push_str(&format!(
                " AND (d.uri = ?{param_idx} OR d.uri LIKE ?{})",
                param_idx + 1
            ));
            values.push(Value::Text(prefix.to_string()));
            values.push(Value::Text(format!("{prefix}/%")));
            param_idx += 2;
        }

        if let Some(mime) = filter
            .and_then(|x| x.mime.as_ref())
            .map(|x| x.trim().to_lowercase())
            .filter(|x| !x.is_empty())
        {
            sql.push_str(&format!(" AND d.mime = ?{param_idx}"));
            values.push(Value::Text(mime));
            param_idx += 1;
        }

        if let Some(filter) = filter {
            for tag in normalize_tags(&filter.tags) {
                sql.push_str(&format!(
                    " AND EXISTS (SELECT 1 FROM search_doc_tags t WHERE t.doc_id = d.id AND t.tag = ?{param_idx})"
                ));
                values.push(Value::Text(tag));
                param_idx += 1;
            }
        }

        if let Some(max_depth) = max_depth {
            sql.push_str(&format!(" AND d.depth <= ?{param_idx}"));
            values.push(Value::Integer(max_depth as i64));
            param_idx += 1;
        }

        sql.push_str(&format!(" ORDER BY rank ASC, d.uri ASC LIMIT ?{param_idx}"));
        values.push(Value::Integer(limit.max(1) as i64));

        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::Internal("sqlite mutex poisoned".to_string()))?;
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
            ))
        })?;

        let mut raw = Vec::<(String, String, String, f64)>::new();
        for row in rows {
            raw.push(row?);
        }

        if raw.is_empty() {
            return Ok(Vec::new());
        }

        let mut min_rank = f64::INFINITY;
        let mut max_rank = f64::NEG_INFINITY;
        for (_, _, _, rank) in &raw {
            min_rank = min_rank.min(*rank);
            max_rank = max_rank.max(*rank);
        }
        let range = (max_rank - min_rank).abs();

        let mut out = Vec::with_capacity(raw.len());
        for (uri, context_type, abstract_text, rank) in raw {
            let score = if range <= f64::EPSILON {
                1.0
            } else {
                ((max_rank - rank) / (max_rank - min_rank)).clamp(0.0, 1.0) as f32
            };
            out.push(SqliteSearchHit {
                uri,
                score,
                context_type,
                abstract_text,
            });
        }
        Ok(out)
    }
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut out = tags
        .iter()
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn normalize_fts_query(raw: &str) -> String {
    crate::embedding::tokenize_vec(raw).join(" OR ")
}

fn infer_mime(record: &IndexRecord) -> Option<&'static str> {
    if !record.is_leaf {
        return None;
    }
    let ext = record.name.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some("text/markdown"),
        "txt" | "log" => Some("text/plain"),
        "json" => Some("application/json"),
        "rs" => Some("text/rust"),
        _ => None,
    }
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_required_column(
    conn: &Connection,
    table: &str,
    column: &str,
    error_message: &'static str,
) -> Result<()> {
    if has_column(conn, table, column)? {
        Ok(())
    } else {
        Err(AxiomError::Validation(error_message.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn migrate_and_enqueue() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        let id = store
            .enqueue(
                "upsert",
                "axiom://resources/demo",
                serde_json::json!({"x": 1}),
            )
            .expect("enqueue failed");
        assert!(id > 0);

        let events = store.fetch_outbox("new", 10).expect("fetch failed");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uri, "axiom://resources/demo");
    }

    #[test]
    fn index_state_list_and_delete() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        store
            .upsert_index_state("axiom://resources/a", "h1", 1, "indexed")
            .expect("upsert1");
        store
            .upsert_index_state("axiom://resources/b", "h2", 1, "indexed")
            .expect("upsert2");

        let uris = store.list_index_state_uris().expect("list failed");
        assert_eq!(uris.len(), 2);

        let removed = store
            .remove_index_state("axiom://resources/a")
            .expect("remove failed");
        assert!(removed);
        let uris2 = store.list_index_state_uris().expect("list2 failed");
        assert_eq!(uris2, vec!["axiom://resources/b".to_string()]);
    }

    #[test]
    fn queue_checkpoint_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        assert_eq!(store.get_checkpoint("replay").expect("get1"), None);
        store.set_checkpoint("replay", 42).expect("set checkpoint");
        assert_eq!(store.get_checkpoint("replay").expect("get2"), Some(42));
    }

    #[test]
    fn system_value_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        assert_eq!(
            store.get_system_value("index_profile").expect("get none"),
            None
        );
        store
            .set_system_value("index_profile", "sqlite|hash-v1")
            .expect("set");
        assert_eq!(
            store.get_system_value("index_profile").expect("get value"),
            Some("sqlite|hash-v1".to_string())
        );
    }

    #[test]
    fn requeue_with_delay_hides_event_until_due() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        let id = store
            .enqueue(
                "semantic_scan",
                "axiom://resources/a",
                serde_json::json!({}),
            )
            .expect("enqueue");
        store
            .mark_outbox_status(id, "processing", true)
            .expect("mark processing");
        store.requeue_outbox_with_delay(id, 60).expect("requeue");

        let visible = store.fetch_outbox("new", 10).expect("fetch");
        assert!(visible.is_empty());

        store.force_outbox_due_now(id).expect("force due");
        let visible2 = store.fetch_outbox("new", 10).expect("fetch2");
        assert_eq!(visible2.len(), 1);
        assert_eq!(visible2[0].id, id);
    }

    #[test]
    fn open_rejects_legacy_outbox_without_next_attempt_at() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("legacy.db");

        {
            let conn = Connection::open(&db_path).expect("open legacy");
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS outbox (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_type TEXT NOT NULL,
                    uri TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    attempt_count INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL
                );
                "#,
            )
            .expect("create legacy schema");
        }

        let err = SqliteStateStore::open(&db_path).expect_err("must reject legacy schema");
        assert_eq!(err.code(), "VALIDATION_FAILED");
        assert!(
            err.to_string().contains("unsupported outbox schema"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn queue_counts_and_checkpoints_report_expected_values() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        let id = store
            .enqueue(
                "semantic_scan",
                "axiom://resources/a",
                serde_json::json!({}),
            )
            .expect("enqueue");
        store
            .mark_outbox_status(id, "processing", true)
            .expect("processing");
        store.mark_outbox_status(id, "done", false).expect("done");

        let dead_id = store
            .enqueue(
                "semantic_scan",
                "axiom://resources/b",
                serde_json::json!({}),
            )
            .expect("enqueue dead");
        store
            .mark_outbox_status(dead_id, "dead_letter", true)
            .expect("dead");

        store.set_checkpoint("replay", id).expect("set checkpoint");

        let counts = store.queue_counts().expect("counts");
        assert_eq!(counts.done, 1);
        assert_eq!(counts.dead_letter, 1);
        assert_eq!(counts.new_total, 0);

        let checkpoints = store.list_checkpoints().expect("checkpoints");
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].worker_name, "replay");
        assert_eq!(checkpoints[0].last_event_id, id);
    }

    #[test]
    fn trace_index_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        let first = TraceIndexEntry {
            trace_id: "t1".to_string(),
            uri: "axiom://queue/traces/t1.json".to_string(),
            request_type: "find".to_string(),
            query: "oauth".to_string(),
            target_uri: Some("axiom://resources/demo".to_string()),
            created_at: Utc::now().to_rfc3339(),
        };
        store.upsert_trace_index(&first).expect("upsert first");

        let second = TraceIndexEntry {
            trace_id: "t2".to_string(),
            uri: "axiom://queue/traces/t2.json".to_string(),
            request_type: "search".to_string(),
            query: "memory".to_string(),
            target_uri: None,
            created_at: (Utc::now() + Duration::seconds(1)).to_rfc3339(),
        };
        store.upsert_trace_index(&second).expect("upsert second");

        let got = store.get_trace_index("t1").expect("get").expect("missing");
        assert_eq!(got.uri, "axiom://queue/traces/t1.json");
        assert_eq!(got.request_type, "find");

        let list = store.list_trace_index(10).expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].trace_id, "t2");
        assert_eq!(list[1].trace_id, "t1");
    }

    #[test]
    fn list_search_documents_reconstructs_records() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        let record = IndexRecord {
            id: "origin".to_string(),
            uri: "axiom://resources/docs/auth.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "auth.md".to_string(),
            abstract_text: "oauth".to_string(),
            content: "oauth authorization flow".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };
        store.upsert_search_document(&record).expect("upsert");

        let listed = store.list_search_documents().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].uri, record.uri);
        assert_eq!(listed[0].context_type, "resource");
        assert!(listed[0].tags.iter().any(|x| x == "auth"));
    }

    #[test]
    fn search_documents_fts_applies_prefix_and_filters() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        let markdown = IndexRecord {
            id: "a".to_string(),
            uri: "axiom://resources/docs/auth.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "auth.md".to_string(),
            abstract_text: "oauth auth".to_string(),
            content: "oauth authorization code flow".to_string(),
            tags: vec!["auth".to_string(), "security".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };
        let json_doc = IndexRecord {
            id: "b".to_string(),
            uri: "axiom://resources/docs/schema.json".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "schema.json".to_string(),
            abstract_text: "oauth schema".to_string(),
            content: "oauth token schema".to_string(),
            tags: vec!["auth".to_string(), "schema".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };

        store
            .upsert_search_document(&markdown)
            .expect("upsert markdown");
        store
            .upsert_search_document(&json_doc)
            .expect("upsert json");

        let all = store
            .search_documents_fts("oauth", Some("axiom://resources/docs"), None, None, 10)
            .expect("search all");
        assert_eq!(all.len(), 2);

        let markdown_only = store
            .search_documents_fts(
                "oauth",
                Some("axiom://resources/docs"),
                Some(&SearchFilter {
                    tags: vec![],
                    mime: Some("text/markdown".to_string()),
                }),
                None,
                10,
            )
            .expect("search markdown");
        assert_eq!(markdown_only.len(), 1);
        assert_eq!(markdown_only[0].uri, "axiom://resources/docs/auth.md");

        let tag_and_mime = store
            .search_documents_fts(
                "oauth",
                Some("axiom://resources/docs"),
                Some(&SearchFilter {
                    tags: vec!["schema".to_string()],
                    mime: Some("application/json".to_string()),
                }),
                None,
                10,
            )
            .expect("search tag+mime");
        assert_eq!(tag_and_mime.len(), 1);
        assert_eq!(tag_and_mime[0].uri, "axiom://resources/docs/schema.json");
    }

    #[test]
    fn remove_search_documents_with_prefix_prunes_descendants() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("state.db");
        let store = SqliteStateStore::open(db_path).expect("open failed");

        let first = IndexRecord {
            id: "a".to_string(),
            uri: "axiom://resources/docs/a.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "a.md".to_string(),
            abstract_text: "oauth a".to_string(),
            content: "oauth details".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };
        let second = IndexRecord {
            id: "b".to_string(),
            uri: "axiom://resources/docs/sub/b.md".to_string(),
            parent_uri: Some("axiom://resources/docs/sub".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "b.md".to_string(),
            abstract_text: "oauth b".to_string(),
            content: "oauth b details".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 4,
        };
        let outside = IndexRecord {
            id: "c".to_string(),
            uri: "axiom://resources/other/c.md".to_string(),
            parent_uri: Some("axiom://resources/other".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "c.md".to_string(),
            abstract_text: "oauth c".to_string(),
            content: "oauth c details".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };

        store.upsert_search_document(&first).expect("upsert first");
        store
            .upsert_search_document(&second)
            .expect("upsert second");
        store
            .upsert_search_document(&outside)
            .expect("upsert outside");

        store
            .remove_search_documents_with_prefix("axiom://resources/docs")
            .expect("remove prefix");

        let remaining = store
            .search_documents_fts("oauth", Some("axiom://resources"), None, None, 10)
            .expect("search remaining");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].uri, "axiom://resources/other/c.md");
    }
}
