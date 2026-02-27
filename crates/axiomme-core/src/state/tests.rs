use chrono::{Duration, Utc};
use tempfile::tempdir;

use crate::models::{IndexRecord, OmReflectionApplyMetrics, QueueEventStatus, ReconcileRunStatus};
use crate::om::{OmObservationChunk, OmOriginType, OmRecord, OmScope};

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

    let events = store
        .fetch_outbox(QueueEventStatus::New, 10)
        .expect("fetch failed");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].uri, "axiom://resources/demo");
}

#[cfg(unix)]
#[test]
fn open_hardens_state_db_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(&db_path).expect("open failed");
    let _ = store
        .enqueue(
            "upsert",
            "axiom://resources/demo",
            serde_json::json!({"x": 1}),
        )
        .expect("enqueue");

    let mode = std::fs::metadata(&db_path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);

    for suffix in ["-wal", "-shm"] {
        let mut os = db_path.as_os_str().to_os_string();
        os.push(suffix);
        let path = PathBuf::from(os);
        if path.exists() {
            let mode = std::fs::metadata(path)
                .expect("sidecar metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
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
fn index_state_remove_prefix_and_clear() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    store
        .upsert_index_state("axiom://resources/demo", "h0", 1, "indexed")
        .expect("upsert root");
    store
        .upsert_index_state("axiom://resources/demo/a.md", "h1", 1, "indexed")
        .expect("upsert leaf");
    store
        .upsert_index_state("axiom://resources/other/b.md", "h2", 1, "indexed")
        .expect("upsert other");

    let removed = store
        .remove_index_state_with_prefix("axiom://resources/demo")
        .expect("remove prefix");
    assert_eq!(removed, 2);

    let uris = store.list_index_state_uris().expect("list");
    assert_eq!(uris, vec!["axiom://resources/other/b.md".to_string()]);

    store.clear_index_state().expect("clear");
    assert!(store.list_index_state_uris().expect("list2").is_empty());
}

#[test]
fn index_state_roundtrip_returns_hash_and_mtime() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    store
        .upsert_index_state("axiom://resources/a.md", "hash-a", 42, "indexed")
        .expect("upsert");
    let state = store
        .get_index_state("axiom://resources/a.md")
        .expect("get")
        .expect("missing");
    assert_eq!(state.0, "hash-a");
    assert_eq!(state.1, 42);
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
fn migration_creates_om_tables() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    let tables = [
        "om_records",
        "om_observation_chunks",
        "om_scope_sessions",
        "om_thread_states",
    ];
    {
        let conn = store.conn.lock().expect("sqlite lock");
        for table in tables {
            let exists = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
                    params![table],
                    |_| Ok(()),
                )
                .optional()
                .expect("query table")
                .is_some();
            assert!(exists, "missing table: {table}");
        }
        drop(conn);
    }
}

#[test]
fn migration_drops_legacy_search_docs_fts_table() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state-legacy-search-fts.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            r"
                CREATE TABLE IF NOT EXISTS search_docs_fts (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL
                );
                ",
        )
        .expect("create legacy search_docs_fts");
        let exists_before = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
                params!["search_docs_fts"],
                |_| Ok(()),
            )
            .optional()
            .expect("query legacy table")
            .is_some();
        assert!(
            exists_before,
            "legacy search_docs_fts table must exist before migrate"
        );
    }

    let store = SqliteStateStore::open(&db_path).expect("open failed");
    let conn = store.conn.lock().expect("sqlite lock");
    let exists_after = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            params!["search_docs_fts"],
            |_| Ok(()),
        )
        .optional()
        .expect("query table after migrate")
        .is_some();
    assert!(
        !exists_after,
        "legacy search_docs_fts table must be dropped"
    );
}

#[test]
fn open_rejects_om_record_schema_without_required_columns() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("om-missing-required-columns.db");
    let now = Utc::now().to_rfc3339();

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            r"
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
                ",
        )
        .expect("create om table without required columns");
        conn.execute(
            r"
                INSERT INTO om_records(
                    id, scope, scope_key, session_id, origin_type,
                    active_observations, observation_token_count, pending_message_tokens,
                    created_at, updated_at
                )
                VALUES(?1, 'session', 'session:s1', 's1', 'initial', 'observation', 10, 20, ?2, ?2)
                ",
            params!["record-old-schema", now],
        )
        .expect("insert om row");
    }

    let err = SqliteStateStore::open(&db_path)
        .expect_err("must reject om_records schema without required columns");
    assert_eq!(err.code(), "VALIDATION_FAILED");
    assert!(
        err.to_string()
            .contains("unsupported om_records schema: last_activated_message_ids_json is missing"),
        "unexpected error message: {err}"
    );
}

#[test]
fn om_record_upsert_and_fetch_by_scope_key() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");
    let now = Utc::now();

    let record = OmRecord {
        id: "record-a".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s-1".to_string(),
        session_id: Some("s-1".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "obs-a".to_string(),
        observation_token_count: 120,
        pending_message_tokens: 300,
        last_observed_at: Some(now),
        current_task: Some("Primary: verify oauth token refresh".to_string()),
        suggested_response: Some("Ask user to confirm new token scope".to_string()),
        last_activated_message_ids: vec!["m-0".to_string()],
        observer_trigger_count_total: 1,
        reflector_trigger_count_total: 0,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: true,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 80,
        last_buffered_at_time: Some(now),
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };

    store.upsert_om_record(&record).expect("upsert record");
    let fetched = store
        .get_om_record_by_scope_key("session:s-1")
        .expect("fetch by scope key")
        .expect("record missing");
    assert_eq!(fetched.id, "record-a");
    assert_eq!(fetched.scope, OmScope::Session);
    assert_eq!(fetched.observation_token_count, 120);
    assert!(fetched.is_buffering_observation);
    assert_eq!(
        fetched.current_task.as_deref(),
        Some("Primary: verify oauth token refresh")
    );
    assert_eq!(
        fetched.suggested_response.as_deref(),
        Some("Ask user to confirm new token scope")
    );
    assert_eq!(fetched.last_activated_message_ids, vec!["m-0".to_string()]);

    let updated = OmRecord {
        id: "record-b".to_string(),
        observation_token_count: 240,
        pending_message_tokens: 100,
        last_activated_message_ids: vec!["m-0".to_string(), "m-1".to_string()],
        updated_at: now + Duration::seconds(5),
        ..record
    };
    store.upsert_om_record(&updated).expect("upsert update");

    let fetched2 = store
        .get_om_record_by_scope_key("session:s-1")
        .expect("fetch2")
        .expect("record missing");
    assert_eq!(fetched2.id, "record-a");
    assert_eq!(fetched2.observation_token_count, 240);
    assert_eq!(fetched2.pending_message_tokens, 100);
    assert_eq!(
        fetched2.last_activated_message_ids,
        vec!["m-0".to_string(), "m-1".to_string()]
    );
}

#[test]
fn om_scope_sessions_upsert_and_list_recent() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    store
        .upsert_om_scope_session("resource:alpha", "s-1")
        .expect("upsert s-1");
    store
        .upsert_om_scope_session("resource:alpha", "s-2")
        .expect("upsert s-2");
    // Refresh s-1 as most recently seen in this scope.
    store
        .upsert_om_scope_session("resource:alpha", "s-1")
        .expect("refresh s-1");
    store
        .upsert_om_scope_session("resource:beta", "s-x")
        .expect("upsert beta");

    let listed = store
        .list_om_scope_sessions("resource:alpha", 10)
        .expect("list alpha");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0], "s-1");
    assert!(listed.contains(&"s-2".to_string()));

    let limited = store
        .list_om_scope_sessions("resource:alpha", 1)
        .expect("list alpha limited");
    assert_eq!(limited, vec!["s-1".to_string()]);

    let beta = store
        .list_om_scope_sessions("resource:beta", 10)
        .expect("list beta");
    assert_eq!(beta, vec!["s-x".to_string()]);
}

#[test]
fn om_scope_sessions_list_scope_keys_for_session_orders_recent() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    store
        .upsert_om_scope_session("thread:t-a", "s-1")
        .expect("upsert thread");
    std::thread::sleep(std::time::Duration::from_millis(2));
    store
        .upsert_om_scope_session("resource:r-z", "s-1")
        .expect("upsert resource");
    std::thread::sleep(std::time::Duration::from_millis(2));
    store
        .upsert_om_scope_session("thread:t-a", "s-1")
        .expect("refresh thread");
    store
        .upsert_om_scope_session("resource:r-z", "s-2")
        .expect("upsert other session");

    let scope_keys = store
        .list_om_scope_keys_for_session("s-1", 10)
        .expect("scope keys");
    assert_eq!(
        scope_keys,
        vec!["thread:t-a".to_string(), "resource:r-z".to_string()]
    );

    let limited = store
        .list_om_scope_keys_for_session("s-1", 1)
        .expect("scope keys limited");
    assert_eq!(limited, vec!["thread:t-a".to_string()]);

    let other = store
        .list_om_scope_keys_for_session("s-2", 10)
        .expect("scope keys other");
    assert_eq!(other, vec!["resource:r-z".to_string()]);
}

#[test]
fn om_thread_states_upsert_and_list_preserves_existing_fields_with_coalesce() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");
    let now = Utc::now();

    store
        .upsert_om_thread_state(
            "resource:alpha",
            "thread-a",
            Some(now),
            Some("Primary: debug auth"),
            None,
        )
        .expect("upsert thread-a initial");
    store
        .upsert_om_thread_state(
            "resource:alpha",
            "thread-a",
            None,
            None,
            Some("Ask user to confirm token scope"),
        )
        .expect("upsert thread-a partial");
    store
        .upsert_om_thread_state(
            "resource:alpha",
            "thread-b",
            Some(now + Duration::seconds(1)),
            Some("Primary: review schema"),
            Some("Share migration diff"),
        )
        .expect("upsert thread-b");

    let listed = store
        .list_om_thread_states("resource:alpha")
        .expect("list thread states");
    assert_eq!(listed.len(), 2);

    let thread_a = listed
        .iter()
        .find(|item| item.thread_id == "thread-a")
        .expect("thread-a state");
    assert_eq!(thread_a.scope_key, "resource:alpha");
    assert_eq!(
        thread_a.current_task.as_deref(),
        Some("Primary: debug auth")
    );
    assert_eq!(
        thread_a.suggested_response.as_deref(),
        Some("Ask user to confirm token scope")
    );
    assert_eq!(thread_a.last_observed_at, Some(now));

    let thread_b = listed
        .iter()
        .find(|item| item.thread_id == "thread-b")
        .expect("thread-b state");
    assert_eq!(
        thread_b.current_task.as_deref(),
        Some("Primary: review schema")
    );
    assert_eq!(
        thread_b.suggested_response.as_deref(),
        Some("Share migration diff")
    );
    assert_eq!(thread_b.last_observed_at, Some(now + Duration::seconds(1)));
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "CAS/idempotency behavior is verified across a full reflection replay sequence"
)]
fn om_reflection_apply_uses_generation_cas_and_event_idempotency() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");
    let now = Utc::now();

    let record = OmRecord {
        id: "record-reflect".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s-reflect".to_string(),
        session_id: Some("s-reflect".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "line-1\nline-2\nline-3".to_string(),
        observation_token_count: 30,
        pending_message_tokens: 0,
        last_observed_at: Some(now),
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 1,
        is_observing: false,
        is_reflecting: true,
        is_buffering_observation: false,
        is_buffering_reflection: true,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    store.upsert_om_record(&record).expect("upsert record");

    let applied = store
        .apply_om_reflection_with_cas(
            "session:s-reflect",
            0,
            41,
            "compact",
            2,
            OmReflectionApplyContext {
                current_task: Some("Primary: consolidate observations"),
                suggested_response: Some("Ask user to confirm next action"),
            },
        )
        .expect("apply reflection");
    assert_eq!(applied, OmReflectionApplyOutcome::Applied);

    let fetched = store
        .get_om_record_by_scope_key("session:s-reflect")
        .expect("fetch 1")
        .expect("record missing");
    assert_eq!(fetched.generation_count, 1);
    assert_eq!(fetched.last_applied_outbox_event_id, Some(41));
    assert_eq!(fetched.origin_type, OmOriginType::Reflection);
    assert_eq!(fetched.active_observations, "compact\n\nline-3");
    assert_eq!(fetched.buffered_reflection, None);
    assert_eq!(fetched.buffered_reflection_tokens, None);
    assert_eq!(fetched.buffered_reflection_input_tokens, None);
    assert_eq!(fetched.reflected_observation_line_count, Some(2));
    assert_eq!(
        fetched.current_task.as_deref(),
        Some("Primary: consolidate observations")
    );
    assert_eq!(
        fetched.suggested_response.as_deref(),
        Some("Ask user to confirm next action")
    );
    assert!(!fetched.is_reflecting);
    assert!(!fetched.is_buffering_reflection);

    let idempotent = store
        .apply_om_reflection_with_cas(
            "session:s-reflect",
            0,
            41,
            "compact-v2",
            2,
            OmReflectionApplyContext::default(),
        )
        .expect("idempotent replay");
    assert_eq!(idempotent, OmReflectionApplyOutcome::IdempotentEvent);

    let stale = store
        .apply_om_reflection_with_cas(
            "session:s-reflect",
            0,
            42,
            "compact-v3",
            2,
            OmReflectionApplyContext::default(),
        )
        .expect("stale generation");
    assert_eq!(stale, OmReflectionApplyOutcome::StaleGeneration);

    let fetched2 = store
        .get_om_record_by_scope_key("session:s-reflect")
        .expect("fetch 2")
        .expect("record missing");
    assert_eq!(fetched2.generation_count, 1);
    assert_eq!(fetched2.last_applied_outbox_event_id, Some(41));
    assert_eq!(fetched2.active_observations, "compact\n\nline-3");

    let metrics = store
        .om_reflection_apply_metrics_snapshot()
        .expect("metrics snapshot");
    assert_eq!(metrics.attempts_total, 3);
    assert_eq!(metrics.applied_total, 1);
    assert_eq!(metrics.stale_generation_total, 1);
    assert_eq!(metrics.idempotent_total, 1);
    assert!((metrics.stale_generation_ratio - (1.0 / 3.0)).abs() < 1e-12);
}

#[test]
fn om_reflection_buffer_apply_uses_generation_cas() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");
    let now = Utc::now();

    let record = OmRecord {
        id: "record-reflect-buffer".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s-reflect-buffer".to_string(),
        session_id: Some("s-reflect-buffer".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "line-1\nline-2\nline-3".to_string(),
        observation_token_count: 30,
        pending_message_tokens: 0,
        last_observed_at: Some(now),
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 1,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: false,
        is_buffering_reflection: true,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    store.upsert_om_record(&record).expect("upsert record");

    let buffered = store
        .buffer_om_reflection_with_cas(
            "session:s-reflect-buffer",
            0,
            OmReflectionBufferPayload {
                reflection: "buffered",
                reflection_token_count: 11,
                reflection_input_tokens: 22,
                reflected_observation_line_count: 2,
                current_task: Some("Primary: consolidate observations"),
                suggested_response: Some("Ask user to confirm next action"),
            },
        )
        .expect("buffer reflection");
    assert!(buffered);

    let fetched = store
        .get_om_record_by_scope_key("session:s-reflect-buffer")
        .expect("fetch")
        .expect("missing");
    assert_eq!(fetched.generation_count, 0);
    assert_eq!(fetched.buffered_reflection.as_deref(), Some("buffered"));
    assert_eq!(fetched.buffered_reflection_tokens, Some(11));
    assert_eq!(fetched.buffered_reflection_input_tokens, Some(22));
    assert_eq!(fetched.reflected_observation_line_count, Some(2));
    assert_eq!(
        fetched.current_task.as_deref(),
        Some("Primary: consolidate observations")
    );
    assert_eq!(
        fetched.suggested_response.as_deref(),
        Some("Ask user to confirm next action")
    );
    assert!(!fetched.is_buffering_reflection);

    let duplicate = store
        .buffer_om_reflection_with_cas(
            "session:s-reflect-buffer",
            0,
            OmReflectionBufferPayload {
                reflection: "buffered-2",
                reflection_token_count: 33,
                reflection_input_tokens: 44,
                reflected_observation_line_count: 2,
                current_task: None,
                suggested_response: None,
            },
        )
        .expect("duplicate buffer");
    assert!(!duplicate);

    let stale = store
        .buffer_om_reflection_with_cas(
            "session:s-reflect-buffer",
            1,
            OmReflectionBufferPayload {
                reflection: "buffered-3",
                reflection_token_count: 55,
                reflection_input_tokens: 66,
                reflected_observation_line_count: 2,
                current_task: None,
                suggested_response: None,
            },
        )
        .expect("stale generation");
    assert!(!stale);
}

#[test]
fn om_reflection_apply_metrics_snapshot_defaults_when_empty() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    let metrics = store
        .om_reflection_apply_metrics_snapshot()
        .expect("metrics snapshot");
    assert_eq!(metrics, OmReflectionApplyMetrics::default());
}

#[test]
fn om_observation_chunks_roundtrip_and_clear_by_seq() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");
    let now = Utc::now();

    let record = OmRecord {
        id: "record-chunk".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s-chunk".to_string(),
        session_id: Some("s-chunk".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: String::new(),
        observation_token_count: 0,
        pending_message_tokens: 0,
        last_observed_at: None,
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 0,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: false,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    store.upsert_om_record(&record).expect("upsert record");

    let chunk1 = OmObservationChunk {
        id: "chunk-1".to_string(),
        record_id: "record-chunk".to_string(),
        seq: 1,
        cycle_id: "c1".to_string(),
        observations: "obs-1".to_string(),
        token_count: 11,
        message_tokens: 101,
        message_ids: vec!["m1".to_string()],
        last_observed_at: now,
        created_at: now,
    };
    let chunk2 = OmObservationChunk {
        id: "chunk-2".to_string(),
        record_id: "record-chunk".to_string(),
        seq: 2,
        cycle_id: "c2".to_string(),
        observations: "obs-2".to_string(),
        token_count: 13,
        message_tokens: 103,
        message_ids: vec!["m2".to_string(), "m3".to_string()],
        last_observed_at: now + Duration::seconds(1),
        created_at: now + Duration::seconds(1),
    };

    store
        .append_om_observation_chunk(&chunk1)
        .expect("append chunk1");
    store
        .append_om_observation_chunk(&chunk2)
        .expect("append chunk2");

    let listed = store
        .list_om_observation_chunks("record-chunk")
        .expect("list chunks");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].seq, 1);
    assert_eq!(listed[1].seq, 2);
    assert_eq!(
        listed[1].message_ids,
        vec!["m2".to_string(), "m3".to_string()]
    );

    let removed = store
        .clear_om_observation_chunks_through_seq("record-chunk", 1)
        .expect("clear through seq");
    assert_eq!(removed, 1);

    let listed2 = store
        .list_om_observation_chunks("record-chunk")
        .expect("list after clear");
    assert_eq!(listed2.len(), 1);
    assert_eq!(listed2[0].seq, 2);
}

#[test]
fn om_observation_chunk_event_cas_blocks_duplicate_replay() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");
    let now = Utc::now();

    let record = OmRecord {
        id: "record-obs-cas".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s-obs-cas".to_string(),
        session_id: Some("s-obs-cas".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 3,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: String::new(),
        observation_token_count: 0,
        pending_message_tokens: 0,
        last_observed_at: None,
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 0,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: false,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    store.upsert_om_record(&record).expect("upsert record");

    let chunk = OmObservationChunk {
        id: "chunk-cas-1".to_string(),
        record_id: "record-obs-cas".to_string(),
        seq: 1,
        cycle_id: "cycle-cas".to_string(),
        observations: "obs-cas".to_string(),
        token_count: 9,
        message_tokens: 99,
        message_ids: vec!["m-cas-1".to_string()],
        last_observed_at: now,
        created_at: now,
    };

    let applied = store
        .append_om_observation_chunk_with_event_cas("session:s-obs-cas", 3, 777, &chunk)
        .expect("first apply");
    assert!(applied);
    assert!(
        store.om_observer_event_applied(777).expect("marker lookup"),
        "event marker should be persisted"
    );

    let replay = store
        .append_om_observation_chunk_with_event_cas("session:s-obs-cas", 3, 777, &chunk)
        .expect("replay apply");
    assert!(!replay);

    let chunks = store
        .list_om_observation_chunks("record-obs-cas")
        .expect("list chunks");
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].id, "chunk-cas-1");
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
        .mark_outbox_status(id, QueueEventStatus::Processing, true)
        .expect("mark processing");
    store.requeue_outbox_with_delay(id, 60).expect("requeue");

    let visible = store
        .fetch_outbox(QueueEventStatus::New, 10)
        .expect("fetch");
    assert!(visible.is_empty());

    store.force_outbox_due_now(id).expect("force due");
    let visible2 = store
        .fetch_outbox(QueueEventStatus::New, 10)
        .expect("fetch2");
    assert_eq!(visible2.len(), 1);
    assert_eq!(visible2[0].id, id);
}

#[test]
fn recover_timed_out_processing_events_requeues_stale_events() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    let id = store
        .enqueue(
            "semantic_scan",
            "axiom://resources/recover-stale",
            serde_json::json!({}),
        )
        .expect("enqueue");
    store
        .mark_outbox_status(id, QueueEventStatus::Processing, true)
        .expect("mark processing");
    let stale_at = (Utc::now() - chrono::Duration::seconds(600)).to_rfc3339();
    store
        .set_outbox_next_attempt_at_for_test(id, &stale_at)
        .expect("set stale next-at");

    let recovered = store
        .recover_timed_out_processing_events(300)
        .expect("recover stale processing");
    assert_eq!(recovered, 1);

    let visible = store
        .fetch_outbox(QueueEventStatus::New, 10)
        .expect("fetch new");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, id);
}

#[test]
fn open_rejects_outbox_without_next_attempt_at() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("outbox-missing-next-at.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            r"
                CREATE TABLE IF NOT EXISTS outbox (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_type TEXT NOT NULL,
                    uri TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    attempt_count INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL
                );
                ",
        )
        .expect("create outbox schema without next_attempt_at");
    }

    let err =
        SqliteStateStore::open(&db_path).expect_err("must reject outbox schema without next_at");
    assert_eq!(err.code(), "VALIDATION_FAILED");
    assert!(
        err.to_string().contains("unsupported outbox schema"),
        "unexpected error message: {err}"
    );
}

#[test]
fn open_rejects_outbox_without_lane_column() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("outbox-missing-lane.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            r"
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
                ",
        )
        .expect("create outbox schema without lane");
        let now = Utc::now().to_rfc3339();
        conn.execute(
                r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at)
                VALUES (?1, ?2, '{}', ?3, 0, 'done', ?3)
                ",
                params!["semantic_scan", "axiom://resources/a", now],
            )
            .expect("insert semantic done");
        conn.execute(
                r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at)
                VALUES (?1, ?2, '{}', ?3, 0, 'done', ?3)
                ",
                params!["upsert", "axiom://resources/a.md", now],
            )
            .expect("insert embedding done");
        conn.execute(
                r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at)
                VALUES (?1, ?2, '{}', ?3, 0, 'dead_letter', ?3)
                ",
                params!["embedding_search_failed", "axiom://resources/a.md", now],
            )
            .expect("insert embedding dead");
    }

    let err = SqliteStateStore::open(&db_path)
        .expect_err("must reject outbox schema without required lane");
    assert_eq!(err.code(), "VALIDATION_FAILED");
    assert!(
        err.to_string()
            .contains("unsupported outbox schema: lane is missing"),
        "unexpected error message: {err}"
    );
}

#[test]
fn open_accepts_outbox_with_required_lane_column_and_existing_rows() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("outbox-with-lane.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            r"
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
                ",
        )
        .expect("create outbox schema with lane");
        let now = Utc::now().to_rfc3339();
        conn.execute(
                r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at, lane)
                VALUES (?1, ?2, '{}', ?3, 0, 'done', ?3, 'semantic')
                ",
                params!["semantic_scan", "axiom://resources/a", now],
            )
            .expect("insert semantic done");
        conn.execute(
                r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at, lane)
                VALUES (?1, ?2, '{}', ?3, 0, 'done', ?3, 'embedding')
                ",
                params!["upsert", "axiom://resources/a.md", now],
            )
            .expect("insert embedding done");
        conn.execute(
                r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at, lane)
                VALUES (?1, ?2, '{}', ?3, 0, 'dead_letter', ?3, 'embedding')
                ",
                params!["embedding_search_failed", "axiom://resources/a.md", now],
            )
            .expect("insert embedding dead");
    }

    let store = SqliteStateStore::open(&db_path).expect("open store");
    let status = store.queue_status().expect("queue status");
    assert_eq!(status.semantic.new_total, 0);
    assert_eq!(status.semantic.new_due, 0);
    assert_eq!(status.semantic.processing, 0);
    assert_eq!(status.semantic.processed, 1);
    assert_eq!(status.semantic.error_count, 0);
    assert_eq!(status.embedding.new_total, 0);
    assert_eq!(status.embedding.new_due, 0);
    assert_eq!(status.embedding.processing, 0);
    assert_eq!(status.embedding.processed, 1);
    assert_eq!(status.embedding.error_count, 1);
}

#[test]
fn open_rejects_outbox_with_invalid_status_domain_value() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("outbox-invalid-status.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            r"
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
                CREATE TABLE IF NOT EXISTS reconcile_runs (
                    run_id TEXT PRIMARY KEY,
                    started_at TEXT NOT NULL,
                    ended_at TEXT,
                    drift_count INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS om_records (
                    id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    scope_key TEXT NOT NULL UNIQUE,
                    session_id TEXT,
                    thread_id TEXT,
                    resource_id TEXT,
                    generation_count INTEGER NOT NULL DEFAULT 0,
                    last_applied_outbox_event_id INTEGER,
                    origin_type TEXT NOT NULL,
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
            ",
        )
        .expect("create legacy schema");
        let now = Utc::now().to_rfc3339();
        conn.execute(
            r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at, lane)
                VALUES (?1, ?2, '{}', ?3, 0, ?4, ?3, ?5)
            ",
            params![
                "semantic_scan",
                "axiom://resources/a",
                now,
                "invalid_status",
                "semantic"
            ],
        )
        .expect("insert invalid outbox status");
    }

    let err = SqliteStateStore::open(&db_path).expect_err("must reject invalid outbox status");
    assert_eq!(err.code(), "VALIDATION_FAILED");
    assert!(
        err.to_string()
            .contains("unsupported outbox schema: invalid status value(s): invalid_status"),
        "unexpected error message: {err}"
    );
}

#[test]
fn open_normalizes_whitespace_and_case_for_status_columns() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("status-normalize.db");

    {
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            r"
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
                CREATE TABLE IF NOT EXISTS reconcile_runs (
                    run_id TEXT PRIMARY KEY,
                    started_at TEXT NOT NULL,
                    ended_at TEXT,
                    drift_count INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS om_records (
                    id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    scope_key TEXT NOT NULL UNIQUE,
                    session_id TEXT,
                    thread_id TEXT,
                    resource_id TEXT,
                    generation_count INTEGER NOT NULL DEFAULT 0,
                    last_applied_outbox_event_id INTEGER,
                    origin_type TEXT NOT NULL,
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
            ",
        )
        .expect("create legacy schema");
        let now = Utc::now().to_rfc3339();
        conn.execute(
            r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, attempt_count, status, next_attempt_at, lane)
                VALUES (?1, ?2, '{}', ?3, 0, ?4, ?3, ?5)
            ",
            params!["semantic_scan", "axiom://resources/a", now, " DONE ", "semantic"],
        )
        .expect("insert outbox status");
        conn.execute(
            r"
                INSERT INTO reconcile_runs(run_id, started_at, ended_at, drift_count, status)
                VALUES (?1, ?2, NULL, 0, ?3)
            ",
            params!["run-1", now, " FAILED "],
        )
        .expect("insert reconcile status");
    }

    let store = SqliteStateStore::open(&db_path).expect("open normalized store");
    let counts = store.queue_counts().expect("queue counts");
    assert_eq!(counts.done, 1);

    let status_raw = {
        let conn = store.conn.lock().expect("lock");
        conn.query_row(
            "SELECT status FROM reconcile_runs WHERE run_id = ?1",
            params!["run-1"],
            |row| row.get::<_, String>(0),
        )
        .expect("read normalized reconcile status")
    };
    assert_eq!(status_raw, ReconcileRunStatus::Failed.as_str());
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
        .mark_outbox_status(id, QueueEventStatus::Processing, true)
        .expect("processing");
    store
        .mark_outbox_status(id, QueueEventStatus::Done, false)
        .expect("done");

    let dead_id = store
        .enqueue(
            "semantic_scan",
            "axiom://resources/b",
            serde_json::json!({}),
        )
        .expect("enqueue dead");
    store
        .mark_outbox_status(dead_id, QueueEventStatus::DeadLetter, true)
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
fn queue_status_splits_semantic_and_embedding_lanes() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    let semantic_done = store
        .enqueue(
            "semantic_scan",
            "axiom://resources/a",
            serde_json::json!({}),
        )
        .expect("enqueue semantic");
    store
        .mark_outbox_status(semantic_done, QueueEventStatus::Done, false)
        .expect("mark semantic done");

    let embedding_done = store
        .enqueue("upsert", "axiom://resources/a.md", serde_json::json!({}))
        .expect("enqueue upsert");
    store
        .mark_outbox_status(embedding_done, QueueEventStatus::Done, false)
        .expect("mark embedding done");

    let embedding_dead = store
        .enqueue(
            "embedding_search_failed",
            "axiom://resources/a.md",
            serde_json::json!({}),
        )
        .expect("enqueue embedding failure");
    store
        .mark_outbox_status(embedding_dead, QueueEventStatus::DeadLetter, false)
        .expect("mark embedding dead");

    let status = store.queue_status().expect("queue status");
    assert_eq!(status.semantic.new_total, 0);
    assert_eq!(status.semantic.new_due, 0);
    assert_eq!(status.semantic.processing, 0);
    assert_eq!(status.semantic.processed, 1);
    assert_eq!(status.semantic.error_count, 0);
    assert_eq!(status.embedding.new_total, 0);
    assert_eq!(status.embedding.new_due, 0);
    assert_eq!(status.embedding.processing, 0);
    assert_eq!(status.embedding.processed, 1);
    assert_eq!(status.embedding.error_count, 1);
}

#[test]
fn queue_status_reports_lane_pending_and_processing_counts() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    let semantic_new = store
        .enqueue(
            "semantic_scan",
            "axiom://resources/semantic-pending",
            serde_json::json!({}),
        )
        .expect("enqueue semantic new");

    let embedding_delayed = store
        .enqueue(
            "upsert",
            "axiom://resources/embedding-delayed.md",
            serde_json::json!({}),
        )
        .expect("enqueue embedding delayed");
    store
        .requeue_outbox_with_delay(embedding_delayed, 60)
        .expect("delay embedding");

    let embedding_processing = store
        .enqueue(
            "embedding_search_failed",
            "axiom://resources/embedding-processing",
            serde_json::json!({}),
        )
        .expect("enqueue embedding processing");
    store
        .mark_outbox_status(embedding_processing, QueueEventStatus::Processing, true)
        .expect("mark processing");

    let status = store.queue_status().expect("queue status");
    assert_eq!(status.semantic.new_total, 1);
    assert_eq!(status.semantic.new_due, 1);
    assert_eq!(status.semantic.processing, 0);
    assert_eq!(status.embedding.new_total, 1);
    assert_eq!(status.embedding.new_due, 0);
    assert_eq!(status.embedding.processing, 1);
    assert_eq!(status.semantic.processed, 0);
    assert_eq!(status.embedding.processed, 0);

    store
        .mark_outbox_status(semantic_new, QueueEventStatus::Done, false)
        .expect("mark semantic done");
}

#[test]
fn queue_status_treats_unknown_lane_rows_as_semantic() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    let id = store
        .enqueue("upsert", "axiom://resources/a.md", serde_json::json!({}))
        .expect("enqueue embedding event");
    store
        .mark_outbox_status(id, QueueEventStatus::Done, false)
        .expect("mark done");

    {
        let conn = store.conn.lock().expect("lock");
        conn.execute(
            "UPDATE outbox SET lane = 'unknown_lane' WHERE id = ?1",
            params![id],
        )
        .expect("force unknown lane");
    }

    let status = store.queue_status().expect("queue status");
    assert_eq!(status.semantic.processed, 1);
    assert_eq!(status.embedding.processed, 0);
}

#[test]
fn queue_counts_match_lane_totals() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    store
        .enqueue(
            "semantic_scan",
            "axiom://resources/semantic-new",
            serde_json::json!({}),
        )
        .expect("enqueue semantic new");
    let embedding_new_delayed = store
        .enqueue(
            "upsert",
            "axiom://resources/embedding-new.md",
            serde_json::json!({}),
        )
        .expect("enqueue embedding new");
    store
        .requeue_outbox_with_delay(embedding_new_delayed, 60)
        .expect("delay embedding new");

    let semantic_done = store
        .enqueue(
            "semantic_scan",
            "axiom://resources/semantic-done",
            serde_json::json!({}),
        )
        .expect("enqueue semantic done");
    store
        .mark_outbox_status(semantic_done, QueueEventStatus::Done, false)
        .expect("mark semantic done");

    let embedding_processing = store
        .enqueue(
            "embedding_search_failed",
            "axiom://resources/embedding-processing",
            serde_json::json!({}),
        )
        .expect("enqueue embedding processing");
    store
        .mark_outbox_status(embedding_processing, QueueEventStatus::Processing, true)
        .expect("mark embedding processing");

    let embedding_dead = store
        .enqueue(
            "embedding_search_failed",
            "axiom://resources/embedding-dead",
            serde_json::json!({}),
        )
        .expect("enqueue embedding dead");
    store
        .mark_outbox_status(embedding_dead, QueueEventStatus::DeadLetter, false)
        .expect("mark embedding dead");

    let counts = store.queue_counts().expect("queue counts");
    let status = store.queue_status().expect("queue status");

    assert_eq!(
        counts.new_total,
        status.semantic.new_total + status.embedding.new_total
    );
    assert_eq!(
        counts.new_due,
        status.semantic.new_due + status.embedding.new_due
    );
    assert_eq!(
        counts.processing,
        status.semantic.processing + status.embedding.processing
    );
    assert_eq!(
        counts.done,
        status.semantic.processed + status.embedding.processed
    );
    assert_eq!(
        counts.dead_letter,
        status.semantic.error_count + status.embedding.error_count
    );
    assert!(counts.earliest_next_attempt_at.is_some());
}

#[test]
fn queue_dead_letter_rates_by_event_type_reports_om_event_ratios() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");

    let om_done = store
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s1",
            serde_json::json!({}),
        )
        .expect("enqueue om done");
    store
        .mark_outbox_status(om_done, QueueEventStatus::Done, false)
        .expect("mark om done");

    let om_dead = store
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s1",
            serde_json::json!({}),
        )
        .expect("enqueue om dead");
    store
        .mark_outbox_status(om_dead, QueueEventStatus::DeadLetter, false)
        .expect("mark om dead");

    let other_dead = store
        .enqueue(
            "semantic_scan",
            "axiom://resources/a",
            serde_json::json!({}),
        )
        .expect("enqueue semantic dead");
    store
        .mark_outbox_status(other_dead, QueueEventStatus::DeadLetter, false)
        .expect("mark semantic dead");

    let rates = store
        .queue_dead_letter_rates_by_event_type()
        .expect("dead letter rates");
    let om_rate = rates
        .iter()
        .find(|entry| entry.event_type == "om_reflect_requested")
        .expect("om rate missing");

    assert_eq!(om_rate.total, 2);
    assert_eq!(om_rate.dead_letter, 1);
    assert!((om_rate.dead_letter_rate - 0.5).abs() < f64::EPSILON);
}

#[test]
fn om_status_snapshot_aggregates_tokens_flags_and_trigger_counts() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let store = SqliteStateStore::open(db_path).expect("open failed");
    let now = Utc::now();

    let first = OmRecord {
        id: "om-status-1".to_string(),
        scope: OmScope::Session,
        scope_key: "session:om-status-1".to_string(),
        session_id: Some("om-status-1".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "alpha".to_string(),
        observation_token_count: 80,
        pending_message_tokens: 20,
        last_observed_at: Some(now),
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 3,
        reflector_trigger_count_total: 1,
        is_observing: false,
        is_reflecting: true,
        is_buffering_observation: true,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    store
        .upsert_om_record(&first)
        .expect("upsert first om record");

    let second = OmRecord {
        id: "om-status-2".to_string(),
        scope: OmScope::Session,
        scope_key: "session:om-status-2".to_string(),
        session_id: Some("om-status-2".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "beta".to_string(),
        observation_token_count: 50,
        pending_message_tokens: 10,
        last_observed_at: Some(now),
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 2,
        reflector_trigger_count_total: 4,
        is_observing: true,
        is_reflecting: false,
        is_buffering_observation: false,
        is_buffering_reflection: true,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    store
        .upsert_om_record(&second)
        .expect("upsert second om record");

    let status = store.om_status_snapshot().expect("om status snapshot");
    assert_eq!(status.records_total, 2);
    assert_eq!(status.observing_count, 1);
    assert_eq!(status.reflecting_count, 1);
    assert_eq!(status.buffering_observation_count, 1);
    assert_eq!(status.buffering_reflection_count, 1);
    assert_eq!(status.observation_tokens_active, 130);
    assert_eq!(status.pending_message_tokens, 30);
    assert_eq!(status.observer_trigger_count_total, 5);
    assert_eq!(status.reflector_trigger_count_total, 5);
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

    let remaining = store.list_search_documents().expect("list remaining");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].uri, "axiom://resources/other/c.md");
}
