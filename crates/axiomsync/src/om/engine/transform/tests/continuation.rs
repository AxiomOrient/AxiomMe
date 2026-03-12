use super::*;

use crate::{
    ContinuationPolicyV2, OmContinuationCandidateV2, OmContinuationSourceKind,
    OmContinuationStateV2,
};

fn sample_previous_state() -> OmContinuationStateV2 {
    OmContinuationStateV2 {
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        current_task: Some("existing task".to_string()),
        suggested_response: Some("existing response".to_string()),
        confidence_milli: 900,
        source_kind: OmContinuationSourceKind::ObserverLlm,
        source_message_ids: vec!["m-prev".to_string()],
        updated_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
        staleness_budget_ms: 120_000,
    }
}

#[test]
fn resolve_continuation_update_preserves_previous_fields_on_weaker_candidate() {
    let previous = sample_previous_state();
    let candidate = OmContinuationCandidateV2 {
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        current_task: Some("weak replacement".to_string()),
        suggested_response: Some("weak suggestion".to_string()),
        confidence_milli: 400,
        source_kind: OmContinuationSourceKind::ObserverDeterministic,
        source_message_ids: vec!["m-new".to_string()],
        updated_at_rfc3339: "2026-03-04T00:01:00Z".to_string(),
        staleness_budget_ms: 90_000,
    };

    let merged =
        resolve_continuation_update(Some(&previous), &candidate, ContinuationPolicyV2::default())
            .expect("continuation state");

    assert_eq!(merged.current_task.as_deref(), Some("existing task"));
    assert_eq!(
        merged.suggested_response.as_deref(),
        Some("existing response")
    );
    assert_eq!(merged.confidence_milli, 900);
    assert_eq!(merged.source_kind, OmContinuationSourceKind::ObserverLlm);
    assert_eq!(merged.source_message_ids, vec!["m-prev".to_string()]);
    assert_eq!(merged.updated_at_rfc3339, "2026-03-04T00:00:00Z");
    assert_eq!(merged.staleness_budget_ms, 120_000);
}

#[test]
fn resolve_continuation_update_replaces_with_stronger_candidate_and_normalizes_ids() {
    let previous = sample_previous_state();
    let candidate = OmContinuationCandidateV2 {
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        current_task: Some("ship release".to_string()),
        suggested_response: Some("reply with release status".to_string()),
        confidence_milli: 940,
        source_kind: OmContinuationSourceKind::ObserverLlm,
        source_message_ids: vec![
            "m-2".to_string(),
            "m-1".to_string(),
            "m-2".to_string(),
            " ".to_string(),
        ],
        updated_at_rfc3339: "2026-03-04T00:02:00Z".to_string(),
        staleness_budget_ms: 60_000,
    };

    let merged =
        resolve_continuation_update(Some(&previous), &candidate, ContinuationPolicyV2::default())
            .expect("continuation state");

    assert_eq!(merged.current_task.as_deref(), Some("ship release"));
    assert_eq!(
        merged.suggested_response.as_deref(),
        Some("reply with release status")
    );
    assert_eq!(
        merged.source_message_ids,
        vec!["m-1".to_string(), "m-2".to_string()]
    );
    assert_eq!(merged.confidence_milli, 940);
    assert_eq!(merged.source_kind, OmContinuationSourceKind::ObserverLlm);
    assert_eq!(merged.updated_at_rfc3339, "2026-03-04T00:02:00Z");
    assert_eq!(merged.staleness_budget_ms, 60_000);
}

#[test]
fn resolve_continuation_update_returns_none_when_no_eligible_signal_and_no_previous_state() {
    let candidate = OmContinuationCandidateV2 {
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        current_task: Some("should be dropped".to_string()),
        suggested_response: Some("should be dropped".to_string()),
        confidence_milli: 200,
        source_kind: OmContinuationSourceKind::ObserverDeterministic,
        source_message_ids: vec!["m-1".to_string()],
        updated_at_rfc3339: "2026-03-04T00:03:00Z".to_string(),
        staleness_budget_ms: 30_000,
    };

    let merged = resolve_continuation_update(None, &candidate, ContinuationPolicyV2::default());
    assert_eq!(merged, None);
}

#[test]
fn resolve_continuation_update_falls_back_to_previous_metadata_when_candidate_is_blank() {
    let previous = sample_previous_state();
    let candidate = OmContinuationCandidateV2 {
        scope_key: " ".to_string(),
        thread_id: " ".to_string(),
        current_task: Some("new task".to_string()),
        suggested_response: Some("new suggestion".to_string()),
        confidence_milli: 900,
        source_kind: OmContinuationSourceKind::ObserverLlm,
        source_message_ids: Vec::new(),
        updated_at_rfc3339: " ".to_string(),
        staleness_budget_ms: 0,
    };

    let merged =
        resolve_continuation_update(Some(&previous), &candidate, ContinuationPolicyV2::default())
            .expect("continuation state");

    assert_eq!(merged.scope_key, previous.scope_key);
    assert_eq!(merged.thread_id, previous.thread_id);
    assert_eq!(merged.updated_at_rfc3339, previous.updated_at_rfc3339);
    assert_eq!(merged.source_message_ids, previous.source_message_ids);
    assert_eq!(merged.staleness_budget_ms, previous.staleness_budget_ms);
}
