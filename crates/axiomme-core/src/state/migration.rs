use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{AxiomError, Result};

use super::SqliteStateStore;

const MIGRATION_SCHEMA_SQL: &str = r"
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
        next_attempt_at TEXT NOT NULL,
        lane TEXT NOT NULL
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

    CREATE TABLE IF NOT EXISTS om_records (
        id TEXT PRIMARY KEY,
        scope TEXT NOT NULL CHECK(scope IN ('session', 'thread', 'resource')),
        scope_key TEXT NOT NULL UNIQUE,
        session_id TEXT,
        thread_id TEXT,
        resource_id TEXT,
        generation_count INTEGER NOT NULL DEFAULT 0,
        last_applied_outbox_event_id INTEGER,
        origin_type TEXT NOT NULL CHECK(origin_type IN ('initial', 'reflection')),
        active_observations TEXT NOT NULL DEFAULT '',
        observation_token_count INTEGER NOT NULL DEFAULT 0,
        pending_message_tokens INTEGER NOT NULL DEFAULT 0,
        last_observed_at TEXT,
        current_task TEXT,
        suggested_response TEXT,
        last_activated_message_ids_json TEXT NOT NULL DEFAULT '[]',
        observer_trigger_count_total INTEGER NOT NULL DEFAULT 0,
        reflector_trigger_count_total INTEGER NOT NULL DEFAULT 0,
        is_observing INTEGER NOT NULL DEFAULT 0,
        is_reflecting INTEGER NOT NULL DEFAULT 0,
        is_buffering_observation INTEGER NOT NULL DEFAULT 0,
        is_buffering_reflection INTEGER NOT NULL DEFAULT 0,
        last_buffered_at_tokens INTEGER NOT NULL DEFAULT 0,
        last_buffered_at_time TEXT,
        buffered_reflection TEXT,
        buffered_reflection_tokens INTEGER,
        buffered_reflection_input_tokens INTEGER,
        reflected_observation_line_count INTEGER,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS om_observation_chunks (
        id TEXT PRIMARY KEY,
        record_id TEXT NOT NULL,
        seq INTEGER NOT NULL,
        cycle_id TEXT NOT NULL,
        observations TEXT NOT NULL,
        token_count INTEGER NOT NULL,
        message_tokens INTEGER NOT NULL,
        message_ids_json TEXT NOT NULL,
        last_observed_at TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY (record_id) REFERENCES om_records(id) ON DELETE CASCADE,
        UNIQUE(record_id, seq)
    );

    CREATE TABLE IF NOT EXISTS om_observer_applied_events (
        outbox_event_id INTEGER PRIMARY KEY,
        scope_key TEXT NOT NULL,
        generation_count INTEGER NOT NULL,
        created_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS om_scope_sessions (
        scope_key TEXT NOT NULL,
        session_id TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY(scope_key, session_id)
    );

    CREATE TABLE IF NOT EXISTS om_thread_states (
        scope_key TEXT NOT NULL,
        thread_id TEXT NOT NULL,
        last_observed_at TEXT,
        current_task TEXT,
        suggested_response TEXT,
        updated_at TEXT NOT NULL,
        PRIMARY KEY(scope_key, thread_id)
    );

    CREATE TABLE IF NOT EXISTS om_runtime_metrics (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        reflect_apply_attempts_total INTEGER NOT NULL DEFAULT 0,
        reflect_apply_applied_total INTEGER NOT NULL DEFAULT 0,
        reflect_apply_stale_generation_total INTEGER NOT NULL DEFAULT 0,
        reflect_apply_idempotent_total INTEGER NOT NULL DEFAULT 0,
        reflect_apply_latency_ms_total INTEGER NOT NULL DEFAULT 0,
        reflect_apply_latency_ms_max INTEGER NOT NULL DEFAULT 0,
        updated_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_search_docs_uri ON search_docs(uri);
    CREATE INDEX IF NOT EXISTS idx_search_docs_parent_uri ON search_docs(parent_uri);
    CREATE INDEX IF NOT EXISTS idx_search_docs_mime ON search_docs(mime);
    CREATE INDEX IF NOT EXISTS idx_search_doc_tags_tag ON search_doc_tags(tag);
    CREATE INDEX IF NOT EXISTS idx_om_records_updated_at ON om_records(updated_at);
    CREATE INDEX IF NOT EXISTS idx_om_records_scope_session ON om_records(scope, session_id);
    CREATE INDEX IF NOT EXISTS idx_om_chunks_record_created_at ON om_observation_chunks(record_id, created_at);
    CREATE INDEX IF NOT EXISTS idx_om_observer_applied_scope_generation
    ON om_observer_applied_events(scope_key, generation_count, outbox_event_id);
    CREATE INDEX IF NOT EXISTS idx_om_scope_sessions_scope_updated_at
    ON om_scope_sessions(scope_key, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_om_thread_states_scope_updated_at
    ON om_thread_states(scope_key, updated_at DESC);
";

impl SqliteStateStore {
    pub fn migrate(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AxiomError::mutex_poisoned("sqlite"))?;
        conn.execute_batch(MIGRATION_SCHEMA_SQL)?;
        ensure_required_column(
            &conn,
            "outbox",
            "next_attempt_at",
            "unsupported outbox schema: next_attempt_at is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "outbox",
            "lane",
            "unsupported outbox schema: lane is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "om_records",
            "last_activated_message_ids_json",
            "unsupported om_records schema: last_activated_message_ids_json is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "om_records",
            "current_task",
            "unsupported om_records schema: current_task is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "om_records",
            "suggested_response",
            "unsupported om_records schema: suggested_response is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "om_records",
            "observer_trigger_count_total",
            "unsupported om_records schema: observer_trigger_count_total is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "om_records",
            "reflector_trigger_count_total",
            "unsupported om_records schema: reflector_trigger_count_total is missing; reset workspace state database",
        )?;
        if !has_table(&conn, "search_docs_fts")? {
            conn.execute(
                r"
                CREATE VIRTUAL TABLE search_docs_fts
                USING fts5(
                    name,
                    abstract_text,
                    content,
                    tags_text,
                    tokenize='unicode61 remove_diacritics 2',
                    prefix='2 3'
                )
                ",
                [],
            )?;
            conn.execute(
                r"
                INSERT INTO search_docs_fts(rowid, name, abstract_text, content, tags_text)
                SELECT id, name, abstract_text, content, tags_text
                FROM search_docs
                ",
                [],
            )?;
        }
        drop(conn);
        Ok(())
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

fn has_table(conn: &Connection, table: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            params![table],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
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
