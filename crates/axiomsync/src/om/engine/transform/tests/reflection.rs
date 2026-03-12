use super::*;

use crate::{
    OmObservationEntryV2, OmObservationOriginKind, OmObservationPriority, OmReflectionResponseV2,
};

#[test]
fn merge_buffered_reflection_replaces_reflected_prefix_and_keeps_suffix() {
    let lines = vec![
        "r1".to_string(),
        "r2".to_string(),
        "new1".to_string(),
        "new2".to_string(),
    ];
    let merged = merge_buffered_reflection(&lines, "r1\nr2", "compact");
    assert_eq!(merged, "compact\n\nnew1\nnew2");
}

#[test]
fn merge_buffered_reflection_keeps_unmatched_prefix_lines() {
    let lines = vec!["a1".to_string(), "a2".to_string(), "tail".to_string()];
    let merged = merge_buffered_reflection(&lines, "x1\nx2", "compact");
    assert_eq!(merged, "compact\n\na1\na2\ntail");
}

fn observation_entry(entry_id: &str, text: &str) -> OmObservationEntryV2 {
    OmObservationEntryV2 {
        entry_id: entry_id.to_string(),
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        priority: OmObservationPriority::Medium,
        text: text.to_string(),
        source_message_ids: vec![format!("msg-{entry_id}")],
        origin_kind: OmObservationOriginKind::Observation,
        created_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
        superseded_by: None,
    }
}

fn reflection_signature(entries: &[OmObservationEntryV2]) -> Vec<(String, Option<String>)> {
    let mut pairs = entries
        .iter()
        .map(|entry| (entry.entry_id.clone(), entry.superseded_by.clone()))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
}

#[test]
fn apply_reflection_response_v2_marks_covered_entries_and_appends_reflection_entry() {
    let entries = vec![
        observation_entry("e-1", "alpha"),
        observation_entry("e-2", "beta"),
        observation_entry("e-3", "gamma"),
    ];
    let response = OmReflectionResponseV2 {
        covers_entry_ids: vec!["e-1".to_string(), "e-2".to_string()],
        reflection_text: "summary alpha beta".to_string(),
        current_task: None,
        suggested_response: None,
    };

    let applied = apply_reflection_response_v2(
        &entries,
        &response,
        "refl-1",
        "thread:t-1",
        "t-1",
        "2026-03-04T00:01:00Z",
    );
    assert_eq!(applied.len(), 4);
    assert!(applied.iter().any(|entry| {
        entry.entry_id == "e-1" && entry.superseded_by.as_deref() == Some("refl-1")
    }));
    assert!(applied.iter().any(|entry| {
        entry.entry_id == "e-2" && entry.superseded_by.as_deref() == Some("refl-1")
    }));
    assert!(
        applied
            .iter()
            .any(|entry| entry.entry_id == "e-3" && entry.superseded_by.is_none())
    );
    let reflection = applied
        .iter()
        .find(|entry| entry.entry_id == "refl-1")
        .expect("reflection entry");
    assert_eq!(reflection.origin_kind, OmObservationOriginKind::Reflection);
    assert_eq!(reflection.text, "summary alpha beta");
}

#[test]
fn apply_reflection_response_v2_is_idempotent_for_same_input() {
    let entries = vec![
        observation_entry("e-1", "alpha"),
        observation_entry("e-2", "beta"),
    ];
    let response = OmReflectionResponseV2 {
        covers_entry_ids: vec!["e-1".to_string()],
        reflection_text: "summary alpha".to_string(),
        current_task: None,
        suggested_response: None,
    };

    let first = apply_reflection_response_v2(
        &entries,
        &response,
        "refl-1",
        "thread:t-1",
        "t-1",
        "2026-03-04T00:01:00Z",
    );
    let second = apply_reflection_response_v2(
        &first,
        &response,
        "refl-1",
        "thread:t-1",
        "t-1",
        "2026-03-04T00:01:00Z",
    );
    assert_eq!(first, second);
}

#[test]
fn apply_reflection_response_v2_noops_when_covered_ids_do_not_match_entries() {
    let entries = vec![
        observation_entry("e-1", "alpha"),
        observation_entry("e-2", "beta"),
    ];
    let response = OmReflectionResponseV2 {
        covers_entry_ids: vec!["missing-entry".to_string()],
        reflection_text: "summary alpha".to_string(),
        current_task: None,
        suggested_response: None,
    };

    let applied = apply_reflection_response_v2(
        &entries,
        &response,
        "refl-1",
        "thread:t-1",
        "t-1",
        "2026-03-04T00:01:00Z",
    );
    assert_eq!(applied, entries);
}

#[test]
fn apply_reflection_response_v2_is_stable_under_line_wrap_perturbation() {
    let response = OmReflectionResponseV2 {
        covers_entry_ids: vec!["e-1".to_string(), "e-2".to_string()],
        reflection_text: "summary".to_string(),
        current_task: None,
        suggested_response: None,
    };
    let compact_entries = vec![
        observation_entry("e-1", "alpha beta gamma"),
        observation_entry("e-2", "delta epsilon"),
    ];
    let wrapped_entries = vec![
        observation_entry("e-1", "alpha\nbeta\ngamma"),
        observation_entry("e-2", "delta\nepsilon"),
    ];

    let compact_applied = apply_reflection_response_v2(
        &compact_entries,
        &response,
        "refl-1",
        "thread:t-1",
        "t-1",
        "2026-03-04T00:01:00Z",
    );
    let wrapped_applied = apply_reflection_response_v2(
        &wrapped_entries,
        &response,
        "refl-1",
        "thread:t-1",
        "t-1",
        "2026-03-04T00:01:00Z",
    );

    assert_eq!(
        reflection_signature(&compact_applied),
        reflection_signature(&wrapped_applied)
    );
    let compact_reflection = compact_applied
        .iter()
        .find(|entry| entry.entry_id == "refl-1")
        .expect("compact reflection");
    let wrapped_reflection = wrapped_applied
        .iter()
        .find(|entry| entry.entry_id == "refl-1")
        .expect("wrapped reflection");
    assert_eq!(compact_reflection.text, wrapped_reflection.text);
    assert_eq!(
        compact_reflection.source_message_ids,
        wrapped_reflection.source_message_ids
    );
}

#[test]
fn apply_reflection_response_v2_noops_when_required_reflection_metadata_is_blank() {
    let entries = vec![
        observation_entry("e-1", "alpha"),
        observation_entry("e-2", "beta"),
    ];
    let response = OmReflectionResponseV2 {
        covers_entry_ids: vec!["e-1".to_string()],
        reflection_text: "summary alpha".to_string(),
        current_task: None,
        suggested_response: None,
    };

    let missing_scope = apply_reflection_response_v2(
        &entries,
        &response,
        "refl-1",
        " ",
        "t-1",
        "2026-03-04T00:01:00Z",
    );
    assert_eq!(missing_scope, entries);

    let missing_thread = apply_reflection_response_v2(
        &entries,
        &response,
        "refl-1",
        "thread:t-1",
        " ",
        "2026-03-04T00:01:00Z",
    );
    assert_eq!(missing_thread, entries);

    let missing_created_at =
        apply_reflection_response_v2(&entries, &response, "refl-1", "thread:t-1", "t-1", " ");
    assert_eq!(missing_created_at, entries);
}

#[test]
fn build_reflection_draft_compacts_non_empty_lines() {
    let draft = build_reflection_draft("a \n\nb   c\n d", 16).expect("draft");
    assert_eq!(draft.covered_observations, "a\nb   c\nd");
    assert!(!draft.reflection.is_empty());
    assert!(draft.reflection_token_count > 0);
    assert!(draft.reflection_input_tokens >= draft.reflection_token_count);
}

#[test]
fn build_reflection_draft_returns_none_for_empty_text() {
    assert!(build_reflection_draft(" \n\t ", 128).is_none());
}

#[test]
fn build_reflection_draft_returns_none_for_zero_char_budget() {
    assert!(build_reflection_draft("non-empty", 0).is_none());
}

#[test]
fn plan_buffered_reflection_slice_matches_boundary_math() {
    let plan = plan_buffered_reflection_slice("l1\nl2\nl3\nl4", 100, 80, 0.5);
    assert_eq!(plan.sliced_observations, "l1");
    assert_eq!(plan.slice_token_estimate, 25);
    assert_eq!(plan.compression_target_tokens, 13);
}

#[test]
fn plan_buffered_reflection_slice_uses_all_lines_when_average_is_zero() {
    let plan = plan_buffered_reflection_slice("l1\nl2", 0, 80, 0.5);
    assert_eq!(plan.sliced_observations, "l1\nl2");
    assert_eq!(plan.slice_token_estimate, 0);
    assert_eq!(plan.compression_target_tokens, 0);
}

#[test]
fn reflector_compression_guidance_is_empty_at_level_zero() {
    assert_eq!(reflector_compression_guidance(0), "");
}

#[test]
fn reflector_compression_guidance_returns_level_text() {
    assert!(reflector_compression_guidance(1).contains("COMPRESSION REQUIRED"));
    assert!(reflector_compression_guidance(2).contains("AGGRESSIVE COMPRESSION REQUIRED"));
    assert_eq!(
        reflector_compression_guidance(9),
        reflector_compression_guidance(2)
    );
}

#[test]
fn validate_reflection_compression_is_strictly_less_than_target() {
    assert!(validate_reflection_compression(39_999, 40_000));
    assert!(!validate_reflection_compression(40_000, 40_000));
    assert!(!validate_reflection_compression(40_001, 40_000));
}

#[test]
fn reflector_trigger_is_strictly_greater_than_threshold() {
    assert!(!should_trigger_reflector(40_000, 40_000));
    assert!(should_trigger_reflector(40_001, 40_000));
}

#[test]
fn reflection_action_without_async_reflects_only_after_threshold() {
    assert_eq!(
        select_reflection_action(39_999, 40_000, None, None, false, false, false),
        ReflectionAction::None
    );
    assert_eq!(
        select_reflection_action(40_001, 40_000, None, None, false, false, false),
        ReflectionAction::Reflect
    );
}

#[test]
fn reflection_action_with_async_buffers_at_activation_point() {
    assert_eq!(
        select_reflection_action(19_999, 40_000, Some(0.5), Some(48_000), false, false, false),
        ReflectionAction::None
    );
    assert_eq!(
        select_reflection_action(20_000, 40_000, Some(0.5), Some(48_000), false, false, false),
        ReflectionAction::Buffer
    );
}

#[test]
fn reflection_action_with_async_reflects_from_buffer_when_threshold_exceeded() {
    assert_eq!(
        select_reflection_action(40_001, 40_000, Some(0.5), Some(48_000), true, false, false),
        ReflectionAction::Reflect
    );
}

#[test]
fn reflection_action_with_async_uses_block_after_for_sync_fallback() {
    assert_eq!(
        select_reflection_action(41_000, 40_000, Some(0.5), Some(48_000), false, false, false),
        ReflectionAction::Buffer
    );
    assert_eq!(
        select_reflection_action(48_000, 40_000, Some(0.5), Some(48_000), false, false, false),
        ReflectionAction::Reflect
    );
}

#[test]
fn reflection_action_skips_when_reflection_or_buffering_already_in_progress() {
    assert_eq!(
        select_reflection_action(48_000, 40_000, Some(0.5), Some(48_000), false, false, true),
        ReflectionAction::None
    );
    assert_eq!(
        select_reflection_action(30_000, 40_000, Some(0.5), Some(48_000), false, true, false),
        ReflectionAction::None
    );
}

#[test]
fn reflection_enqueue_decision_creates_command_and_next_flags() {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let record = OmRecord {
        id: "r1".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s1".to_string(),
        session_id: Some("s1".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 3,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "obs".to_string(),
        observation_token_count: 40_100,
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
        created_at: now,
        updated_at: now,
    };
    let config = ResolvedReflectionConfig {
        observation_tokens: 40_000,
        buffer_activation: Some(0.5),
        block_after: Some(48_000),
    };
    let decision = decide_reflection_enqueue(&record, config, "2026-01-01T00:00:00Z");
    assert_eq!(decision.action, ReflectionAction::Buffer);
    assert!(decision.command.is_some());
    assert!(!decision.next_is_reflecting);
    assert!(decision.next_is_buffering_reflection);
    assert!(decision.should_increment_trigger_count);
}

#[test]
fn reflection_enqueue_decision_reflect_path_sets_reflecting_without_buffering() {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let record = OmRecord {
        id: "r2".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s2".to_string(),
        session_id: Some("s2".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 4,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "obs".to_string(),
        observation_token_count: 48_000,
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
        created_at: now,
        updated_at: now,
    };
    let config = ResolvedReflectionConfig {
        observation_tokens: 40_000,
        buffer_activation: Some(0.5),
        block_after: Some(48_000),
    };
    let decision = decide_reflection_enqueue(&record, config, "2026-01-01T00:00:00Z");
    assert_eq!(decision.action, ReflectionAction::Reflect);
    assert!(decision.command.is_some());
    assert!(decision.next_is_reflecting);
    assert!(!decision.next_is_buffering_reflection);
}
