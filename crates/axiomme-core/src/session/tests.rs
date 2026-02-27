use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use chrono::{Duration, Utc};
use rusqlite::{Connection, params};
use tempfile::tempdir;

use crate::fs::LocalContextFs;
use crate::index::InMemoryIndex;
use crate::models::{
    CommitMode, MemoryCategory, MemoryPromotionFact, MemoryPromotionRequest, Message,
    QueueEventStatus,
    PromotionApplyMode,
};
use crate::om::{OmOriginType, OmRecord, OmScope, build_scope_key};
use crate::state::{PromotionCheckpointPhase, SqliteStateStore};
use crate::uri::AxiomUri;
use crate::{
    AxiomError, AxiomMe,
    error::{OmInferenceFailureKind, OmInferenceSource},
};

use super::Session;
use super::memory::{extract_memories, stable_text_key};

fn fixture_categories(role: &str, text: &str) -> HashSet<String> {
    extract_memories(&[Message {
        id: "fixture-msg-001".to_string(),
        role: role.to_string(),
        text: text.to_string(),
        created_at: Utc::now(),
    }])
    .into_iter()
    .map(|candidate| candidate.category)
    .collect::<HashSet<_>>()
}

fn required_payload_u32(payload: &serde_json::Value, key: &str, context: &str) -> u32 {
    payload
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_else(|| panic!("{context}: missing or invalid {key}"))
}

#[test]
fn commit_extracts_preference_memory() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s1", fs.clone(), state, index);
    session.load().expect("load failed");
    session
        .add_message("user", "I prefer concise Rust code.")
        .expect("append failed");
    let result = session.commit().expect("commit failed");

    assert!(result.archived);
    assert!(result.memories_extracted >= 1);

    let pref_uri = AxiomUri::parse("axiom://user/memories/preferences/pref-item.md")
        .unwrap_or_else(|_| AxiomUri::parse("axiom://user/memories/preferences").expect("uri"));
    let pref_parent = pref_uri
        .parent()
        .unwrap_or_else(|| AxiomUri::parse("axiom://user/memories/preferences").expect("uri2"));
    assert!(fs.exists(&pref_parent));
}

#[test]
fn commit_mode_archive_only_skips_auto_extraction() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-archive-only", fs.clone(), state, index);
    session.load().expect("load failed");
    session
        .add_message("user", "I prefer archive-only checkpoint flow.")
        .expect("append failed");

    let result = session
        .commit_with_mode(CommitMode::ArchiveOnly)
        .expect("commit archive only");
    assert!(result.archived);
    assert_eq!(result.memories_extracted, 0);

    let preferences_uri =
        AxiomUri::parse("axiom://user/memories/preferences").expect("preferences uri");
    assert!(
        !fs.exists(&preferences_uri),
        "archive-only should not extract durable memories"
    );
}

fn promotion_request(
    session_id: &str,
    checkpoint_id: &str,
    apply_mode: PromotionApplyMode,
    facts: Vec<MemoryPromotionFact>,
) -> MemoryPromotionRequest {
    MemoryPromotionRequest {
        session_id: session_id.to_string(),
        checkpoint_id: checkpoint_id.to_string(),
        apply_mode,
        facts,
    }
}

fn promotion_fact(
    category: MemoryCategory,
    text: &str,
    source_ids: &[&str],
) -> MemoryPromotionFact {
    MemoryPromotionFact {
        category,
        text: text.to_string(),
        source_message_ids: source_ids.iter().copied().map(str::to_string).collect(),
        source: Some("episodic".to_string()),
        confidence_milli: 800,
    }
}

fn promotion_checkpoint_payload(request: &MemoryPromotionRequest) -> (String, String) {
    let facts_json = request
        .facts
        .iter()
        .map(|fact| {
            serde_json::json!({
                "category": fact.category.as_str(),
                "text": fact.text,
                "source_message_ids": fact.source_message_ids,
                "source": fact.source,
                "confidence_milli": fact.confidence_milli,
            })
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "session_id": request.session_id,
        "checkpoint_id": request.checkpoint_id,
        "apply_mode": match request.apply_mode {
            PromotionApplyMode::AllOrNothing => "all_or_nothing",
            PromotionApplyMode::BestEffort => "best_effort",
        },
        "facts": facts_json,
    });
    let request_json = serde_json::to_string(&payload).expect("serialize request payload");
    let request_hash = blake3::hash(request_json.as_bytes()).to_hex().to_string();
    (request_json, request_hash)
}

#[test]
fn promotion_rejects_invalid_category_and_empty_text() {
    let invalid_payload = serde_json::json!({
        "session_id": "s-invalid-category",
        "checkpoint_id": "cp-invalid-category",
        "apply_mode": "all_or_nothing",
        "facts": [
            {
                "category": "unsupported",
                "text": "invalid category",
                "source_message_ids": ["m-1"],
                "source": "episodic",
                "confidence_milli": 700
            }
        ]
    });
    let decoded = serde_json::from_value::<MemoryPromotionRequest>(invalid_payload);
    assert!(decoded.is_err(), "invalid category must fail deserialize");

    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-empty-text", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-empty-text",
        "cp-empty-text",
        PromotionApplyMode::AllOrNothing,
        vec![MemoryPromotionFact {
            category: MemoryCategory::Preferences,
            text: "   ".to_string(),
            source_message_ids: vec!["m-1".to_string()],
            source: Some("episodic".to_string()),
            confidence_milli: 700,
        }],
    );
    let err = session
        .promote_memories(&request)
        .expect_err("empty text must fail validation");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn promotion_idempotent_on_same_checkpoint_and_same_facts() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-idempotent", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-idempotent",
        "cp-idempotent",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Preferences,
            "Always keep promotion idempotent",
            &["m-1"],
        )],
    );
    let first = session.promote_memories(&request).expect("first promotion");
    let second = session
        .promote_memories(&request)
        .expect("second promotion");
    assert_eq!(first, second);
    assert_eq!(second.persisted, 1);
}

#[test]
fn promotion_all_or_nothing_restores_snapshots_on_in_process_write_failure() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-rollback", fs.clone(), state, index);
    session.load().expect("load");

    let success_text = "alpha rollback candidate";
    let fail_text = "zeta rollback blocker";
    let success_suffix = stable_text_key(success_text);
    let fail_suffix = stable_text_key(fail_text);
    let success_uri = AxiomUri::parse(&format!(
        "axiom://agent/memories/patterns/pattern-{success_suffix}.md"
    ))
    .expect("success uri");
    let fail_uri = AxiomUri::parse(&format!(
        "axiom://agent/memories/patterns/pattern-{fail_suffix}.md"
    ))
    .expect("fail uri");
    let success_path = fs.resolve_uri(&success_uri);
    let fail_path = fs.resolve_uri(&fail_uri);

    std::fs::create_dir_all(&fail_path).expect("create blocking directory");
    assert!(fail_path.is_dir(), "blocking path must be a directory");

    let request = promotion_request(
        "s-promote-rollback",
        "cp-rollback",
        PromotionApplyMode::AllOrNothing,
        vec![
            promotion_fact(MemoryCategory::Patterns, success_text, &["m-1"]),
            promotion_fact(MemoryCategory::Patterns, fail_text, &["m-2"]),
        ],
    );
    let err = session
        .promote_memories(&request)
        .expect_err("second write must fail");
    assert!(matches!(err, AxiomError::Io(_)));
    assert!(
        !success_path.exists(),
        "successful first write must be rolled back after second write failure"
    );

    let checkpoint = session
        .state
        .get_promotion_checkpoint("s-promote-rollback", "cp-rollback")
        .expect("read checkpoint")
        .expect("checkpoint must exist");
    assert_eq!(checkpoint.phase, PromotionCheckpointPhase::Pending);
}

#[test]
fn promotion_same_checkpoint_same_hash_returns_cached_result() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-cached", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-cached",
        "cp-1",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Preferences,
            "Always use deterministic CI gates",
            &["m-1"],
        )],
    );
    let first = session.promote_memories(&request).expect("first promotion");
    let second = session
        .promote_memories(&request)
        .expect("second promotion");
    assert_eq!(first, second);
    assert_eq!(second.persisted, 1);
}

#[test]
fn promotion_same_checkpoint_different_hash_rejected() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-conflict", fs, state, index);
    session.load().expect("load");

    let first = promotion_request(
        "s-promote-conflict",
        "cp-conflict",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Events,
            "Phase one finished on Tuesday",
            &["m-1"],
        )],
    );
    session.promote_memories(&first).expect("first promotion");

    let second = promotion_request(
        "s-promote-conflict",
        "cp-conflict",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Events,
            "Phase two finished on Wednesday",
            &["m-2"],
        )],
    );
    let err = session
        .promote_memories(&second)
        .expect_err("second must conflict");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn promotion_same_checkpoint_same_hash_pending_reconciles_and_applies_once() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-pending", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-pending",
        "cp-pending",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Events,
            "Pending checkpoint is replayed exactly once",
            &["m-1"],
        )],
    );
    let (request_json, request_hash) = promotion_checkpoint_payload(&request);
    session
        .state
        .insert_promotion_checkpoint_pending(
            "s-promote-pending",
            "cp-pending",
            request_hash.as_str(),
            request_json.as_str(),
        )
        .expect("insert pending");

    let first = session
        .promote_memories(&request)
        .expect("first reconcile apply");
    let second = session
        .promote_memories(&request)
        .expect("second replay from applied checkpoint");
    assert_eq!(first, second);
    assert_eq!(first.persisted, 1);

    let checkpoint = session
        .state
        .get_promotion_checkpoint("s-promote-pending", "cp-pending")
        .expect("load checkpoint")
        .expect("checkpoint record");
    assert_eq!(checkpoint.phase, PromotionCheckpointPhase::Applied);
    assert_eq!(
        checkpoint.attempt_count, 1,
        "pending checkpoint should transition to applying once"
    );
}

#[test]
fn promotion_pending_checkpoint_replay_detects_request_json_hash_mismatch() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-replay-guard", fs, state, index);
    session.load().expect("load");

    let incoming = promotion_request(
        "s-promote-replay-guard",
        "cp-replay-guard",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Events,
            "incoming request payload",
            &["m-1"],
        )],
    );
    let (_, incoming_hash) = promotion_checkpoint_payload(&incoming);

    let tampered = promotion_request(
        "s-promote-replay-guard",
        "cp-replay-guard",
        PromotionApplyMode::BestEffort,
        vec![promotion_fact(
            MemoryCategory::Events,
            "tampered checkpoint payload",
            &["m-2"],
        )],
    );
    let (tampered_json, _) = promotion_checkpoint_payload(&tampered);

    session
        .state
        .insert_promotion_checkpoint_pending(
            "s-promote-replay-guard",
            "cp-replay-guard",
            incoming_hash.as_str(),
            tampered_json.as_str(),
        )
        .expect("insert pending checkpoint");

    let err = session
        .promote_memories(&incoming)
        .expect_err("must detect checkpoint request_json/hash mismatch");
    match err {
        AxiomError::Internal(message) => {
            assert!(message.contains("request_json hash mismatch"));
        }
        other => panic!("expected internal mismatch error, got {other}"),
    }
}

#[test]
fn promotion_request_hash_canonicalization_is_order_independent() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-order", fs, state, index);
    session.load().expect("load");

    let request_a = promotion_request(
        "s-promote-order",
        "cp-order",
        PromotionApplyMode::AllOrNothing,
        vec![
            promotion_fact(MemoryCategory::Cases, "Root cause captured", &["m-a"]),
            promotion_fact(MemoryCategory::Patterns, "Run benchmark gate", &["m-b"]),
        ],
    );
    let request_b = promotion_request(
        "s-promote-order",
        "cp-order",
        PromotionApplyMode::AllOrNothing,
        vec![
            promotion_fact(MemoryCategory::Patterns, "Run benchmark gate", &["m-b"]),
            promotion_fact(MemoryCategory::Cases, "Root cause captured", &["m-a"]),
        ],
    );

    let first = session.promote_memories(&request_a).expect("first");
    let second = session.promote_memories(&request_b).expect("second");
    assert_eq!(first, second);
}

#[test]
fn promotion_rejects_out_of_bounds_facts_before_apply() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-bounds", fs, state, index);
    session.load().expect("load");

    let oversized = "x".repeat(513);
    let request = promotion_request(
        "s-promote-bounds",
        "cp-bounds",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Preferences,
            oversized.as_str(),
            &["m-1"],
        )],
    );
    let err = session
        .promote_memories(&request)
        .expect_err("must reject oversized text");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn promotion_rejects_empty_text_in_all_or_nothing() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-empty", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-empty",
        "cp-empty",
        PromotionApplyMode::AllOrNothing,
        vec![MemoryPromotionFact {
            category: MemoryCategory::Preferences,
            text: "    ".to_string(),
            source_message_ids: vec!["m-1".to_string()],
            source: Some("episodic".to_string()),
            confidence_milli: 700,
        }],
    );
    let err = session
        .promote_memories(&request)
        .expect_err("must reject empty text");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn promotion_best_effort_persists_valid_and_reports_rejected() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-best-effort", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-best-effort",
        "cp-best-effort",
        PromotionApplyMode::BestEffort,
        vec![
            promotion_fact(
                MemoryCategory::Patterns,
                "Always run deterministic smoke tests first",
                &["m-valid"],
            ),
            MemoryPromotionFact {
                category: MemoryCategory::Patterns,
                text: " ".to_string(),
                source_message_ids: vec!["m-invalid".to_string()],
                source: Some("episodic".to_string()),
                confidence_milli: 600,
            },
        ],
    );
    let result = session
        .promote_memories(&request)
        .expect("best effort promotion");
    assert_eq!(result.accepted, 1);
    assert_eq!(result.persisted, 1);
    assert_eq!(result.rejected, 1);
}

#[test]
fn promotion_all_or_nothing_reindex_failure_rolls_back_and_stays_pending() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new(
        "s-promote-aon-reindex-fail",
        fs.clone(),
        state,
        Arc::clone(&index),
    );
    session.load().expect("load");

    let text = "All or nothing reindex rollback target";
    let suffix = stable_text_key(text);
    let uri = AxiomUri::parse(&format!(
        "axiom://agent/memories/patterns/pattern-{suffix}.md"
    ))
    .expect("uri");
    let path = fs.resolve_uri(&uri);

    let poisoned = std::thread::spawn(move || {
        let _guard = index.write().expect("lock");
        panic!("poison index lock for reindex failure");
    })
    .join();
    assert!(poisoned.is_err());

    let request = promotion_request(
        "s-promote-aon-reindex-fail",
        "cp-aon-reindex-fail",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(MemoryCategory::Patterns, text, &["m-1"])],
    );

    let err = session
        .promote_memories(&request)
        .expect_err("reindex failure must error");
    match err {
        AxiomError::Internal(message) => {
            assert!(message.contains("all_or_nothing reindex failed"));
        }
        other => panic!("expected internal reindex error, got {other}"),
    }
    assert!(!path.exists(), "rollback must remove persisted file");

    let checkpoint = session
        .state
        .get_promotion_checkpoint("s-promote-aon-reindex-fail", "cp-aon-reindex-fail")
        .expect("read checkpoint")
        .expect("checkpoint");
    assert_eq!(checkpoint.phase, PromotionCheckpointPhase::Pending);
}

#[test]
fn promotion_best_effort_reindex_failure_rolls_back_and_stays_pending() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new(
        "s-promote-best-effort-reindex-fail",
        fs.clone(),
        state,
        Arc::clone(&index),
    );
    session.load().expect("load");

    let valid_text = "Always run deterministic smoke tests first";
    let valid_suffix = stable_text_key(valid_text);
    let valid_uri = AxiomUri::parse(&format!(
        "axiom://agent/memories/patterns/pattern-{valid_suffix}.md"
    ))
    .expect("valid uri");
    let valid_path = fs.resolve_uri(&valid_uri);

    let poisoned = std::thread::spawn(move || {
        let _guard = index.write().expect("lock");
        panic!("poison index lock for reindex failure");
    })
    .join();
    assert!(poisoned.is_err());

    let request = promotion_request(
        "s-promote-best-effort-reindex-fail",
        "cp-best-effort-reindex-fail",
        PromotionApplyMode::BestEffort,
        vec![
            promotion_fact(MemoryCategory::Patterns, valid_text, &["m-valid"]),
            MemoryPromotionFact {
                category: MemoryCategory::Patterns,
                text: " ".to_string(),
                source_message_ids: vec!["m-invalid".to_string()],
                source: Some("episodic".to_string()),
                confidence_milli: 600,
            },
        ],
    );

    let err = session
        .promote_memories(&request)
        .expect_err("reindex failure must error");
    match err {
        AxiomError::Internal(message) => {
            assert!(message.contains("best_effort reindex failed"));
        }
        other => panic!("expected internal reindex error, got {other}"),
    }
    assert!(
        !valid_path.exists(),
        "best-effort rollback must remove persisted file on reindex failure"
    );

    let checkpoint = session
        .state
        .get_promotion_checkpoint(
            "s-promote-best-effort-reindex-fail",
            "cp-best-effort-reindex-fail",
        )
        .expect("read checkpoint")
        .expect("checkpoint");
    assert_eq!(checkpoint.phase, PromotionCheckpointPhase::Pending);
}

#[test]
fn promotion_all_or_nothing_counts_persisted_facts_even_for_single_target_file() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-profile-count", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-profile-count",
        "cp-profile-count",
        PromotionApplyMode::AllOrNothing,
        vec![
            promotion_fact(MemoryCategory::Profile, "Primary role: operator", &["m-1"]),
            promotion_fact(
                MemoryCategory::Profile,
                "Primary preference: explicit checkpoints",
                &["m-2"],
            ),
        ],
    );
    let result = session
        .promote_memories(&request)
        .expect("all-or-nothing promotion");
    assert_eq!(result.accepted, 2);
    assert_eq!(result.persisted, 2);
    assert_eq!(result.rejected, 0);
}

#[test]
fn promotion_same_checkpoint_same_hash_applying_returns_retryable_busy() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-applying", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-applying",
        "cp-applying",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Events,
            "Apply in progress checkpoint",
            &["m-1"],
        )],
    );
    let (request_json, request_hash) = promotion_checkpoint_payload(&request);
    session
        .state
        .insert_promotion_checkpoint_pending(
            "s-promote-applying",
            "cp-applying",
            request_hash.as_str(),
            request_json.as_str(),
        )
        .expect("insert pending");
    assert!(
        session
            .state
            .claim_promotion_checkpoint_applying(
                "s-promote-applying",
                "cp-applying",
                request_hash.as_str(),
            )
            .expect("claim applying")
    );

    let err = session
        .promote_memories(&request)
        .expect_err("must report checkpoint busy");
    match err {
        AxiomError::Conflict(message) => {
            assert!(message.contains("checkpoint_busy"));
        }
        other => panic!("expected conflict, got {other}"),
    }
}

#[test]
fn promotion_concurrent_claim_has_single_cas_winner() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-claim", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-claim",
        "cp-claim",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Cases,
            "single winner for pending->applying",
            &["m-1"],
        )],
    );
    let (request_json, request_hash) = promotion_checkpoint_payload(&request);
    session
        .state
        .insert_promotion_checkpoint_pending(
            "s-promote-claim",
            "cp-claim",
            request_hash.as_str(),
            request_json.as_str(),
        )
        .expect("insert pending");

    assert!(
        session
            .state
            .claim_promotion_checkpoint_applying(
                "s-promote-claim",
                "cp-claim",
                request_hash.as_str()
            )
            .expect("first claim")
    );
    assert!(
        !session
            .state
            .claim_promotion_checkpoint_applying(
                "s-promote-claim",
                "cp-claim",
                request_hash.as_str()
            )
            .expect("second claim"),
        "only one claimant can transition pending->applying"
    );
}

#[test]
#[ignore = "manual benchmark gate; run explicitly for perf baseline verification"]
fn promotion_and_commit_latency_regression_within_budget() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root");
    let script_path = repo_root.join("scripts/perf_regression_gate.sh");
    let status = std::process::Command::new("bash")
        .arg(&script_path)
        .args([
            "--window-size",
            "1",
            "--required-passes",
            "1",
            "--min-cases",
            "12",
        ])
        .status()
        .expect("run performance gate script");
    assert!(
        status.success(),
        "performance gate command failed: {}",
        script_path.display()
    );
}

#[test]
fn promotion_stale_applying_reconcile_replays_deterministically() {
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("state.db");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(&db_path).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-promote-stale", fs, state, index);
    session.load().expect("load");

    let request = promotion_request(
        "s-promote-stale",
        "cp-stale",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Cases,
            "Stale checkpoint should replay exactly once",
            &["m-1"],
        )],
    );
    let (request_json, request_hash) = promotion_checkpoint_payload(&request);
    session
        .state
        .insert_promotion_checkpoint_pending(
            "s-promote-stale",
            "cp-stale",
            request_hash.as_str(),
            request_json.as_str(),
        )
        .expect("insert pending");
    assert!(
        session
            .state
            .claim_promotion_checkpoint_applying(
                "s-promote-stale",
                "cp-stale",
                request_hash.as_str()
            )
            .expect("claim applying")
    );

    let stale_time = (Utc::now() - Duration::seconds(180)).to_rfc3339();
    let conn = Connection::open(&db_path).expect("open sqlite");
    conn.execute(
        r"
        UPDATE memory_promotion_checkpoints
        SET updated_at = ?1
        WHERE session_id = ?2 AND checkpoint_id = ?3
        ",
        params![stale_time, "s-promote-stale", "cp-stale"],
    )
    .expect("mark stale");

    let result = session
        .promote_memories(&request)
        .expect("replay stale checkpoint");
    assert_eq!(result.persisted, 1);

    let checkpoint = session
        .state
        .get_promotion_checkpoint("s-promote-stale", "cp-stale")
        .expect("load checkpoint")
        .expect("checkpoint record");
    assert_eq!(checkpoint.phase, PromotionCheckpointPhase::Applied);
}

#[test]
fn delete_session_cleans_promotion_checkpoints() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let session = app.session(Some("s-delete-promotion"));
    session.load().expect("load");
    let first_message = session
        .add_message("user", "checkpoint cleanup validation")
        .expect("add message");
    let first = promotion_request(
        "s-delete-promotion",
        "cp-cleanup",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Preferences,
            "Remember delete cleanup contract",
            &[first_message.id.as_str()],
        )],
    );
    app.promote_session_memories(&first)
        .expect("first promotion");

    assert!(app.delete("s-delete-promotion").expect("delete session"));

    let session = app.session(Some("s-delete-promotion"));
    session.load().expect("reload");
    let second_message = session
        .add_message("user", "new lifecycle after delete")
        .expect("add message");
    let second = promotion_request(
        "s-delete-promotion",
        "cp-cleanup",
        PromotionApplyMode::AllOrNothing,
        vec![promotion_fact(
            MemoryCategory::Preferences,
            "Checkpoint rows were removed on delete",
            &[second_message.id.as_str()],
        )],
    );
    let replay = app
        .promote_session_memories(&second)
        .expect("promotion should not conflict after delete");
    assert_eq!(replay.persisted, 1);
}

#[test]
fn load_returns_error_for_invalid_session_id() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("../bad-session", fs, state, index);
    let err = session.load().expect_err("must fail");
    assert!(matches!(err, crate::error::AxiomError::PathTraversal(_)));
}

#[test]
fn context_for_search_includes_relevant_archive_messages() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-archive", fs, state, index);
    session.load().expect("load failed");
    session
        .add_message("user", "OAuth refresh token strategy")
        .expect("append failed");
    session.commit().expect("commit failed");

    let no_archive = session
        .get_context_for_search("oauth", 0, 8)
        .expect("ctx without archive");
    assert!(no_archive.recent_messages.is_empty());

    let with_archive = session
        .get_context_for_search("oauth", 1, 8)
        .expect("ctx with archive");
    assert!(
        with_archive
            .recent_messages
            .iter()
            .any(|m| m.text.contains("OAuth refresh token strategy"))
    );
}

#[test]
fn output_stage_contract_skips_save_when_read_only() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-output-readonly", fs, state, index);
    session.load().expect("load failed");

    let message = Message {
        id: "m-out-readonly".to_string(),
        role: "user".to_string(),
        text: "readonly should skip save".to_string(),
        created_at: Utc::now(),
    };
    let saved = session
        .persist_output_stage_messages(std::slice::from_ref(&message), true)
        .expect("persist");
    assert_eq!(saved, 0);
    assert!(session.read_messages().expect("read messages").is_empty());
}

#[test]
fn output_stage_contract_skips_save_when_no_messages() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-output-empty", fs, state, index);
    session.load().expect("load failed");

    let saved = session
        .persist_output_stage_messages(&[], false)
        .expect("persist");
    assert_eq!(saved, 0);
    assert!(session.read_messages().expect("read messages").is_empty());
}

#[test]
fn output_stage_contract_saves_when_writable_and_non_empty() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session = Session::new("s-output-write", fs, state, index);
    session.load().expect("load failed");

    let message = Message {
        id: "m-out-write".to_string(),
        role: "assistant".to_string(),
        text: "writable save".to_string(),
        created_at: Utc::now(),
    };
    let saved = session
        .persist_output_stage_messages(std::slice::from_ref(&message), false)
        .expect("persist");
    assert_eq!(saved, 1);

    let messages = session.read_messages().expect("read messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "m-out-write");
    assert_eq!(messages[0].text, "writable save");
}

#[test]
fn om_write_path_accepts_explicit_session_scope_binding() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-explicit-session", fs, state, index)
        .with_om_scope(OmScope::Session, None, None)
        .expect("scope");
    session.load().expect("load failed");
    session
        .add_message("user", "x".repeat(26_000))
        .expect("append");

    let record = session
        .state
        .get_om_record_by_scope_key("session:s-explicit-session")
        .expect("fetch om")
        .expect("om record");
    assert_eq!(record.scope, OmScope::Session);

    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observe_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("observe event");
    assert_eq!(
        observe_event
            .payload_json
            .get("scope_key")
            .and_then(serde_json::Value::as_str),
        Some("session:s-explicit-session")
    );
}

#[test]
fn om_write_path_accepts_explicit_thread_scope_binding() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-explicit-thread", fs, state, index)
        .with_om_scope(
            OmScope::Thread,
            Some("thread-explicit"),
            Some("resource-explicit"),
        )
        .expect("scope");
    session.load().expect("load failed");
    session
        .add_message("user", "x".repeat(26_000))
        .expect("append");

    let record = session
        .state
        .get_om_record_by_scope_key("thread:thread-explicit")
        .expect("fetch om")
        .expect("om record");
    assert_eq!(record.scope, OmScope::Thread);
    assert_eq!(record.thread_id.as_deref(), Some("thread-explicit"));
    assert_eq!(record.resource_id.as_deref(), Some("resource-explicit"));

    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observe_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("observe event");
    assert_eq!(
        observe_event
            .payload_json
            .get("scope_key")
            .and_then(serde_json::Value::as_str),
        Some("thread:thread-explicit")
    );
}

#[test]
fn om_write_path_accepts_explicit_resource_scope_binding() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-explicit-resource", fs, state, index)
        .with_om_scope(
            OmScope::Resource,
            Some("thread-explicit"),
            Some("resource-explicit"),
        )
        .expect("scope");
    session.load().expect("load failed");
    session
        .add_message("user", "x".repeat(26_000))
        .expect("append");

    let record = session
        .state
        .get_om_record_by_scope_key("resource:resource-explicit")
        .expect("fetch om")
        .expect("om record");
    assert_eq!(record.scope, OmScope::Resource);
    assert_eq!(record.thread_id, None);
    assert_eq!(record.resource_id.as_deref(), Some("resource-explicit"));

    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    assert!(
        !queued
            .iter()
            .any(|event| event.event_type == "om_observe_buffer_requested"),
        "resource scope defaults to async buffering disabled and must not enqueue observe outbox"
    );
}

#[test]
fn context_for_search_uses_archive_relevance_not_only_recency() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-archive-rank", fs, state, index);
    session.load().expect("load failed");

    session
        .add_message("user", "OAuth grant flow details")
        .expect("append");
    session.commit().expect("commit 1");

    session
        .add_message("user", "Kubernetes deployment note")
        .expect("append");
    session.commit().expect("commit 2");

    let ctx = session
        .get_context_for_search("oauth", 1, 8)
        .expect("context");
    assert!(
        ctx.recent_messages
            .iter()
            .any(|m| m.text.contains("OAuth grant flow details"))
    );
    assert!(
        !ctx.recent_messages
            .iter()
            .any(|m| m.text.contains("Kubernetes deployment note"))
    );
}

#[test]
fn context_for_search_skips_corrupted_active_jsonl_lines() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-active-corrupt", fs, state, index);
    session.load().expect("load failed");
    session
        .add_message("user", "OAuth refresh token from active messages")
        .expect("append failed");
    let messages_uri = session
        .session_uri()
        .expect("session uri")
        .join("messages.jsonl")
        .expect("messages uri");
    session
        .fs
        .append(&messages_uri, "{invalid-json\n", true)
        .expect("append corrupt");

    let context = session
        .get_context_for_search("oauth", 0, 8)
        .expect("context failed");
    assert!(
        context
            .recent_messages
            .iter()
            .any(|m| m.text.contains("OAuth refresh token from active messages"))
    );
}

#[test]
fn context_for_search_skips_corrupted_archive_jsonl_lines() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-archive-corrupt", fs, state, index);
    session.load().expect("load failed");
    session
        .add_message("user", "OAuth archive message should survive corruption")
        .expect("append");
    session.commit().expect("commit");

    let archive_uri = session
        .session_uri()
        .expect("session uri")
        .join("history/archive_001/messages.jsonl")
        .expect("archive uri");
    session
        .fs
        .append(&archive_uri, "{invalid-json\n", true)
        .expect("append corrupt archive line");

    let context = session
        .get_context_for_search("oauth", 1, 8)
        .expect("context failed");
    assert!(context.recent_messages.iter().any(|m| {
        m.text
            .contains("OAuth archive message should survive corruption")
    }));
}

#[test]
fn commit_extracts_six_categories_and_reindexes_immediately() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-all-categories", fs, state, index.clone());
    session.load().expect("load failed");
    session
        .add_message("user", "My name is Axient")
        .expect("append profile");
    session
        .add_message("user", "I prefer concise Rust code")
        .expect("append preferences");
    session
        .add_message("user", "This project repository is AxiomMe")
        .expect("append entities");
    session
        .add_message("assistant", "Today we deployed release v1.2")
        .expect("append events");
    session
        .add_message(
            "assistant",
            "Root cause identified and fixed with workaround",
        )
        .expect("append cases");
    session
        .add_message("assistant", "Always run this checklist before release")
        .expect("append patterns");

    let result = session.commit().expect("commit failed");
    assert!(result.memories_extracted >= 6);

    let records = index
        .read()
        .expect("index read")
        .all_records()
        .into_iter()
        .map(|record| record.uri)
        .collect::<Vec<_>>();

    assert!(
        records
            .iter()
            .any(|uri| uri == "axiom://user/memories/profile.md")
    );
    assert!(
        records
            .iter()
            .any(|uri| uri.starts_with("axiom://user/memories/preferences/"))
    );
    assert!(
        records
            .iter()
            .any(|uri| uri.starts_with("axiom://user/memories/entities/"))
    );
    assert!(
        records
            .iter()
            .any(|uri| uri.starts_with("axiom://user/memories/events/"))
    );
    assert!(
        records
            .iter()
            .any(|uri| uri.starts_with("axiom://agent/memories/cases/"))
    );
    assert!(
        records
            .iter()
            .any(|uri| uri.starts_with("axiom://agent/memories/patterns/"))
    );
}

#[test]
fn commit_merges_same_memory_with_provenance_across_sessions() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");

    let state_one =
        SqliteStateStore::open(temp.path().join("state-one.db")).expect("state one open");
    let index_one = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session_one = Session::new("s-merge-1", fs.clone(), state_one, index_one);
    session_one.load().expect("load one");
    session_one
        .add_message("user", "I prefer concise Rust code")
        .expect("append one");
    session_one.commit().expect("commit one");

    let state_two =
        SqliteStateStore::open(temp.path().join("state-two.db")).expect("state two open");
    let index_two = Arc::new(RwLock::new(InMemoryIndex::new()));
    let session_two = Session::new("s-merge-2", fs.clone(), state_two, index_two);
    session_two.load().expect("load two");
    session_two
        .add_message("user", "I prefer concise Rust code")
        .expect("append two");
    session_two.commit().expect("commit two");

    let key = stable_text_key("I prefer concise Rust code");
    let uri =
        AxiomUri::parse(&format!("axiom://user/memories/preferences/pref-{key}.md")).expect("uri");
    let content = fs.read(&uri).expect("read merged memory");

    assert_eq!(content.matches("- I prefer concise Rust code").count(), 1);
    assert!(content.contains("source: session s-merge-1"));
    assert!(content.contains("source: session s-merge-2"));
}

#[test]
fn extract_memories_uses_stable_key_for_same_text() {
    let messages = vec![
        Message {
            id: "msg-1".to_string(),
            role: "user".to_string(),
            text: "I prefer concise Rust code".to_string(),
            created_at: Utc::now(),
        },
        Message {
            id: "msg-2".to_string(),
            role: "user".to_string(),
            text: "I prefer concise Rust code".to_string(),
            created_at: Utc::now(),
        },
    ];

    let keys = extract_memories(&messages)
        .into_iter()
        .filter(|candidate| candidate.category == "preferences")
        .map(|candidate| candidate.key)
        .collect::<HashSet<_>>();

    assert_eq!(keys.len(), 1);
}

#[test]
fn extract_memories_fixture_profile_category() {
    let categories = fixture_categories("user", "My name is Axient and I build tools.");
    assert!(categories.contains("profile"));
}

#[test]
fn extract_memories_fixture_preferences_category() {
    let categories = fixture_categories("user", "I prefer concise Rust code and avoid magic.");
    assert!(categories.contains("preferences"));
}

#[test]
fn extract_memories_fixture_entities_category() {
    let categories = fixture_categories("user", "This project repository is AxiomMe.");
    assert!(categories.contains("entities"));
}

#[test]
fn extract_memories_fixture_events_category() {
    let categories = fixture_categories("assistant", "Today we deployed and rolled back once.");
    assert!(categories.contains("events"));
}

#[test]
fn extract_memories_fixture_cases_category() {
    let categories = fixture_categories("assistant", "Root cause found and fixed with workaround.");
    assert!(categories.contains("cases"));
}

#[test]
fn extract_memories_fixture_patterns_category() {
    let categories = fixture_categories("assistant", "Always run this checklist before release.");
    assert!(categories.contains("patterns"));
}

#[test]
fn observer_buffers_before_threshold_when_interval_crossed() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-observer", fs, state, index);
    session.load().expect("load failed");
    let long_text = "x".repeat(26_000);
    session
        .add_message("user", long_text)
        .expect("append should succeed");

    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-observer"), None, None).expect("scope key");
    let record = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("state query")
        .expect("om record must exist");

    let chunks = session
        .state
        .list_om_observation_chunks(&record.id)
        .expect("list chunks");
    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observer_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("om_observe_buffer_requested not queued");

    assert!(record.active_observations.is_empty());
    assert_eq!(record.observation_token_count, 0);
    assert!(record.pending_message_tokens > 0);
    assert!(!record.is_buffering_observation);
    assert_eq!(record.observer_trigger_count_total, 1);
    assert!(record.last_observed_at.is_none());
    assert!(chunks.is_empty());
    assert_eq!(
        record.last_buffered_at_tokens,
        record.pending_message_tokens
    );
    assert_eq!(observer_event.uri, "axiom://session/s-om-observer");
    assert_eq!(
        observer_event
            .payload_json
            .get("scope_key")
            .and_then(serde_json::Value::as_str),
        Some(scope_key.as_str())
    );
    assert_eq!(
        observer_event
            .payload_json
            .get("expected_generation")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        observer_event
            .payload_json
            .get("session_id")
            .and_then(serde_json::Value::as_str),
        Some("s-om-observer")
    );
}

#[test]
fn observer_activates_buffered_chunks_before_second_observer_call() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-step0", fs, state, index);
    session.load().expect("load failed");

    let first = session
        .add_message("user", format!("alpha-marker {}", "a".repeat(130_000)))
        .expect("append first");
    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-step0"), None, None).expect("scope key");
    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observer_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("om_observe_buffer_requested not queued");
    let expected_generation = required_payload_u32(
        &observer_event.payload_json,
        "expected_generation",
        "expected_generation must exist",
    );
    session
        .process_om_observe_buffer_requested(&scope_key, expected_generation, observer_event.id)
        .expect("process observer buffer request");
    session.add_message("user", "ping").expect("append second");

    let record = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("state query")
        .expect("om record must exist");
    let chunks = session
        .state
        .list_om_observation_chunks(&record.id)
        .expect("list chunks");

    assert!(record.last_activated_message_ids.contains(&first.id));
    assert!(record.active_observations.contains("alpha-marker"));
    assert!(record.pending_message_tokens > 0);
    assert_eq!(record.observer_trigger_count_total, 1);
    assert!(record.last_observed_at.is_some());
    assert!(chunks.is_empty());
}

#[test]
fn observer_falls_back_to_sync_activation_when_block_after_exceeded() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-block-after", fs, state, index);
    session.load().expect("load failed");
    session
        .add_message("user", "x".repeat(200_000))
        .expect("append should succeed");

    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-block-after"), None, None).expect("scope key");
    let record = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("state query")
        .expect("om record must exist");
    let chunks = session
        .state
        .list_om_observation_chunks(&record.id)
        .expect("list chunks");

    assert!(!record.active_observations.is_empty());
    assert_eq!(record.pending_message_tokens, 0);
    assert!(record.observation_token_count > 0);
    assert_eq!(record.observer_trigger_count_total, 1);
    assert!(record.last_observed_at.is_some());
    assert!(chunks.is_empty());
}

#[test]
fn observer_avoids_reprocessing_already_activated_messages() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-dedupe", fs, state, index);
    session.load().expect("load failed");

    let first = session
        .add_message("user", format!("alpha-marker {}", "a".repeat(130_000)))
        .expect("append first");
    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-dedupe"), None, None).expect("scope key");
    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observe_event_first = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("first observer event");
    let first_generation = required_payload_u32(
        &observe_event_first.payload_json,
        "expected_generation",
        "first expected_generation",
    );
    session
        .process_om_observe_buffer_requested(&scope_key, first_generation, observe_event_first.id)
        .expect("process first observer event");

    let second = session
        .add_message("user", format!("beta-marker {}", "b".repeat(130_000)))
        .expect("append second");
    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observe_event_second = queued
        .iter()
        .rfind(|event| event.event_type == "om_observe_buffer_requested")
        .expect("second observer event");
    let second_generation = required_payload_u32(
        &observe_event_second.payload_json,
        "expected_generation",
        "second expected_generation",
    );
    session
        .process_om_observe_buffer_requested(&scope_key, second_generation, observe_event_second.id)
        .expect("process second observer event");

    let third = session
        .add_message("user", format!("gamma-marker {}", "c".repeat(130_000)))
        .expect("append third");
    let record = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("state query")
        .expect("om record must exist");

    assert!(record.last_activated_message_ids.contains(&first.id));
    assert!(record.last_activated_message_ids.contains(&second.id));
    assert!(!record.last_activated_message_ids.contains(&third.id));
    assert_eq!(
        record.active_observations.matches("alpha-marker").count(),
        1
    );
    assert_eq!(record.active_observations.matches("beta-marker").count(), 1);
    assert!(!record.active_observations.contains("gamma-marker"));
    assert_eq!(record.observer_trigger_count_total, 3);
}

#[test]
fn observer_async_replay_same_event_id_is_idempotent() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-observe-idempotent", fs, state, index);
    session.load().expect("load failed");
    session
        .add_message("user", format!("idempotent-marker {}", "a".repeat(130_000)))
        .expect("append first");

    let scope_key = build_scope_key(
        OmScope::Session,
        Some("s-om-observe-idempotent"),
        None,
        None,
    )
    .expect("scope key");
    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observe_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("observe event");
    let expected_generation = required_payload_u32(
        &observe_event.payload_json,
        "expected_generation",
        "expected_generation",
    );

    session
        .process_om_observe_buffer_requested(&scope_key, expected_generation, observe_event.id)
        .expect("first observe processing");
    session
        .process_om_observe_buffer_requested(&scope_key, expected_generation, observe_event.id)
        .expect("second observe processing");

    let record = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("state query")
        .expect("om record must exist");
    let chunks = session
        .state
        .list_om_observation_chunks(&record.id)
        .expect("list chunks");
    assert_eq!(chunks.len(), 1);
    assert!(
        chunks[0].observations.contains("idempotent-marker"),
        "expected buffered chunk to include marker text"
    );
}

#[test]
fn observer_enqueues_om_reflect_buffer_requested_at_activation_threshold() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-reflect-buffer", fs, state, index);
    session.load().expect("load failed");
    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-reflect-buffer"), None, None).expect("key");
    let now = Utc::now();
    session
        .state
        .upsert_om_record(&OmRecord {
            id: "om-reflect-buffer-seed".to_string(),
            scope: OmScope::Session,
            scope_key: scope_key.clone(),
            session_id: Some("s-om-reflect-buffer".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 1,
            last_applied_outbox_event_id: None,
            origin_type: OmOriginType::Initial,
            active_observations: "seed observation".to_string(),
            observation_token_count: 21_000,
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
        })
        .expect("seed om record");

    session
        .add_message("user", format!("trigger {}", "x".repeat(26_000)))
        .expect("append");

    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let om_event = queued
        .iter()
        .find(|event| event.event_type == "om_reflect_buffer_requested")
        .expect("om_reflect_buffer_requested not queued");
    assert_eq!(om_event.uri, "axiom://session/s-om-reflect-buffer");
    assert_eq!(
        om_event
            .payload_json
            .get("scope_key")
            .and_then(serde_json::Value::as_str),
        Some(scope_key.as_str())
    );
    assert_eq!(
        om_event
            .payload_json
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(u64::from(crate::om_bridge::OM_OUTBOX_SCHEMA_VERSION_V1))
    );
    assert!(
        om_event
            .payload_json
            .get("requested_at")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    );
    assert_eq!(
        om_event
            .payload_json
            .get("expected_generation")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let updated = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("load om record")
        .expect("om record");
    assert_eq!(updated.reflector_trigger_count_total, 1);
    assert!(updated.is_buffering_reflection);
    assert!(!updated.is_reflecting);
}

#[test]
fn observer_enqueues_om_reflect_requested_when_reflector_block_after_is_met() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-reflect-enqueue", fs, state, index);
    session.load().expect("load failed");
    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-reflect-enqueue"), None, None).expect("key");
    let now = Utc::now();
    session
        .state
        .upsert_om_record(&OmRecord {
            id: "om-reflect-seed".to_string(),
            scope: OmScope::Session,
            scope_key: scope_key.clone(),
            session_id: Some("s-om-reflect-enqueue".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 3,
            last_applied_outbox_event_id: None,
            origin_type: OmOriginType::Initial,
            active_observations: "seed observation".to_string(),
            observation_token_count: 60_001,
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
        })
        .expect("seed om record");

    session
        .add_message("user", format!("trigger {}", "x".repeat(130_000)))
        .expect("append");

    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let om_event = queued
        .iter()
        .find(|event| event.event_type == "om_reflect_requested")
        .expect("om_reflect_requested not queued");
    assert_eq!(om_event.uri, "axiom://session/s-om-reflect-enqueue");
    assert_eq!(
        om_event
            .payload_json
            .get("scope_key")
            .and_then(serde_json::Value::as_str),
        Some(scope_key.as_str())
    );
    assert_eq!(
        om_event
            .payload_json
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(u64::from(crate::om_bridge::OM_OUTBOX_SCHEMA_VERSION_V1))
    );
    assert!(
        om_event
            .payload_json
            .get("requested_at")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    );
    assert_eq!(
        om_event
            .payload_json
            .get("expected_generation")
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    let updated = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("load om record")
        .expect("om record");
    assert_eq!(updated.reflector_trigger_count_total, 1);
    assert!(updated.is_reflecting);
    assert!(!updated.is_buffering_reflection);
}

#[test]
fn om_write_path_still_checks_reflection_when_observer_threshold_not_reached() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-reflect-only", fs, state, index);
    session.load().expect("load failed");
    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-reflect-only"), None, None).expect("key");
    let now = Utc::now();
    session
        .state
        .upsert_om_record(&OmRecord {
            id: "om-reflect-only-seed".to_string(),
            scope: OmScope::Session,
            scope_key: scope_key.clone(),
            session_id: Some("s-om-reflect-only".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 4,
            last_applied_outbox_event_id: None,
            origin_type: OmOriginType::Initial,
            active_observations: "seed observation".to_string(),
            observation_token_count: 50_000,
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
        })
        .expect("seed om record");

    session.add_message("user", "small ping").expect("append");

    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let om_event = queued
        .iter()
        .find(|event| event.event_type == "om_reflect_requested")
        .expect("om_reflect_requested not queued");
    assert_eq!(om_event.uri, "axiom://session/s-om-reflect-only");
    assert_eq!(
        om_event
            .payload_json
            .get("scope_key")
            .and_then(serde_json::Value::as_str),
        Some(scope_key.as_str())
    );
    assert_eq!(
        om_event
            .payload_json
            .get("expected_generation")
            .and_then(serde_json::Value::as_u64),
        Some(4)
    );

    let updated = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("load om record")
        .expect("om record");
    assert_eq!(updated.reflector_trigger_count_total, 1);
    assert!(updated.is_reflecting);
}

#[test]
fn om_write_path_resets_stale_buffer_boundary_and_retriggers_interval() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-boundary-reset", fs, state, index);
    session.load().expect("load failed");
    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-boundary-reset"), None, None).expect("key");
    let now = Utc::now();
    session
        .state
        .upsert_om_record(&OmRecord {
            id: "om-boundary-reset-seed".to_string(),
            scope: OmScope::Session,
            scope_key: scope_key.clone(),
            session_id: Some("s-om-boundary-reset".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 1,
            last_applied_outbox_event_id: None,
            origin_type: OmOriginType::Initial,
            active_observations: String::new(),
            observation_token_count: 0,
            pending_message_tokens: 5_900,
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
            // stale boundary greater than current pending tokens
            last_buffered_at_tokens: 30_000,
            last_buffered_at_time: Some(now),
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om record");

    session
        .add_message("user", "a".repeat(500))
        .expect("append first");
    session
        .add_message("user", "b".repeat(24_400))
        .expect("append second");

    let updated = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("load om record")
        .expect("om record");
    assert_eq!(updated.observer_trigger_count_total, 1);
}

#[test]
fn om_write_path_skips_async_observer_when_new_tokens_are_below_min_gate() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-min-new-gate", fs, state, index);
    session.load().expect("load failed");
    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-min-new-gate"), None, None).expect("key");
    let now = Utc::now();
    session
        .state
        .upsert_om_record(&OmRecord {
            id: "om-min-new-gate-seed".to_string(),
            scope: OmScope::Session,
            scope_key: scope_key.clone(),
            session_id: Some("s-om-min-new-gate".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 1,
            last_applied_outbox_event_id: None,
            origin_type: OmOriginType::Initial,
            active_observations: String::new(),
            observation_token_count: 0,
            pending_message_tokens: 4_500,
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
            last_buffered_at_tokens: 4_500,
            last_buffered_at_time: Some(now),
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om record");

    session
        .add_message("user", "a".repeat(1_600))
        .expect("append");

    let updated = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("load om record")
        .expect("om record");
    assert_eq!(updated.observer_trigger_count_total, 0);

    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    assert!(
        !queued
            .iter()
            .any(|event| event.event_type == "om_observe_buffer_requested"),
        "async observer event should not be queued below min-new-token gate"
    );
}

#[test]
fn observer_async_noops_when_candidates_are_cursor_covered() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-cursor-covered", fs, state, index);
    session.load().expect("load failed");
    session
        .add_message("user", "x".repeat(26_000))
        .expect("append first");

    let scope_key =
        build_scope_key(OmScope::Session, Some("s-om-cursor-covered"), None, None).expect("key");
    let queued = session.state.fetch_outbox(QueueEventStatus::New, 20).expect("fetch outbox");
    let observer_event = queued
        .iter()
        .find(|event| event.event_type == "om_observe_buffer_requested")
        .expect("observe event");
    let expected_generation = required_payload_u32(
        &observer_event.payload_json,
        "expected_generation",
        "expected_generation must exist",
    );

    session
        .process_om_observe_buffer_requested(&scope_key, expected_generation, observer_event.id)
        .expect("process first observe event");
    let after_first = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("state query")
        .expect("om record");
    let first_chunks = session
        .state
        .list_om_observation_chunks(&after_first.id)
        .expect("list first chunks");
    assert_eq!(first_chunks.len(), 1);

    let second_event_id = session
        .state
        .enqueue(
            "om_observe_buffer_requested",
            "axiom://session/s-om-cursor-covered",
            serde_json::json!({
                "scope_key": scope_key,
                "expected_generation": expected_generation,
                "session_id": "s-om-cursor-covered",
            }),
        )
        .expect("enqueue second observe event");
    session
        .process_om_observe_buffer_requested(&scope_key, expected_generation, second_event_id)
        .expect("process second observe event");

    let updated = session
        .state
        .get_om_record_by_scope_key(&scope_key)
        .expect("state query")
        .expect("om record");
    let second_chunks = session
        .state
        .list_om_observation_chunks(&updated.id)
        .expect("list chunks");
    assert!(
        second_chunks.len() == 1,
        "cursor-covered candidates should not create buffered observation chunk"
    );
    assert_eq!(second_chunks[0].id, first_chunks[0].id);
}

#[test]
fn add_message_records_dead_letter_when_observer_write_fails() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let db_path = temp.path().join("state.db");
    let state = SqliteStateStore::open(&db_path).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-observer-fail", fs, state, index);
    session.load().expect("load failed");

    session
        .state
        .drop_om_tables_for_test()
        .expect("drop om tables");

    session
        .add_message("user", "observer failure should be recorded")
        .expect("append should still succeed");

    let dead = session
        .state
        .fetch_outbox(QueueEventStatus::DeadLetter, 20)
        .expect("fetch dead-letter");
    let event = dead
        .iter()
        .find(|item| item.event_type == "om_observer_failed")
        .expect("om_observer_failed event");
    assert_eq!(event.uri, "axiom://session/s-om-observer-fail");
    assert_eq!(
        event
            .payload_json
            .get("error_code")
            .and_then(serde_json::Value::as_str),
        Some("SQLITE_ERROR")
    );
}

#[test]
fn record_observer_failure_persists_taxonomy_fields_for_om_inference_errors() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session = Session::new("s-om-observer-taxonomy", fs, state, index);
    session.load().expect("load failed");

    session.record_observer_failure(&AxiomError::OmInference {
        inference_source: OmInferenceSource::Observer,
        kind: OmInferenceFailureKind::Transient,
        message: "request timeout".to_string(),
    });

    let dead = session
        .state
        .fetch_outbox(QueueEventStatus::DeadLetter, 20)
        .expect("fetch dead-letter");
    let event = dead
        .iter()
        .find(|item| item.event_type == "om_observer_failed")
        .expect("om_observer_failed event");
    assert_eq!(
        event
            .payload_json
            .get("om_failure_source")
            .and_then(serde_json::Value::as_str),
        Some("observer")
    );
    assert_eq!(
        event
            .payload_json
            .get("om_failure_kind")
            .and_then(serde_json::Value::as_str),
        Some("transient")
    );
}
