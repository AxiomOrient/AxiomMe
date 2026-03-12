use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{AxiomError, Result};
use crate::models::{OmV2MigrationReport, QueueEventStatus, ReconcileRunStatus};
use crate::om::{OM_PROTOCOL_VERSION, OmOriginType, OmRecord, resolve_canonical_thread_id};

use super::SqliteStateStore;

const OM_V2_MIGRATION_APPLIED_AT_KEY: &str = "om_v2_one_shot_migration_applied_at";
const OM_V2_REQUIRED_EPISODIC_REV: &str = "53dfe97bc7df8e32dbee5f7b2be862a6da9171c5";

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
        status TEXT NOT NULL CHECK(status IN ('new', 'processing', 'done', 'dead_letter')),
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
        status TEXT NOT NULL CHECK(status IN ('running', 'dry_run', 'success', 'failed'))
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

    CREATE TABLE IF NOT EXISTS om_entries (
        entry_id TEXT PRIMARY KEY,
        scope_key TEXT NOT NULL,
        canonical_thread_id TEXT NOT NULL,
        priority TEXT NOT NULL CHECK(priority IN ('high', 'medium', 'low')),
        text TEXT NOT NULL,
        source_message_ids_json TEXT NOT NULL,
        origin_kind TEXT NOT NULL CHECK(origin_kind IN ('observation', 'reflection', 'migration')),
        created_at TEXT NOT NULL,
        superseded_by TEXT
    );

    CREATE TABLE IF NOT EXISTS om_reflection_events (
        event_id TEXT PRIMARY KEY,
        scope_key TEXT NOT NULL,
        covers_entry_ids_json TEXT NOT NULL,
        reflection_entry_id TEXT NOT NULL,
        created_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS om_continuation_state (
        scope_key TEXT NOT NULL,
        canonical_thread_id TEXT NOT NULL,
        current_task TEXT,
        suggested_response TEXT,
        confidence REAL NOT NULL DEFAULT 0.0,
        source_kind TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY(scope_key, canonical_thread_id)
    );

    CREATE TABLE IF NOT EXISTS om_protocol_meta (
        id INTEGER PRIMARY KEY CHECK(id = 1),
        protocol_version TEXT NOT NULL,
        episodic_rev TEXT NOT NULL,
        migrated_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
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

    CREATE TABLE IF NOT EXISTS memory_promotion_checkpoints (
        session_id TEXT NOT NULL,
        checkpoint_id TEXT NOT NULL,
        request_hash TEXT NOT NULL,
        request_json TEXT NOT NULL,
        phase TEXT NOT NULL CHECK(phase IN ('pending', 'applying', 'applied')),
        result_json TEXT,
        applied_at TEXT,
        attempt_count INTEGER NOT NULL DEFAULT 0,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (session_id, checkpoint_id)
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
    CREATE INDEX IF NOT EXISTS idx_om_entries_scope_created_at
    ON om_entries(scope_key, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_om_entries_scope_thread
    ON om_entries(scope_key, canonical_thread_id);
    CREATE INDEX IF NOT EXISTS idx_om_continuation_scope_updated_at
    ON om_continuation_state(scope_key, updated_at DESC);
    CREATE INDEX IF NOT EXISTS idx_om_reflection_events_scope_created_at
    ON om_reflection_events(scope_key, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_memory_promotion_checkpoints_session
    ON memory_promotion_checkpoints(session_id, updated_at DESC);
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
        ensure_required_column(
            &conn,
            "om_records",
            "buffered_reflection_tokens",
            "unsupported om_records schema: buffered_reflection_tokens is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "om_records",
            "buffered_reflection_input_tokens",
            "unsupported om_records schema: buffered_reflection_input_tokens is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "om_records",
            "reflected_observation_line_count",
            "unsupported om_records schema: reflected_observation_line_count is missing; reset workspace state database",
        )?;
        ensure_required_column(
            &conn,
            "reconcile_runs",
            "status",
            "unsupported reconcile_runs schema: status is missing; reset workspace state database",
        )?;
        normalize_status_column(&conn, "outbox", "status")?;
        validate_status_domain(
            &conn,
            "outbox",
            "status",
            &outbox_status_domain_values(),
            "unsupported outbox schema",
        )?;
        normalize_status_column(&conn, "reconcile_runs", "status")?;
        validate_status_domain(
            &conn,
            "reconcile_runs",
            "status",
            &reconcile_run_status_domain_values(),
            "unsupported reconcile_runs schema",
        )?;
        conn.execute("DROP TABLE IF EXISTS search_docs_fts", [])?;
        drop(conn);
        Ok(())
    }

    pub fn om_v2_migration_dry_run(&self) -> Result<OmV2MigrationReport> {
        self.run_om_v2_one_shot_migration(true)
    }

    pub fn apply_om_v2_one_shot_migration(&self) -> Result<OmV2MigrationReport> {
        self.run_om_v2_one_shot_migration(false)
    }

    fn run_om_v2_one_shot_migration(&self, dry_run: bool) -> Result<OmV2MigrationReport> {
        let records = self.list_om_records()?;
        let mut report = OmV2MigrationReport {
            dry_run,
            already_applied: false,
            records_scanned: records.len(),
            entries_planned: 0,
            continuation_planned: 0,
            entries_upserted: 0,
            continuation_upserted: 0,
            protocol_meta_updated: false,
            integrity_ok: false,
            protocol_version: OM_PROTOCOL_VERSION.to_string(),
            episodic_rev: OM_V2_REQUIRED_EPISODIC_REV.to_string(),
            issues: Vec::new(),
        };

        for record in &records {
            if !record.active_observations.trim().is_empty() {
                report.entries_planned = report.entries_planned.saturating_add(1);
            }
            if has_continuation_candidate(record) {
                report.continuation_planned = report.continuation_planned.saturating_add(1);
            }
        }

        let meta_matches = self.with_conn(|conn| {
            let meta = read_om_protocol_meta(conn)?;
            Ok(meta.is_some_and(|(protocol_version, episodic_rev)| {
                protocol_version == OM_PROTOCOL_VERSION
                    && episodic_rev == OM_V2_REQUIRED_EPISODIC_REV
            }))
        })?;

        if dry_run {
            report.protocol_meta_updated = !meta_matches;
            report.integrity_ok = true;
            return Ok(report);
        }

        if meta_matches
            && self
                .get_system_value(OM_V2_MIGRATION_APPLIED_AT_KEY)?
                .is_some()
        {
            report.already_applied = true;
            report.integrity_ok = true;
            return Ok(report);
        }

        self.with_tx(|tx| {
            for record in &records {
                if !record.active_observations.trim().is_empty() {
                    let entry_id = entry_id_for_record(record);
                    let source_message_ids_json = serde_json::to_string(&record.last_activated_message_ids)?;
                    tx.execute(
                        r"
                        INSERT INTO om_entries(
                            entry_id, scope_key, canonical_thread_id, priority, text,
                            source_message_ids_json, origin_kind, created_at, superseded_by
                        )
                        VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
                        ON CONFLICT(entry_id) DO UPDATE SET
                            scope_key = excluded.scope_key,
                            canonical_thread_id = excluded.canonical_thread_id,
                            priority = excluded.priority,
                            text = excluded.text,
                            source_message_ids_json = excluded.source_message_ids_json,
                            origin_kind = excluded.origin_kind,
                            created_at = excluded.created_at,
                            superseded_by = NULL
                        ",
                        params![
                            entry_id,
                            record.scope_key,
                            canonical_thread_id_for_record(record),
                            "medium",
                            record.active_observations.trim(),
                            source_message_ids_json,
                            entry_origin_kind(record.origin_type),
                            record.created_at.to_rfc3339(),
                        ],
                    )?;
                    report.entries_upserted = report.entries_upserted.saturating_add(1);
                }

                let current_task = normalize_optional_text(record.current_task.as_deref());
                let suggested_response =
                    normalize_optional_text(record.suggested_response.as_deref());
                if current_task.is_none() && suggested_response.is_none() {
                    continue;
                }

                tx.execute(
                    r"
                    INSERT INTO om_continuation_state(
                        scope_key, canonical_thread_id, current_task, suggested_response,
                        confidence, source_kind, updated_at
                    )
                    VALUES(?1, ?2, ?3, ?4, ?5, 'migration', ?6)
                    ON CONFLICT(scope_key, canonical_thread_id) DO UPDATE SET
                        current_task = excluded.current_task,
                        suggested_response = excluded.suggested_response,
                        confidence = excluded.confidence,
                        source_kind = excluded.source_kind,
                        updated_at = excluded.updated_at
                    ",
                    params![
                        record.scope_key,
                        canonical_thread_id_for_record(record),
                        current_task,
                        suggested_response,
                        continuation_confidence(record.current_task.as_deref(), record.suggested_response.as_deref()),
                        record.updated_at.to_rfc3339(),
                    ],
                )?;
                report.continuation_upserted = report.continuation_upserted.saturating_add(1);
            }

            let now = Utc::now().to_rfc3339();
            tx.execute(
                r"
                INSERT INTO om_protocol_meta(id, protocol_version, episodic_rev, migrated_at, updated_at)
                VALUES(1, ?1, ?2, ?3, ?3)
                ON CONFLICT(id) DO UPDATE SET
                    protocol_version = excluded.protocol_version,
                    episodic_rev = excluded.episodic_rev,
                    updated_at = excluded.updated_at
                ",
                params![OM_PROTOCOL_VERSION, OM_V2_REQUIRED_EPISODIC_REV, now],
            )?;
            report.protocol_meta_updated = true;
            Ok(())
        })?;

        self.set_system_value(OM_V2_MIGRATION_APPLIED_AT_KEY, &Utc::now().to_rfc3339())?;

        let issues = self.validate_om_v2_migration_integrity(&records)?;
        report.integrity_ok = issues.is_empty();
        report.issues = issues;
        if !report.integrity_ok {
            return Err(AxiomError::Validation(format!(
                "om v2 one-shot migration integrity check failed: {}",
                report.issues.join("; ")
            )));
        }
        Ok(report)
    }

    fn validate_om_v2_migration_integrity(&self, records: &[OmRecord]) -> Result<Vec<String>> {
        self.with_conn(|conn| {
            let mut issues = Vec::<String>::new();

            let meta = read_om_protocol_meta(conn)?;
            match meta {
                Some((protocol_version, episodic_rev))
                    if protocol_version == OM_PROTOCOL_VERSION
                        && episodic_rev == OM_V2_REQUIRED_EPISODIC_REV => {}
                Some((protocol_version, episodic_rev)) => {
                    issues.push(format!(
                        "om_protocol_meta mismatch: protocol_version={protocol_version}, episodic_rev={episodic_rev}"
                    ));
                }
                None => issues.push("missing om_protocol_meta row".to_string()),
            }

            for record in records {
                if !record.active_observations.trim().is_empty() {
                    let exists = conn
                        .query_row(
                            "SELECT 1 FROM om_entries WHERE entry_id = ?1 LIMIT 1",
                            params![entry_id_for_record(record)],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some();
                    if !exists {
                        issues.push(format!(
                            "missing om_entries row for scope_key={}",
                            record.scope_key
                        ));
                    }
                }

                if has_continuation_candidate(record) {
                    let exists = conn
                        .query_row(
                            "SELECT 1 FROM om_continuation_state WHERE scope_key = ?1 AND canonical_thread_id = ?2 LIMIT 1",
                            params![record.scope_key, canonical_thread_id_for_record(record)],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some();
                    if !exists {
                        issues.push(format!(
                            "missing om_continuation_state row for scope_key={} canonical_thread_id={}",
                            record.scope_key,
                            canonical_thread_id_for_record(record)
                        ));
                    }
                }
            }

            Ok(issues)
        })
    }
}

fn read_om_protocol_meta(conn: &Connection) -> Result<Option<(String, String)>> {
    let row = conn
        .query_row(
            "SELECT protocol_version, episodic_rev FROM om_protocol_meta WHERE id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    Ok(row)
}

fn entry_id_for_record(record: &OmRecord) -> String {
    format!(
        "omv2:{}:{}:{}",
        record.scope_key,
        record.generation_count,
        record.created_at.timestamp_micros()
    )
}

fn canonical_thread_id_for_record(record: &OmRecord) -> String {
    let fallback = record
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(record.scope_key.as_str());
    resolve_canonical_thread_id(
        record.scope,
        &record.scope_key,
        record.thread_id.as_deref(),
        record.session_id.as_deref(),
        fallback,
    )
}

const fn entry_origin_kind(origin: OmOriginType) -> &'static str {
    match origin {
        OmOriginType::Initial => "observation",
        OmOriginType::Reflection => "reflection",
    }
}

fn has_continuation_candidate(record: &OmRecord) -> bool {
    normalize_optional_text(record.current_task.as_deref()).is_some()
        || normalize_optional_text(record.suggested_response.as_deref()).is_some()
}

fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn continuation_confidence(current_task: Option<&str>, suggested_response: Option<&str>) -> f64 {
    let has_current_task = normalize_optional_text(current_task).is_some();
    let has_suggested_response = normalize_optional_text(suggested_response).is_some();
    if has_current_task && has_suggested_response {
        0.92
    } else if has_current_task || has_suggested_response {
        0.82
    } else {
        0.0
    }
}

fn outbox_status_domain_values() -> [&'static str; 4] {
    [
        QueueEventStatus::New.as_str(),
        QueueEventStatus::Processing.as_str(),
        QueueEventStatus::Done.as_str(),
        QueueEventStatus::DeadLetter.as_str(),
    ]
}

fn reconcile_run_status_domain_values() -> [&'static str; 4] {
    [
        ReconcileRunStatus::Running.as_str(),
        ReconcileRunStatus::DryRun.as_str(),
        ReconcileRunStatus::Success.as_str(),
        ReconcileRunStatus::Failed.as_str(),
    ]
}

fn normalize_status_column(conn: &Connection, table: &str, column: &str) -> Result<()> {
    conn.execute(
        &format!(
            "UPDATE {table} SET {column} = lower(trim({column})) WHERE {column} <> lower(trim({column}))"
        ),
        [],
    )?;
    Ok(())
}

fn validate_status_domain(
    conn: &Connection,
    table: &str,
    column: &str,
    allowed: &[&str],
    error_prefix: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("SELECT DISTINCT {column} FROM {table}"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut invalid = Vec::<String>::new();
    for row in rows {
        let value = row?.trim().to_ascii_lowercase();
        if !allowed.contains(&value.as_str()) {
            invalid.push(value);
        }
    }

    if invalid.is_empty() {
        return Ok(());
    }

    invalid.sort();
    invalid.dedup();
    Err(AxiomError::Validation(format!(
        "{error_prefix}: invalid status value(s): {}",
        invalid.join(", ")
    )))
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
