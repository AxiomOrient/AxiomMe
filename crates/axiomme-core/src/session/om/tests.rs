use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use tempfile::tempdir;

use crate::error::OmInferenceSource;
use crate::fs::LocalContextFs;
use crate::index::InMemoryIndex;
use crate::state::SqliteStateStore;

use super::*;

fn msg(id: &str, role: &str, text: &str) -> OmObserverMessageCandidate {
    OmObserverMessageCandidate {
        id: id.to_string(),
        role: role.to_string(),
        text: text.to_string(),
        created_at: Utc::now(),
        source_thread_id: Some("s-test".to_string()),
        source_session_id: Some("s-test".to_string()),
    }
}

fn observer_config(mode: OmObserverMode, model_enabled: bool) -> OmObserverConfig {
    let limits = crate::config::OmRuntimeLimitsConfig::default();
    OmObserverConfig {
        mode,
        model_enabled,
        llm_endpoint: "http://127.0.0.1:11434/api/chat".to_string(),
        llm_model: "qwen2.5:7b-instruct".to_string(),
        llm_timeout_ms: DEFAULT_OM_OBSERVER_LLM_TIMEOUT_MS,
        llm_max_output_tokens: DEFAULT_OM_OBSERVER_LLM_MAX_OUTPUT_TOKENS,
        llm_temperature_milli: DEFAULT_OM_OBSERVER_LLM_TEMPERATURE_MILLI,
        llm_strict: false,
        llm_max_chars_per_message: DEFAULT_OM_OBSERVER_LLM_MAX_CHARS_PER_MESSAGE,
        llm_max_input_tokens: DEFAULT_OM_OBSERVER_LLM_MAX_INPUT_TOKENS,
        observation_max_chars: limits.observation_max_chars,
        active_observations_max_chars: limits.observer_active_observations_max_chars,
        other_conversation_max_part_chars: limits.observer_other_conversation_max_part_chars,
    }
}

#[test]
fn parse_observer_response_value_reads_object_payload() {
    let payload = serde_json::json!({
        "observations": "[user] OAuth token refresh",
        "observed_message_ids": ["m1", "m2"],
        "observation_token_count": 8,
        "usage": {"input_tokens": 12, "output_tokens": 5}
    });
    let known_ids = vec!["m1".to_string(), "m2".to_string()];
    let parsed = parse_observer_response_value(
        &payload,
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("parse");
    assert_eq!(parsed.observations, "[user] OAuth token refresh");
    assert_eq!(
        parsed.observed_message_ids,
        vec!["m1".to_string(), "m2".to_string()]
    );
    assert_eq!(parsed.observation_token_count, 8);
    assert_eq!(parsed.current_task, None);
    assert_eq!(parsed.suggested_response, None);
    assert_eq!(parsed.usage.input_tokens, 12);
    assert_eq!(parsed.usage.output_tokens, 5);
}

#[test]
fn parse_observer_response_value_preserves_observation_lines() {
    let payload = serde_json::json!({
        "observations": " line-a \n\n line-b ",
        "observed_message_ids": ["m1"],
        "current_task": "Primary: debug auth",
        "suggested_response": "Ask user to confirm"
    });
    let known_ids = vec!["m1".to_string()];
    let parsed = parse_observer_response_value(
        &payload,
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("parse");
    assert_eq!(parsed.observations, "line-a\nline-b");
    assert_eq!(parsed.current_task.as_deref(), Some("Primary: debug auth"));
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Ask user to confirm")
    );
}

#[test]
fn parse_observer_response_value_accepts_mastra_alias_fields() {
    let payload = serde_json::json!({
        "observations": "line-a",
        "observedMessageIds": ["m1"],
        "observationTokenCount": 7,
        "usage": {"inputTokens": 12, "outputTokens": 5},
        "currentTask": "Primary: debug auth",
        "suggestedContinuation": "Ask user to confirm"
    });
    let known_ids = vec!["m1".to_string()];
    let parsed = parse_observer_response_value(
        &payload,
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("parse");
    assert_eq!(parsed.observations, "line-a");
    assert_eq!(parsed.observed_message_ids, vec!["m1".to_string()]);
    assert_eq!(parsed.observation_token_count, 7);
    assert_eq!(parsed.usage.input_tokens, 12);
    assert_eq!(parsed.usage.output_tokens, 5);
    assert_eq!(parsed.current_task.as_deref(), Some("Primary: debug auth"));
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Ask user to confirm")
    );
}

#[test]
fn parse_llm_observer_response_accepts_embedded_json_content() {
    let payload = serde_json::json!({
        "message": {
            "content": "```json\n{\"observations\":\"[assistant] keep signal\",\"observed_message_ids\":[\"m9\"]}\n```"
        }
    });
    let known_ids = vec!["m9".to_string()];
    let parsed = parse_llm_observer_response(
        &payload,
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("parse");
    assert_eq!(parsed.observations, "[assistant] keep signal");
    assert_eq!(parsed.observed_message_ids, vec!["m9".to_string()]);
}

#[test]
fn parse_llm_observer_response_accepts_xml_observations_content() {
    let payload = serde_json::json!({
        "message": {
            "content": "<observations>\n* 🔴 (09:15) User prefers direct answers\n* 🟡 (09:20) User asked about auth flow\n</observations>\n<current-task>\nPrimary: debug auth\n</current-task>\n<suggested-response>\nAsk user to confirm\n</suggested-response>"
        }
    });
    let known_ids = vec!["m7".to_string(), "m8".to_string()];
    let parsed = parse_llm_observer_response(
        &payload,
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("parse");
    assert!(parsed.observations.contains("User prefers direct answers"));
    assert!(parsed.observations.contains("User asked about auth flow"));
    assert_eq!(parsed.observed_message_ids, known_ids);
    assert_eq!(parsed.current_task.as_deref(), Some("Primary: debug auth"));
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Ask user to confirm")
    );
}

#[test]
fn parse_llm_observer_response_accepts_list_items_without_xml_tags() {
    let payload = serde_json::json!({
        "message": {
            "content": "* 🔴 (09:15) User prefers direct answers\n- 🟡 (09:20) User asked about auth flow\n1. 🟢 (09:22) Assistant suggested follow-up"
        }
    });
    let known_ids = vec!["m7".to_string()];
    let parsed = parse_llm_observer_response(
        &payload,
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("parse");
    assert!(parsed.observations.contains("User prefers direct answers"));
    assert!(parsed.observations.contains("User asked about auth flow"));
    assert!(
        parsed
            .observations
            .contains("Assistant suggested follow-up")
    );
    assert_eq!(parsed.observed_message_ids, known_ids);
}

#[test]
fn parse_llm_observer_response_returns_schema_taxonomy_for_invalid_payload() {
    let payload = serde_json::json!({"unexpected": "shape"});
    let known_ids = vec!["m9".to_string()];
    let err = parse_llm_observer_response(
        &payload,
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect_err("must fail");
    match err {
        AxiomError::OmInference {
            inference_source,
            kind,
            ..
        } => {
            assert_eq!(inference_source, OmInferenceSource::Observer);
            assert_eq!(kind, OmInferenceFailureKind::Schema);
        }
        other => panic!("unexpected error type: {other}"),
    }
}

#[test]
fn normalize_observation_text_preserves_lines_and_removes_blanks() {
    assert_eq!(
        normalize_observation_text(" a \n\n  b  \n c "),
        "a\nb\nc".to_string()
    );
}

#[test]
fn parse_memory_section_xml_returns_inner_observations_text() {
    let content = "<observations>\nline-a\nline-b\n</observations>\n<current-task>x</current-task>";
    let parsed = crate::om::parse_memory_section_xml(content, crate::om::OmParseMode::Strict);
    assert_eq!(parsed.observations, "line-a\nline-b");
}

#[test]
fn parse_memory_section_xml_joins_multiple_observation_blocks() {
    let content =
        "<observations>\nline-a\n</observations>\nnoise\n<observations>\nline-b\n</observations>";
    let parsed = crate::om::parse_memory_section_xml(content, crate::om::OmParseMode::Strict);
    assert_eq!(parsed.observations, "line-a\nline-b");
}

#[test]
fn parse_memory_section_xml_ignores_inline_tag_mentions() {
    let content = "literal <observations>inline</observations> mention\n<observations>\nline-a\n</observations>";
    let parsed = crate::om::parse_memory_section_xml(content, crate::om::OmParseMode::Strict);
    assert_eq!(parsed.observations, "line-a");
}

#[test]
fn build_observer_thread_messages_groups_and_sorts_by_thread_and_time() {
    let base = Utc::now();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m-b2".to_string(),
            role: "assistant".to_string(),
            text: "b2".to_string(),
            created_at: base + chrono::Duration::milliseconds(2),
            source_thread_id: Some("thread-b".to_string()),
            source_session_id: Some("thread-b".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-a1".to_string(),
            role: "user".to_string(),
            text: "a1".to_string(),
            created_at: base + chrono::Duration::milliseconds(1),
            source_thread_id: Some("thread-a".to_string()),
            source_session_id: Some("thread-a".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-b1".to_string(),
            role: "user".to_string(),
            text: "b1".to_string(),
            created_at: base,
            source_thread_id: Some("thread-b".to_string()),
            source_session_id: Some("thread-b".to_string()),
        },
    ];

    let grouped =
        build_observer_thread_messages(&candidates, OmScope::Resource, "resource:r1", "fallback");
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].thread_id, "thread-a");
    assert!(grouped[0].message_history.contains("a1"));
    assert_eq!(grouped[1].thread_id, "thread-b");
    let b_history = &grouped[1].message_history;
    let b1_idx = b_history.find("b1").expect("b1");
    let b2_idx = b_history.find("b2").expect("b2");
    assert!(b1_idx < b2_idx);
}

#[test]
fn parse_llm_multi_thread_observer_response_aggregates_primary_thread_metadata() {
    let payload = serde_json::json!({
        "message": {
            "content": "<observations>\n<thread id=\"s-main\">\nDate: Dec 4, 2025\n* 🔴 (14:30) User prefers direct answers\n<current-task>Primary: implement auth</current-task>\n<suggested-response>Continue auth changes</suggested-response>\n</thread>\n<thread id=\"s-peer\">\n* 🟡 (15:00) Peer session context\n<current-task>Primary: peer task</current-task>\n</thread>\n</observations>"
        }
    });
    let known_ids = vec!["m1".to_string(), "m2".to_string()];
    let parsed = parse_llm_multi_thread_observer_response(
        &payload,
        "s-main",
        &known_ids,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("multi-thread parse");

    assert!(
        parsed
            .response
            .observations
            .contains("<thread id=\"s-main\">")
    );
    assert!(
        parsed
            .response
            .observations
            .contains("<thread id=\"s-peer\">")
    );
    assert_eq!(
        parsed.response.current_task.as_deref(),
        Some("Primary: implement auth")
    );
    assert_eq!(
        parsed.response.suggested_response.as_deref(),
        Some("Continue auth changes")
    );
    assert_eq!(parsed.response.observed_message_ids, known_ids);
    assert_eq!(parsed.thread_states.len(), 2);
    assert!(
        parsed
            .thread_states
            .iter()
            .any(|state| state.thread_id == "s-main"
                && state.current_task.as_deref() == Some("Primary: implement auth"))
    );
}

#[test]
fn build_observer_thread_messages_uses_scope_thread_key_for_thread_scope() {
    let base = Utc::now();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m-a".to_string(),
            role: "user".to_string(),
            text: "from session a".to_string(),
            created_at: base,
            source_thread_id: Some("s-a".to_string()),
            source_session_id: Some("s-a".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-b".to_string(),
            role: "assistant".to_string(),
            text: "from session b".to_string(),
            created_at: base + chrono::Duration::milliseconds(1),
            source_thread_id: Some("s-b".to_string()),
            source_session_id: Some("s-b".to_string()),
        },
    ];

    let grouped =
        build_observer_thread_messages(&candidates, OmScope::Thread, "thread:t-1", "fallback");
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].thread_id, "t-1");
    assert!(grouped[0].message_history.contains("from session a"));
    assert!(grouped[0].message_history.contains("from session b"));
}

#[test]
fn chunk_observer_thread_batches_splits_by_token_budget_and_keeps_order() {
    let threads = vec![
        OmObserverThreadMessages {
            thread_id: "t-a".to_string(),
            message_history: "a".repeat(80),
        },
        OmObserverThreadMessages {
            thread_id: "t-b".to_string(),
            message_history: "b".repeat(80),
        },
        OmObserverThreadMessages {
            thread_id: "t-c".to_string(),
            message_history: "c".repeat(80),
        },
    ];

    let batches = chunk_observer_thread_batches(&threads, 30);
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0][0].thread_id, "t-a");
    assert_eq!(batches[1][0].thread_id, "t-b");
    assert_eq!(batches[2][0].thread_id, "t-c");
}

#[test]
fn collect_known_ids_for_thread_batch_respects_batch_membership() {
    let batch = vec![
        OmObserverThreadMessages {
            thread_id: "thread-a".to_string(),
            message_history: "a".to_string(),
        },
        OmObserverThreadMessages {
            thread_id: "thread-c".to_string(),
            message_history: "c".to_string(),
        },
    ];
    let known_ids = BTreeMap::from([
        (
            "thread-a".to_string(),
            vec!["m1".to_string(), "m2".to_string()],
        ),
        ("thread-b".to_string(), vec!["m3".to_string()]),
        ("thread-c".to_string(), vec!["m4".to_string()]),
    ]);

    let collected = collect_known_ids_for_thread_batch(&batch, &known_ids);
    assert_eq!(
        collected,
        vec!["m1".to_string(), "m2".to_string(), "m4".to_string()]
    );
}

#[test]
fn build_observer_batch_tasks_filters_empty_known_ids_and_preserves_batch_index() {
    let thread_batches = vec![
        vec![OmObserverThreadMessages {
            thread_id: "thread-a".to_string(),
            message_history: "a".to_string(),
        }],
        vec![OmObserverThreadMessages {
            thread_id: "thread-missing".to_string(),
            message_history: "x".to_string(),
        }],
        vec![OmObserverThreadMessages {
            thread_id: "thread-c".to_string(),
            message_history: "c".to_string(),
        }],
    ];
    let known_ids = BTreeMap::from([
        ("thread-a".to_string(), vec!["m1".to_string()]),
        (
            "thread-c".to_string(),
            vec!["m3".to_string(), "m4".to_string()],
        ),
    ]);

    let tasks = build_observer_batch_tasks(thread_batches, &known_ids);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].index, 0);
    assert_eq!(tasks[0].known_ids, vec!["m1".to_string()]);
    assert_eq!(tasks[1].index, 2);
    assert_eq!(tasks[1].known_ids, vec!["m3".to_string(), "m4".to_string()]);
}

#[test]
fn select_messages_for_observer_llm_enforces_bounds() {
    let selected = vec![
        msg("m1", "user", &"a".repeat(200)),
        msg("m2", "assistant", &"b".repeat(200)),
        msg("m3", "user", &"c".repeat(200)),
    ];
    let bounded = select_messages_for_observer_llm(&selected, 12, 40);
    assert!(!bounded.is_empty());
    assert!(bounded.len() <= selected.len());
    assert!(bounded.iter().all(|item| item.text.chars().count() <= 12));
    let total_tokens = bounded.iter().fold(0u32, |sum, item| {
        sum.saturating_add(estimate_text_tokens(&item.id))
            .saturating_add(estimate_text_tokens(&item.role))
            .saturating_add(estimate_text_tokens(&item.text))
            .saturating_add(8)
    });
    assert!(total_tokens <= 40);
}

#[test]
fn build_observation_chunk_uses_latest_selected_candidate_boundary() {
    let base = Utc::now();
    let selected = vec![
        OmObserverMessageCandidate {
            id: "m-old".to_string(),
            role: "user".to_string(),
            text: "older text".to_string(),
            created_at: base,
            source_thread_id: Some("s-a".to_string()),
            source_session_id: Some("s-a".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-new".to_string(),
            role: "assistant".to_string(),
            text: "newer text".to_string(),
            created_at: base + chrono::Duration::milliseconds(1),
            source_thread_id: Some("s-b".to_string()),
            source_session_id: Some("s-b".to_string()),
        },
    ];
    let chunk = build_observation_chunk(
        "record-1",
        &selected,
        &[],
        Utc::now(),
        "[assistant] summarized observation",
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    )
    .expect("chunk");

    assert_eq!(chunk.cycle_id, "observer_sync:m-new");
    assert_eq!(chunk.last_observed_at, selected[1].created_at);
    assert_eq!(
        chunk.message_ids,
        vec!["m-old".to_string(), "m-new".to_string()]
    );
    assert!(chunk.message_tokens > 0);
}

#[test]
fn record_with_buffered_observation_context_combines_active_and_buffered_text() {
    let mut record = new_session_om_record("s-buffered", "session:s-buffered", Utc::now());
    record.active_observations = "active-a".to_string();
    let buffered_chunks = vec![
        OmObservationChunk {
            id: "c1".to_string(),
            record_id: record.id.clone(),
            seq: 1,
            cycle_id: "cy1".to_string(),
            observations: "buffered-b".to_string(),
            token_count: 1,
            message_tokens: 1,
            message_ids: vec!["m1".to_string()],
            last_observed_at: Utc::now(),
            created_at: Utc::now(),
        },
        OmObservationChunk {
            id: "c2".to_string(),
            record_id: record.id.clone(),
            seq: 2,
            cycle_id: "cy2".to_string(),
            observations: "buffered-c".to_string(),
            token_count: 1,
            message_tokens: 1,
            message_ids: vec!["m2".to_string()],
            last_observed_at: Utc::now(),
            created_at: Utc::now(),
        },
    ];

    let combined = record_with_buffered_observation_context(
        &record,
        &buffered_chunks,
        crate::config::OmRuntimeLimitsConfig::default().observer_active_observations_max_chars,
    );
    assert!(combined.active_observations.contains("active-a"));
    assert!(combined.active_observations.contains("buffered-b"));
    assert!(combined.active_observations.contains("buffered-c"));
    assert!(
        combined
            .active_observations
            .contains("--- BUFFERED (pending activation) ---")
    );
}

#[test]
fn split_pending_and_other_conversation_candidates_partitions_by_session() {
    let selected = vec![
        OmObserverMessageCandidate {
            id: "m-local".to_string(),
            role: "user".to_string(),
            text: "local".to_string(),
            created_at: Utc::now(),
            source_thread_id: Some("s-local".to_string()),
            source_session_id: Some("s-local".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-peer".to_string(),
            role: "assistant".to_string(),
            text: "peer".to_string(),
            created_at: Utc::now(),
            source_thread_id: Some("s-peer".to_string()),
            source_session_id: Some("s-peer".to_string()),
        },
    ];

    let (pending, others) =
        split_pending_and_other_conversation_candidates(&selected, Some("s-local"));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "m-local");
    assert_eq!(others.len(), 1);
    assert_eq!(others[0].id, "m-peer");
}

#[test]
fn select_observed_message_candidates_filters_to_response_ids() {
    let selected = vec![
        OmObserverMessageCandidate {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "a".to_string(),
            created_at: Utc::now(),
            source_thread_id: Some("s-local".to_string()),
            source_session_id: Some("s-local".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "b".to_string(),
            created_at: Utc::now(),
            source_thread_id: Some("s-local".to_string()),
            source_session_id: Some("s-local".to_string()),
        },
    ];

    let filtered =
        select_observed_message_candidates(&selected, &["m2".to_string(), "unknown".to_string()]);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "m2");
}

#[test]
fn observer_model_feature_flag_off_forces_deterministic_output() {
    let selected = vec![msg("m1", "user", "important oauth detail")];
    let now = Utc::now();
    let record = new_session_om_record("s-flag", "session:s-flag", now);
    let config = observer_config(OmObserverMode::Llm, false);

    let resolved = resolve_observer_response_with_config(
        &record,
        "session:s-flag",
        &selected,
        "s-flag",
        crate::om::DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH,
        false,
        &config,
    )
    .expect("resolve");
    let expected = deterministic_observer_response(
        &record,
        &selected,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    );

    assert_eq!(resolved.response.observations, expected.observations);
    assert_eq!(resolved.selected_messages.len(), 1);
    assert_eq!(resolved.selected_messages[0].id, "m1");
}

#[test]
fn deterministic_observer_response_avoids_exact_active_duplicates() {
    let selected = vec![
        msg("m1", "user", "same detail"),
        msg("m2", "assistant", "new detail"),
    ];
    let mut record = new_session_om_record("s-dedupe", "session:s-dedupe", Utc::now());
    record.active_observations = "[user] same detail".to_string();

    let response = deterministic_observer_response(
        &record,
        &selected,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    );
    assert!(!response.observations.contains("[user] same detail"));
    assert!(response.observations.contains("[assistant] new detail"));
    assert_eq!(
        response.observed_message_ids,
        vec!["m1".to_string(), "m2".to_string()]
    );
}

#[test]
fn deterministic_observer_response_avoids_duplicates_present_in_buffered_context() {
    let selected = vec![
        msg("m1", "user", "same detail"),
        msg("m2", "assistant", "new detail"),
    ];
    let mut record =
        new_session_om_record("s-buffer-dedupe", "session:s-buffer-dedupe", Utc::now());
    let buffered_chunks = vec![OmObservationChunk {
        id: "c1".to_string(),
        record_id: record.id.clone(),
        seq: 1,
        cycle_id: "cy1".to_string(),
        observations: "[user] same detail".to_string(),
        token_count: 1,
        message_tokens: 1,
        message_ids: vec!["m1".to_string()],
        last_observed_at: Utc::now(),
        created_at: Utc::now(),
    }];
    record = record_with_buffered_observation_context(
        &record,
        &buffered_chunks,
        crate::config::OmRuntimeLimitsConfig::default().observer_active_observations_max_chars,
    );

    let response = deterministic_observer_response(
        &record,
        &selected,
        crate::config::OmRuntimeLimitsConfig::default().observation_max_chars,
    );
    assert!(!response.observations.contains("[user] same detail"));
    assert!(response.observations.contains("[assistant] new detail"));
}

#[test]
fn observer_rollout_profile_overrides_model_enabled_flag() {
    assert!(resolve_observer_model_enabled(false, Some("observer_only")));
    assert!(resolve_observer_model_enabled(false, Some("full_model")));
    assert!(!resolve_observer_model_enabled(true, Some("baseline")));
    assert!(resolve_observer_model_enabled(true, Some("unknown")));
    assert!(!resolve_observer_model_enabled(false, Some("unknown")));
}

#[test]
fn collect_observer_messages_for_resource_scope_includes_peer_sessions() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session_a = Session::new("s-resource-a", fs.clone(), state.clone(), index.clone());
    let session_b = Session::new("s-resource-b", fs, state.clone(), index);
    session_a.load().expect("load a");
    session_b.load().expect("load b");
    let a_msg = session_a
        .add_message("user", "resource shared note from-a")
        .expect("append a");
    let _b_msg = session_b
        .add_message("user", "resource shared note from-b")
        .expect("append b");

    state
        .upsert_om_scope_session("resource:r-1", "s-resource-a")
        .expect("scope map a");
    state
        .upsert_om_scope_session("resource:r-1", "s-resource-b")
        .expect("scope map b");

    let observed = HashSet::from([a_msg.id.clone()]);
    let selected = session_a
        .collect_observer_messages_for_scope(
            OmScope::Resource,
            "resource:r-1",
            None,
            &observed,
            8,
            false,
        )
        .expect("collect");

    assert!(
        selected.iter().any(|item| item.text.contains("from-b")),
        "expected peer-session message to be included"
    );
    assert!(
        selected.iter().all(|item| item.id != a_msg.id),
        "observed ids should be filtered"
    );
}

#[test]
fn collect_observer_messages_for_thread_scope_includes_peer_sessions() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session_a = Session::new("s-thread-a", fs.clone(), state.clone(), index.clone());
    let session_b = Session::new("s-thread-b", fs, state.clone(), index);
    session_a.load().expect("load a");
    session_b.load().expect("load b");
    let a_msg = session_a
        .add_message("user", "thread shared note from-a")
        .expect("append a");
    let _b_msg = session_b
        .add_message("assistant", "thread shared note from-b")
        .expect("append b");

    state
        .upsert_om_scope_session("thread:t-1", "s-thread-a")
        .expect("scope map a");
    state
        .upsert_om_scope_session("thread:t-1", "s-thread-b")
        .expect("scope map b");

    let observed = HashSet::from([a_msg.id.clone()]);
    let selected = session_a
        .collect_observer_messages_for_scope(
            OmScope::Thread,
            "thread:t-1",
            None,
            &observed,
            8,
            false,
        )
        .expect("collect");

    assert!(
        selected.iter().any(|item| item.text.contains("from-b")),
        "expected peer-session message to be included for thread scope"
    );
    assert!(
        selected.iter().all(|item| item.id != a_msg.id),
        "observed ids should be filtered"
    );
}

#[test]
fn collect_observer_messages_for_scope_prefers_unobserved_since_cursor() {
    let temp = tempdir().expect("tempdir");
    let fs = LocalContextFs::new(temp.path());
    fs.initialize().expect("init failed");
    let state = SqliteStateStore::open(temp.path().join("state.db")).expect("state open");
    let index = Arc::new(RwLock::new(InMemoryIndex::new()));

    let session_a = Session::new(
        "s-resource-cursor-a",
        fs.clone(),
        state.clone(),
        index.clone(),
    );
    let session_b = Session::new("s-resource-cursor-b", fs, state.clone(), index);
    session_a.load().expect("load a");
    session_b.load().expect("load b");

    let old = session_b
        .add_message("user", "resource old peer message")
        .expect("append old");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let new = session_b
        .add_message("user", "resource new peer message")
        .expect("append new");

    state
        .upsert_om_scope_session("resource:r-cursor", "s-resource-cursor-a")
        .expect("scope map a");
    state
        .upsert_om_scope_session("resource:r-cursor", "s-resource-cursor-b")
        .expect("scope map b");

    let selected = session_a
        .collect_observer_messages_for_scope(
            OmScope::Resource,
            "resource:r-cursor",
            Some(old.created_at),
            &HashSet::new(),
            16,
            false,
        )
        .expect("collect");

    assert!(
        selected.iter().any(|item| item.id == new.id),
        "new peer message must remain after cursor filter"
    );
    assert!(
        selected.iter().all(|item| item.id != old.id),
        "old peer message must be filtered by last_observed_at"
    );
}

#[test]
fn resolve_om_scope_binding_defaults_to_session_scope() {
    let resolved = resolve_om_scope_binding("s-1", None, None, None).expect("resolve");
    assert_eq!(resolved.scope, OmScope::Session);
    assert_eq!(resolved.scope_key, "session:s-1");
    assert_eq!(resolved.session_id.as_deref(), Some("s-1"));
    assert_eq!(resolved.thread_id, None);
    assert_eq!(resolved.resource_id, None);
}

#[test]
fn resolve_om_scope_binding_resolves_thread_scope() {
    let resolved =
        resolve_om_scope_binding("s-1", Some("thread"), Some("t-1"), Some("r-1")).expect("resolve");
    assert_eq!(resolved.scope, OmScope::Thread);
    assert_eq!(resolved.scope_key, "thread:t-1");
    assert_eq!(resolved.session_id, None);
    assert_eq!(resolved.thread_id.as_deref(), Some("t-1"));
    assert_eq!(resolved.resource_id.as_deref(), Some("r-1"));
}

#[test]
fn resolve_om_scope_binding_explicit_resolves_session_scope() {
    let resolved = resolve_om_scope_binding_explicit(
        "s-explicit-session",
        OmScope::Session,
        Some("ignored-thread"),
        Some("ignored-resource"),
    )
    .expect("resolve");
    assert_eq!(resolved.scope, OmScope::Session);
    assert_eq!(resolved.scope_key, "session:s-explicit-session");
    assert_eq!(resolved.session_id.as_deref(), Some("s-explicit-session"));
    assert_eq!(resolved.thread_id, None);
    assert_eq!(resolved.resource_id, None);
}

#[test]
fn resolve_om_scope_binding_explicit_resolves_thread_scope() {
    let resolved = resolve_om_scope_binding_explicit(
        "s-explicit-thread",
        OmScope::Thread,
        Some("thread-explicit"),
        Some("resource-explicit"),
    )
    .expect("resolve");
    assert_eq!(resolved.scope, OmScope::Thread);
    assert_eq!(resolved.scope_key, "thread:thread-explicit");
    assert_eq!(resolved.session_id, None);
    assert_eq!(resolved.thread_id.as_deref(), Some("thread-explicit"));
    assert_eq!(resolved.resource_id.as_deref(), Some("resource-explicit"));
}

#[test]
fn resolve_om_scope_binding_explicit_resolves_resource_scope() {
    let resolved = resolve_om_scope_binding_explicit(
        "s-explicit-resource",
        OmScope::Resource,
        Some("thread-explicit"),
        Some("resource-explicit"),
    )
    .expect("resolve");
    assert_eq!(resolved.scope, OmScope::Resource);
    assert_eq!(resolved.scope_key, "resource:resource-explicit");
    assert_eq!(resolved.session_id, None);
    assert_eq!(resolved.thread_id, None);
    assert_eq!(resolved.resource_id.as_deref(), Some("resource-explicit"));
}

#[test]
fn resolve_om_scope_binding_rejects_invalid_scope() {
    let err = resolve_om_scope_binding("s-1", Some("invalid"), Some("t-1"), Some("r-1"))
        .expect_err("must fail");
    assert!(err.to_string().contains(ENV_OM_SCOPE));
}

#[test]
fn resolve_om_scope_binding_rejects_thread_scope_without_thread_id() {
    let err =
        resolve_om_scope_binding("s-1", Some("thread"), None, Some("r-1")).expect_err("must fail");
    assert!(err.to_string().contains("thread_id"));
}

#[test]
fn runtime_config_rejects_share_budget_when_async_buffering_enabled() {
    let env = RuntimeOmEnv {
        share_token_budget: Some("true".to_string()),
        ..RuntimeOmEnv::default()
    };
    let err = resolve_runtime_om_config(&env, OmScope::Session).expect_err("must reject");
    match err {
        AxiomError::Validation(message) => {
            assert!(message.contains("shareTokenBudget"));
            assert!(message.contains("async buffering"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn runtime_config_allows_share_budget_when_buffering_is_disabled() {
    let env = RuntimeOmEnv {
        share_token_budget: Some("true".to_string()),
        buffer_tokens: Some("disabled".to_string()),
        ..RuntimeOmEnv::default()
    };
    let resolved = resolve_runtime_om_config(&env, OmScope::Session).expect("resolve");
    assert!(resolved.share_token_budget);
    assert!(resolved.async_buffering_disabled);
    assert_eq!(resolved.observation.total_budget, Some(70_000));
    assert_eq!(resolved.observation.buffer_tokens, None);
    assert_eq!(resolved.observation.buffer_activation, None);
    assert_eq!(resolved.reflection.buffer_activation, None);
    assert_eq!(resolved.reflection.block_after, None);
}

#[test]
fn runtime_config_rejects_invalid_buffer_tokens_env_value() {
    let env = RuntimeOmEnv {
        buffer_tokens: Some("abc".to_string()),
        ..RuntimeOmEnv::default()
    };
    let err = resolve_runtime_om_config(&env, OmScope::Session).expect_err("must reject");
    match err {
        AxiomError::Validation(message) => {
            assert!(message.contains(ENV_OM_BUFFER_TOKENS));
            assert!(message.contains("got: abc"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn runtime_config_resource_scope_disables_async_by_default() {
    let env = RuntimeOmEnv::default();
    let resolved = resolve_runtime_om_config(&env, OmScope::Resource).expect("resolve");
    assert!(resolved.async_buffering_disabled);
    assert_eq!(resolved.observation.buffer_tokens, None);
    assert_eq!(resolved.observation.buffer_activation, None);
    assert_eq!(resolved.observation.block_after, None);
    assert_eq!(
        resolved.observation.max_tokens_per_batch,
        crate::om::DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH
    );
    assert_eq!(resolved.reflection.buffer_activation, None);
    assert_eq!(resolved.reflection.block_after, None);
}

#[test]
fn runtime_config_resource_scope_rejects_explicit_async_buffering() {
    let env = RuntimeOmEnv {
        buffer_tokens: Some("0.2".to_string()),
        ..RuntimeOmEnv::default()
    };
    let err = resolve_runtime_om_config(&env, OmScope::Resource).expect_err("must reject");
    match err {
        AxiomError::Validation(message) => {
            assert!(message.contains("resource scope"));
            assert!(message.contains("async buffering"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn runtime_config_resource_scope_share_budget_allowed_without_explicit_async() {
    let env = RuntimeOmEnv {
        share_token_budget: Some("true".to_string()),
        ..RuntimeOmEnv::default()
    };
    let resolved = resolve_runtime_om_config(&env, OmScope::Resource).expect("resolve");
    assert!(resolved.share_token_budget);
    assert!(resolved.async_buffering_disabled);
    assert_eq!(resolved.observation.total_budget, Some(70_000));
}

#[test]
fn runtime_config_resource_scope_share_budget_rejected_when_async_explicitly_enabled() {
    let env = RuntimeOmEnv {
        share_token_budget: Some("true".to_string()),
        buffer_tokens: Some("0.2".to_string()),
        ..RuntimeOmEnv::default()
    };
    let err = resolve_runtime_om_config(&env, OmScope::Resource).expect_err("must reject");
    match err {
        AxiomError::Validation(message) => {
            assert!(message.contains("resource scope"));
            assert!(message.contains("async buffering"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn runtime_config_allows_overriding_max_tokens_per_batch() {
    let env = RuntimeOmEnv {
        observer_max_tokens_per_batch: Some("4096".to_string()),
        ..RuntimeOmEnv::default()
    };
    let resolved = resolve_runtime_om_config(&env, OmScope::Session).expect("resolve");
    assert_eq!(resolved.observation.max_tokens_per_batch, 4096);
}

#[test]
fn runtime_config_session_scope_parses_ratio_buffer_tokens_without_behavior_change() {
    let env = RuntimeOmEnv {
        buffer_tokens: Some("0.2".to_string()),
        ..RuntimeOmEnv::default()
    };
    let resolved = resolve_runtime_om_config(&env, OmScope::Session).expect("resolve");
    assert!(!resolved.async_buffering_disabled);
    assert_eq!(resolved.observation.buffer_tokens, Some(6_000));
    assert_eq!(resolved.observation.buffer_activation, Some(0.8));
    assert_eq!(resolved.observation.block_after, Some(36_000));
}

#[test]
fn om_enabled_parser_defaults_to_true_and_accepts_false_tokens() {
    assert!(parse_env_enabled_default_true(None));
    assert!(parse_env_enabled_default_true(Some("")));
    assert!(parse_env_enabled_default_true(Some("true")));
    assert!(parse_env_enabled_default_true(Some("1")));
    assert!(!parse_env_enabled_default_true(Some("false")));
    assert!(!parse_env_enabled_default_true(Some("0")));
    assert!(!parse_env_enabled_default_true(Some("off")));
    assert!(!parse_env_enabled_default_true(Some("disabled")));
}
