use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;
use tempfile::{TempDir, tempdir};

use crate::models::{
    ContextHit, FindResult, IndexRecord, QueryPlan, RuntimeHint, RuntimeHintKind, SearchBudget,
    SearchOptions, SearchRequest,
};
use crate::om::{OmOriginType, OmRecord, OmScope, build_scope_key};

use super::reranker::{RerankerMode, resolve_reranker_mode};
use super::{
    AxiomMe, OmHintPolicy, merge_observation_hint_with_suggested_response,
    merge_recent_and_om_hints, merge_runtime_om_recent_hints, normalize_runtime_hints,
};

#[test]
fn reranker_mode_parser_defaults_to_off() {
    assert_eq!(resolve_reranker_mode(None), RerankerMode::Off);
    assert_eq!(resolve_reranker_mode(Some("unknown")), RerankerMode::Off);
    assert_eq!(resolve_reranker_mode(Some("doc-aware")), RerankerMode::Off);
    assert_eq!(
        resolve_reranker_mode(Some("doc-aware-v1")),
        RerankerMode::DocAwareV1
    );
    assert_eq!(resolve_reranker_mode(Some("OFF")), RerankerMode::Off);
}

#[test]
fn search_with_budget_propagates_budget_notes() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let root = IndexRecord {
        id: "root".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resources root".to_string(),
        content: "resources root".to_string(),
        tags: Vec::new(),
        updated_at: Utc::now(),
        depth: 0,
    };
    let leaf = IndexRecord {
        id: "leaf".to_string(),
        uri: "axiom://resources/auth.md".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "auth.md".to_string(),
        abstract_text: "OAuth".to_string(),
        content: "oauth authorization code flow".to_string(),
        tags: vec!["auth".to_string()],
        updated_at: Utc::now(),
        depth: 1,
    };

    {
        let mut index = app.index.write().expect("index write");
        index.upsert(root.clone());
        index.upsert(leaf.clone());
    }
    app.state
        .upsert_search_document(&root)
        .expect("upsert root");
    app.state
        .upsert_search_document(&leaf)
        .expect("upsert leaf");

    let result = app
        .search_with_request(SearchRequest {
            query: "oauth".to_string(),
            target_uri: Some("axiom://resources".to_string()),
            session: None,
            limit: Some(5),
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            budget: Some(SearchBudget {
                max_ms: None,
                max_nodes: Some(1),
                max_depth: Some(3),
            }),
            runtime_hints: Vec::new(),
        })
        .expect("search with budget");

    let notes = &result.query_plan.notes;
    assert!(notes.iter().any(|x| x == "budget_nodes:1"));
    assert!(notes.iter().any(|x| x == "budget_depth:3"));
}

#[test]
fn memory_backend_reads_in_memory_index() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let record = IndexRecord {
        id: "sqlite-only".to_string(),
        uri: "axiom://resources/sqlite-only.md".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "sqlite-only.md".to_string(),
        abstract_text: "sqlite only".to_string(),
        content: "bm25 sqlite fts".to_string(),
        tags: vec!["sqlite".to_string()],
        updated_at: Utc::now(),
        depth: 1,
    };
    app.state
        .upsert_search_document(&record)
        .expect("upsert search document");
    {
        let mut index = app.index.write().expect("index write");
        index.upsert(record.clone());
    }

    let result = app
        .run_retrieval_memory_only(&SearchOptions {
            query: "sqlite".to_string(),
            target_uri: Some(
                crate::uri::AxiomUri::parse("axiom://resources").expect("target parse"),
            ),
            session: None,
            session_hints: Vec::new(),
            budget: None,
            limit: 5,
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            request_type: "search".to_string(),
        })
        .expect("memory retrieval");

    assert!(
        result
            .query_results
            .iter()
            .any(|x| x.uri == "axiom://resources/sqlite-only.md")
    );
}

#[test]
fn memory_backend_returns_hits_for_in_memory_records() {
    let (_temp, app) = setup_test_app();
    upsert_records(
        &app,
        &[
            resources_root_record("root-fail-fast"),
            resources_leaf_record(
                "leaf-fail-fast",
                "fail-fast.md",
                "memory retrieval",
                "memory retrieval returns in-memory hits",
                &["memory"],
            ),
        ],
    );

    let result = app
        .run_retrieval_memory_only(&search_options("memory"))
        .expect("memory retrieval");
    assert!(!result.query_results.is_empty());
}

#[test]
fn memory_backend_policy_note_is_explicit() {
    let (_temp, app) = setup_test_app();
    upsert_records(
        &app,
        &[
            resources_root_record("root-fallback"),
            resources_leaf_record(
                "leaf-fallback",
                "fallback.md",
                "fallback memory",
                "memory retrieval policy note",
                &["fallback"],
            ),
        ],
    );

    let result = app
        .run_retrieval_memory_only(&search_options("fallback"))
        .expect("memory retrieval");
    assert!(
        result
            .query_results
            .iter()
            .any(|x| x.uri == "axiom://resources/fallback.md")
    );
    let notes = &result.query_plan.notes;
    assert!(notes.iter().any(|x| x == "backend_policy:memory_only"));
    assert!(notes.iter().any(|x| x == "backend:memory"));
}

#[test]
fn doc_aware_reranker_prioritizes_config_documents() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let config = IndexRecord {
        id: "cfg-1".to_string(),
        uri: "axiom://resources/app/settings.toml".to_string(),
        parent_uri: Some("axiom://resources/app".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "settings.toml".to_string(),
        abstract_text: "runtime settings".to_string(),
        content: "database_url and retry limits".to_string(),
        tags: vec!["config".to_string()],
        updated_at: Utc::now(),
        depth: 3,
    };
    let guide = IndexRecord {
        id: "guide-1".to_string(),
        uri: "axiom://resources/app/guide.md".to_string(),
        parent_uri: Some("axiom://resources/app".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "guide.md".to_string(),
        abstract_text: "developer guide".to_string(),
        content: "overview and onboarding".to_string(),
        tags: vec!["markdown".to_string()],
        updated_at: Utc::now(),
        depth: 3,
    };
    {
        let mut index = app.index.write().expect("index write");
        index.upsert(config);
        index.upsert(guide);
    }

    let mut result = sample_find_result(vec![
        hit("axiom://resources/app/guide.md", 0.92),
        hit("axiom://resources/app/settings.toml", 0.86),
    ]);
    app.apply_reranker_with_mode(
        "config env settings",
        &mut result,
        2,
        RerankerMode::DocAwareV1,
    )
    .expect("rerank");

    assert_eq!(
        result.query_results[0].uri,
        "axiom://resources/app/settings.toml"
    );
    let notes = &result.query_plan.notes;
    assert!(notes.iter().any(|x| x == "reranker:doc-aware-v1"));
}

#[test]
fn search_injects_om_hint_and_records_om_metrics_in_request_log() {
    let (_temp, app) = setup_test_app();
    upsert_records(
        &app,
        &[
            resources_root_record("root-om"),
            resources_leaf_record(
                "leaf-om",
                "om-note.md",
                "om",
                "oauth memory retrieval note",
                &["om"],
            ),
        ],
    );

    let session = app.session(Some("s-om-search"));
    session.load().expect("session load");
    session
        .add_message("user", "OAuth hint from recent message")
        .expect("session append");

    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-search"), None, None).expect("scope key");
    let now = Utc::now();
    let mut om_record = base_om_record("om-search-record", OmScope::Session, &scope_key, now);
    om_record.session_id = Some("s-om-search".to_string());
    om_record.active_observations =
        "alpha observation\nbeta observation\nlatest observation".to_string();
    om_record.observation_token_count = 120;
    om_record.observer_trigger_count_total = 7;
    om_record.reflector_trigger_count_total = 2;
    app.state
        .upsert_om_record(&om_record)
        .expect("upsert om record");

    let result = app
        .search(
            "oauth",
            Some("axiom://resources"),
            Some("s-om-search"),
            Some(5),
            None,
            None,
        )
        .expect("search");
    let notes = &result.query_plan.notes;
    assert!(
        notes
            .iter()
            .any(|value| value.starts_with("om_hint_applied:1"))
    );
    assert!(
        notes
            .iter()
            .any(|value| value.starts_with("om_hint_policy:"))
    );

    let logs = app
        .list_request_logs_filtered(5, Some("search"), Some("ok"))
        .expect("request logs");
    let entry = logs.first().expect("latest search log");
    let details = entry.details.as_ref().expect("search details");

    assert!(details.get("context_tokens_before_om").is_some());
    assert!(details.get("context_tokens_after_om").is_some());
    assert_eq!(
        details
            .get("observation_tokens_active")
            .and_then(serde_json::Value::as_u64),
        Some(120)
    );
    assert_eq!(
        details
            .get("observer_trigger_count")
            .and_then(serde_json::Value::as_u64),
        Some(7)
    );
    assert_eq!(
        details
            .get("reflector_trigger_count")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        details
            .get("om_hint_applied")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(details.get("om_hint_policy").is_some());
}

#[test]
fn search_with_session_context_always_records_om_metrics_without_om_record() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let root = IndexRecord {
        id: "root-om-metrics".to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resources root".to_string(),
        content: "resources root".to_string(),
        tags: Vec::new(),
        updated_at: Utc::now(),
        depth: 0,
    };
    let leaf = IndexRecord {
        id: "leaf-om-metrics".to_string(),
        uri: "axiom://resources/om-metrics.md".to_string(),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: "om-metrics.md".to_string(),
        abstract_text: "om metrics".to_string(),
        content: "oauth session search context".to_string(),
        tags: vec!["om".to_string()],
        updated_at: Utc::now(),
        depth: 1,
    };
    {
        let mut index = app.index.write().expect("index write");
        index.upsert(root.clone());
        index.upsert(leaf.clone());
    }
    app.state
        .upsert_search_document(&root)
        .expect("upsert root");
    app.state
        .upsert_search_document(&leaf)
        .expect("upsert leaf");

    let session = app.session(Some("s-om-metrics-empty"));
    session.load().expect("session load");
    session
        .add_message("user", "recent context without om record")
        .expect("session append");

    app.search(
        "oauth",
        Some("axiom://resources"),
        Some("s-om-metrics-empty"),
        Some(5),
        None,
        None,
    )
    .expect("search");

    let logs = app
        .list_request_logs_filtered(5, Some("search"), Some("ok"))
        .expect("request logs");
    let entry = logs.first().expect("latest search log");
    let details = entry.details.as_ref().expect("search details");

    assert!(details.get("context_tokens_before_om").is_some());
    assert!(details.get("context_tokens_after_om").is_some());
    assert_eq!(
        details
            .get("observation_tokens_active")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        details
            .get("observer_trigger_count")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        details
            .get("reflector_trigger_count")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        details
            .get("om_hint_applied")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(details.get("session_recent_hint_count").is_some());
    assert!(details.get("session_hint_count_final").is_some());
    assert!(details.get("om_filtered_message_count").is_some());
    assert!(details.get("om_hint_policy").is_some());
    assert_eq!(
        details
            .get("retrieval_backend")
            .and_then(serde_json::Value::as_str),
        Some("memory")
    );
    assert_eq!(
        details
            .get("retrieval_backend_policy")
            .and_then(serde_json::Value::as_str),
        Some("memory_only")
    );
}

#[test]
fn fetch_session_om_state_returns_none_when_om_disabled() {
    let (_temp, app) = setup_test_app();

    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-disabled"), None, None).expect("scope");
    let now = Utc::now();
    let mut om_record = base_om_record("om-disabled-record", OmScope::Session, &scope_key, now);
    om_record.session_id = Some("s-om-disabled".to_string());
    om_record.active_observations = "hidden hint".to_string();
    om_record.observation_token_count = 42;
    om_record.observer_trigger_count_total = 1;
    om_record.reflector_trigger_count_total = 1;
    app.state
        .upsert_om_record(&om_record)
        .expect("upsert om record");

    let state = app
        .fetch_session_om_state_with_enabled("s-om-disabled", false)
        .expect("state");
    assert!(state.is_none());
}

#[test]
fn fetch_session_om_state_falls_back_to_recent_non_session_scope_record() {
    let (_temp, app) = setup_test_app();

    let now = Utc::now();
    let mut resource_record = base_om_record(
        "om-fallback-resource-record",
        OmScope::Resource,
        "resource:r-fallback",
        now,
    );
    resource_record.resource_id = Some("r-fallback".to_string());
    resource_record.generation_count = 1;
    resource_record.active_observations = "resource scoped observation".to_string();
    resource_record.observation_token_count = 30;
    resource_record.observer_trigger_count_total = 1;
    resource_record.reflector_trigger_count_total = 1;

    let mut thread_record = base_om_record(
        "om-fallback-thread-record",
        OmScope::Thread,
        "thread:t-fallback",
        now,
    );
    thread_record.thread_id = Some("t-fallback".to_string());
    thread_record.resource_id = Some("r-fallback".to_string());
    thread_record.generation_count = 1;
    thread_record.active_observations = "thread scoped observation".to_string();
    thread_record.observation_token_count = 45;
    thread_record.observer_trigger_count_total = 3;
    thread_record.reflector_trigger_count_total = 2;

    app.state
        .upsert_om_record(&resource_record)
        .expect("upsert resource record");
    app.state
        .upsert_om_record(&thread_record)
        .expect("upsert thread record");

    app.state
        .upsert_om_scope_session("resource:r-fallback", "s-fallback")
        .expect("map resource scope");
    std::thread::sleep(std::time::Duration::from_millis(2));
    app.state
        .upsert_om_scope_session("thread:t-fallback", "s-fallback")
        .expect("map thread scope");
    app.state
        .upsert_om_thread_state(
            "thread:t-fallback",
            "t-fallback",
            Some(now),
            None,
            Some("reply from thread fallback"),
        )
        .expect("upsert thread state");

    let state = app
        .fetch_session_om_state_with_enabled("s-fallback", true)
        .expect("state")
        .expect("state missing");
    assert_eq!(state.observation_tokens_active, 45);
    assert_eq!(state.observer_trigger_count_total, 3);
    assert_eq!(state.reflector_trigger_count_total, 2);
    assert!(
        state
            .hint
            .as_deref()
            .is_some_and(|value| value.contains("thread scoped observation"))
    );
}

#[test]
fn search_filters_activated_message_ids_from_recent_hints_ephemerally() {
    let (_temp, app) = setup_test_app();
    upsert_records(
        &app,
        &[
            resources_root_record("root-om-filter"),
            resources_leaf_record(
                "leaf-om-filter",
                "om-filter.md",
                "om",
                "oauth memory retrieval note",
                &["om"],
            ),
        ],
    );

    let session = app.session(Some("s-om-filter"));
    session.load().expect("session load");
    let first = session
        .add_message("user", "first hint should remain")
        .expect("append first");
    let second = session
        .add_message("user", "second hint should be filtered")
        .expect("append second");

    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-filter"), None, None).expect("scope key");
    let now = Utc::now();
    let mut om_record = base_om_record("om-filter-record", OmScope::Session, &scope_key, now);
    om_record.session_id = Some("s-om-filter".to_string());
    om_record.active_observations = "latest observation".to_string();
    om_record.observation_token_count = 20;
    om_record.last_activated_message_ids = vec![second.id];
    app.state
        .upsert_om_record(&om_record)
        .expect("upsert om record");

    app.search(
        "oauth",
        Some("axiom://resources"),
        Some("s-om-filter"),
        Some(5),
        None,
        None,
    )
    .expect("search");

    let logs = app
        .list_request_logs_filtered(5, Some("search"), Some("ok"))
        .expect("request logs");
    let entry = logs.first().expect("latest search log");
    let details = entry.details.as_ref().expect("search details");
    let expected_tokens = {
        let first_chars = u64::try_from(first.text.chars().count()).unwrap_or(u64::MAX);
        let second_chars = u64::try_from(second.text.chars().count()).unwrap_or(u64::MAX);
        first_chars.div_ceil(4) + second_chars.div_ceil(4)
    };
    assert_eq!(
        details
            .get("context_tokens_before_om")
            .and_then(serde_json::Value::as_u64),
        Some(expected_tokens)
    );
    assert_eq!(
        details
            .get("observation_tokens_active")
            .and_then(serde_json::Value::as_u64),
        Some(20)
    );
}

#[test]
fn merge_recent_and_om_hints_reserves_slot_for_om_by_policy() {
    let recent = vec![
        "recent-1".to_string(),
        "recent-2".to_string(),
        "recent-3".to_string(),
    ];
    let policy = OmHintPolicy {
        context_max_archives: 2,
        context_max_messages: 8,
        recent_hint_limit: 3,
        total_hint_limit: 2,
        keep_recent_with_om: 1,
    };

    let merged = merge_recent_and_om_hints(&recent, Some("om: compact"), policy);
    assert_eq!(
        merged,
        vec!["recent-1".to_string(), "om: compact".to_string()]
    );
}

#[test]
fn merge_recent_and_om_hints_without_om_uses_total_limit() {
    let recent = vec![
        "recent-1".to_string(),
        "recent-2".to_string(),
        "recent-3".to_string(),
    ];
    let policy = OmHintPolicy {
        context_max_archives: 2,
        context_max_messages: 8,
        recent_hint_limit: 3,
        total_hint_limit: 2,
        keep_recent_with_om: 1,
    };

    let merged = merge_recent_and_om_hints(&recent, None, policy);
    assert_eq!(merged, vec!["recent-1".to_string(), "recent-2".to_string()]);
}

#[test]
fn normalize_runtime_hints_trims_dedups_and_caps_chars() {
    let hints = vec![
        RuntimeHint {
            kind: RuntimeHintKind::Observation,
            text: "  alpha   beta  ".to_string(),
            source: Some("episodic".to_string()),
        },
        RuntimeHint {
            kind: RuntimeHintKind::CurrentTask,
            text: "alpha beta".to_string(),
            source: Some("episodic".to_string()),
        },
        RuntimeHint {
            kind: RuntimeHintKind::SuggestedResponse,
            text: "0123456789".to_string(),
            source: None,
        },
    ];

    let normalized = normalize_runtime_hints(&hints, 2, 5);
    assert_eq!(normalized, vec!["alpha".to_string(), "01234".to_string()]);
}

#[test]
fn merge_runtime_om_recent_hints_preserves_om_slot_and_recent_reservation() {
    let runtime = vec![
        "runtime-1".to_string(),
        "runtime-2".to_string(),
        "runtime-3".to_string(),
    ];
    let recent = vec![
        "recent-1".to_string(),
        "recent-2".to_string(),
        "recent-3".to_string(),
    ];
    let policy = OmHintPolicy {
        context_max_archives: 2,
        context_max_messages: 8,
        recent_hint_limit: 3,
        total_hint_limit: 4,
        keep_recent_with_om: 1,
    };

    let merged = merge_runtime_om_recent_hints(&runtime, Some("om: compact"), &recent, policy, 256);
    assert_eq!(
        merged,
        vec![
            "recent-1".to_string(),
            "om: compact".to_string(),
            "runtime-1".to_string(),
            "runtime-2".to_string(),
        ]
    );
}

#[test]
fn search_with_runtime_hints_has_no_message_or_outbox_side_effect() {
    let (temp, app) = setup_test_app();
    upsert_records(
        &app,
        &[
            resources_root_record("root-runtime-hint"),
            resources_leaf_record(
                "leaf-runtime-hint",
                "runtime-hint.md",
                "runtime hint",
                "context from runtime hint",
                &["runtime"],
            ),
        ],
    );
    let before_outbox = app
        .state
        .fetch_outbox("new", 100)
        .expect("outbox before")
        .len();

    let result = app
        .search_with_request(SearchRequest {
            query: "runtime".to_string(),
            target_uri: Some("axiom://resources".to_string()),
            session: None,
            limit: Some(5),
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            budget: None,
            runtime_hints: vec![RuntimeHint {
                kind: RuntimeHintKind::Observation,
                text: "ephemeral runtime hint".to_string(),
                source: Some("episodic".to_string()),
            }],
        })
        .expect("search");
    assert!(!result.query_results.is_empty());

    let after_outbox = app
        .state
        .fetch_outbox("new", 100)
        .expect("outbox after")
        .len();
    assert_eq!(before_outbox, after_outbox);

    assert_eq!(
        count_session_message_files(temp.path()),
        0,
        "runtime hints should not create session message files"
    );
}

#[test]
fn search_with_runtime_hints_without_session_uses_runtime_query_not_memory_focus() {
    let (_temp, app) = setup_test_app();
    upsert_records(
        &app,
        &[
            resources_root_record("root-runtime-plan"),
            resources_leaf_record(
                "leaf-runtime-plan",
                "runtime-plan.md",
                "runtime plan",
                "runtime hints must not imply session memory focus",
                &["runtime"],
            ),
        ],
    );

    let result = app
        .search_with_request(SearchRequest {
            query: "runtime".to_string(),
            target_uri: None,
            session: None,
            limit: Some(5),
            score_threshold: None,
            min_match_tokens: None,
            filter: None,
            budget: None,
            runtime_hints: vec![RuntimeHint {
                kind: RuntimeHintKind::Observation,
                text: "ephemeral preference hint".to_string(),
                source: Some("episodic".to_string()),
            }],
        })
        .expect("search");

    let kinds = result
        .query_plan
        .typed_queries
        .iter()
        .map(|query| query.kind.as_str())
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"runtime_hints"));
    assert!(!kinds.contains(&"memory_focus"));
}

#[test]
fn merge_observation_hint_with_suggested_response_appends_next_hint() {
    let merged = merge_observation_hint_with_suggested_response(
        Some("om: latest observation".to_string()),
        Some("ask user to confirm oauth scope"),
        64,
    );
    assert_eq!(
        merged.as_deref(),
        Some("om: latest observation | next: ask user to confirm oauth scope")
    );
}

#[test]
fn merge_observation_hint_with_suggested_response_uses_next_only_when_base_missing() {
    let merged = merge_observation_hint_with_suggested_response(
        None,
        Some("continue with migration step"),
        64,
    );
    assert_eq!(
        merged.as_deref(),
        Some("om: next: continue with migration step")
    );
}

fn count_session_message_files(root: &Path) -> usize {
    let session_root = root.join("session");
    if !session_root.exists() {
        return 0;
    }
    let mut count = 0usize;
    let Ok(entries) = fs::read_dir(&session_root) else {
        return 0;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let messages_path = entry.path().join("messages.jsonl");
        if messages_path.exists() {
            count = count.saturating_add(1);
        }
    }
    count
}

fn search_options(query: &str) -> SearchOptions {
    SearchOptions {
        query: query.to_string(),
        target_uri: Some(crate::uri::AxiomUri::root(crate::uri::Scope::Resources)),
        session: None,
        session_hints: Vec::new(),
        budget: None,
        limit: 5,
        score_threshold: None,
        min_match_tokens: None,
        filter: None,
        request_type: "search".to_string(),
    }
}

fn setup_test_app() -> (TempDir, AxiomMe) {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");
    (temp, app)
}

fn upsert_records(app: &AxiomMe, records: &[IndexRecord]) {
    {
        let mut index = app.index.write().expect("index write");
        for record in records {
            index.upsert(record.clone());
        }
    }
    for record in records {
        app.state
            .upsert_search_document(record)
            .expect("upsert record");
    }
}

fn resources_root_record(id: &str) -> IndexRecord {
    IndexRecord {
        id: id.to_string(),
        uri: "axiom://resources".to_string(),
        parent_uri: None,
        is_leaf: false,
        context_type: "resource".to_string(),
        name: "resources".to_string(),
        abstract_text: "resources root".to_string(),
        content: "resources root".to_string(),
        tags: Vec::new(),
        updated_at: Utc::now(),
        depth: 0,
    }
}

fn resources_leaf_record(
    id: &str,
    name: &str,
    abstract_text: &str,
    content: &str,
    tags: &[&str],
) -> IndexRecord {
    IndexRecord {
        id: id.to_string(),
        uri: format!("axiom://resources/{name}"),
        parent_uri: Some("axiom://resources".to_string()),
        is_leaf: true,
        context_type: "resource".to_string(),
        name: name.to_string(),
        abstract_text: abstract_text.to_string(),
        content: content.to_string(),
        tags: tags.iter().map(ToString::to_string).collect::<Vec<_>>(),
        updated_at: Utc::now(),
        depth: 1,
    }
}

fn base_om_record(id: &str, scope: OmScope, scope_key: &str, now: DateTime<Utc>) -> OmRecord {
    OmRecord {
        id: id.to_string(),
        scope,
        scope_key: scope_key.to_string(),
        session_id: None,
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: String::new(),
        observation_token_count: 0,
        pending_message_tokens: 0,
        last_observed_at: Some(now),
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
    }
}

fn hit(uri: &str, score: f32) -> ContextHit {
    ContextHit {
        uri: uri.to_string(),
        score,
        abstract_text: String::new(),
        context_type: "resource".to_string(),
        relations: Vec::new(),
    }
}

fn sample_find_result(hits: Vec<ContextHit>) -> FindResult {
    FindResult {
        memories: Vec::new(),
        resources: hits.clone(),
        skills: Vec::new(),
        query_plan: QueryPlan::default(),
        query_results: hits,
        trace: None,
        trace_uri: None,
    }
}
