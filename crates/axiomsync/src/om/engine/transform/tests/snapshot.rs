use super::*;

use crate::{
    OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION, OmContinuationSourceKind, OmContinuationStateV2,
    OmHintPolicyV2, OmObservationEntryV2, OmObservationOriginKind, OmObservationPriority,
    OmSearchVisibleSnapshotV2,
};

fn entry(
    id: &str,
    priority: OmObservationPriority,
    text: &str,
    created_at: &str,
) -> OmObservationEntryV2 {
    OmObservationEntryV2 {
        entry_id: id.to_string(),
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        priority,
        text: text.to_string(),
        source_message_ids: vec![format!("msg-{id}")],
        origin_kind: OmObservationOriginKind::Observation,
        created_at_rfc3339: created_at.to_string(),
        superseded_by: None,
    }
}

fn continuation_state() -> OmContinuationStateV2 {
    OmContinuationStateV2 {
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        current_task: Some("finalize om-v2 rollout".to_string()),
        suggested_response: Some("reply with rollout status".to_string()),
        confidence_milli: 900,
        source_kind: OmContinuationSourceKind::ObserverDeterministic,
        source_message_ids: vec!["m-1".to_string()],
        updated_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
        staleness_budget_ms: 120_000,
    }
}

#[test]
fn materialize_search_visible_snapshot_combines_entries_and_continuation() {
    let activated = vec![
        entry(
            "a-1",
            OmObservationPriority::High,
            "priority:high fix queue drift",
            "2026-03-04T00:00:01Z",
        ),
        entry(
            "a-2",
            OmObservationPriority::Medium,
            "collect replay diagnostics",
            "2026-03-04T00:00:02Z",
        ),
    ];
    let buffered = vec![entry(
        "b-1",
        OmObservationPriority::Low,
        "tail-noise-1",
        "2026-03-04T00:00:03Z",
    )];

    let snapshot = materialize_search_visible_snapshot(
        "thread:t-1",
        &activated,
        &buffered,
        Some(&continuation_state()),
        "2026-03-04T00:00:10Z",
        OmHintPolicyV2::default(),
    );

    assert_eq!(snapshot.scope_key, "thread:t-1");
    assert_eq!(snapshot.activated_entry_ids, vec!["a-1", "a-2"]);
    assert_eq!(snapshot.buffered_entry_ids, vec!["b-1"]);
    assert_eq!(
        snapshot.current_task.as_deref(),
        Some("finalize om-v2 rollout")
    );
    assert_eq!(
        snapshot.snapshot_version,
        OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION
    );
    assert!(
        snapshot
            .rendered_hint
            .as_deref()
            .is_some_and(|hint| hint.starts_with("om: "))
    );
}

#[test]
fn render_search_hint_preserves_current_task_and_high_priority_under_tight_budget() {
    let snapshot = OmSearchVisibleSnapshotV2 {
        scope_key: "thread:t-1".to_string(),
        activated_entry_ids: vec!["a-1".to_string(), "a-2".to_string()],
        buffered_entry_ids: vec!["b-1".to_string()],
        current_task: Some("finalize om-v2 rollout".to_string()),
        suggested_response: Some("reply with rollout status".to_string()),
        rendered_hint: None,
        materialized_at_rfc3339: "2026-03-04T00:00:10Z".to_string(),
        snapshot_version: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION.to_string(),
        visible_entries: vec![
            entry(
                "a-1",
                OmObservationPriority::High,
                "priority:high fix queue drift",
                "2026-03-04T00:00:01Z",
            ),
            entry(
                "a-2",
                OmObservationPriority::Low,
                "tail-noise-1",
                "2026-03-04T00:00:02Z",
            ),
            entry(
                "a-3",
                OmObservationPriority::Low,
                "tail-noise-2",
                "2026-03-04T00:00:03Z",
            ),
        ],
    };
    let policy = OmHintPolicyV2 {
        max_lines: 2,
        max_chars: 240,
        reserve_current_task_line: true,
        reserve_suggested_response_line: false,
        high_priority_slots: 1,
        include_buffered_entries: true,
    };
    let hint = render_search_hint(&snapshot, policy).expect("hint");
    assert!(hint.contains("current-task: finalize om-v2 rollout"));
    assert!(hint.contains("priority:high fix queue drift"));
    assert!(!hint.contains("tail-noise-1"));
    assert!(!hint.contains("tail-noise-2"));
}

#[test]
fn render_search_hint_uses_deterministic_tie_break_for_same_timestamp() {
    let snapshot = OmSearchVisibleSnapshotV2 {
        scope_key: "thread:t-1".to_string(),
        activated_entry_ids: vec!["a-1".to_string(), "a-2".to_string()],
        buffered_entry_ids: Vec::new(),
        current_task: None,
        suggested_response: None,
        rendered_hint: None,
        materialized_at_rfc3339: "2026-03-04T00:00:10Z".to_string(),
        snapshot_version: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION.to_string(),
        visible_entries: vec![
            entry(
                "a-2",
                OmObservationPriority::High,
                "priority:high second",
                "2026-03-04T00:00:05Z",
            ),
            entry(
                "a-1",
                OmObservationPriority::High,
                "priority:high first",
                "2026-03-04T00:00:05Z",
            ),
        ],
    };
    let policy = OmHintPolicyV2 {
        max_lines: 1,
        max_chars: 240,
        reserve_current_task_line: false,
        reserve_suggested_response_line: false,
        high_priority_slots: 1,
        include_buffered_entries: true,
    };
    let first = render_search_hint(&snapshot, policy.clone()).expect("first");
    let second = render_search_hint(&snapshot, policy).expect("second");
    assert_eq!(first, second);
    assert!(first.contains("priority:high first"));
}

#[test]
fn render_search_hint_fills_high_priority_slots_after_duplicate_high_entries() {
    let snapshot = OmSearchVisibleSnapshotV2 {
        scope_key: "thread:t-1".to_string(),
        activated_entry_ids: vec!["a-1".to_string(), "a-2".to_string(), "a-3".to_string()],
        buffered_entry_ids: Vec::new(),
        current_task: None,
        suggested_response: None,
        rendered_hint: None,
        materialized_at_rfc3339: "2026-03-04T00:00:10Z".to_string(),
        snapshot_version: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION.to_string(),
        visible_entries: vec![
            entry(
                "a-3",
                OmObservationPriority::High,
                "priority:high duplicate",
                "2026-03-04T00:00:03Z",
            ),
            entry(
                "a-2",
                OmObservationPriority::High,
                "priority:high duplicate",
                "2026-03-04T00:00:02Z",
            ),
            entry(
                "a-1",
                OmObservationPriority::High,
                "priority:high unique",
                "2026-03-04T00:00:01Z",
            ),
        ],
    };

    let policy = OmHintPolicyV2 {
        max_lines: 2,
        max_chars: 240,
        reserve_current_task_line: false,
        reserve_suggested_response_line: false,
        high_priority_slots: 2,
        include_buffered_entries: true,
    };

    let hint = render_search_hint(&snapshot, policy).expect("hint");
    assert!(hint.contains("priority:high duplicate"));
    assert!(hint.contains("priority:high unique"));
}

#[test]
fn materialize_search_visible_snapshot_excludes_buffered_entries_when_policy_disables_them() {
    let activated = vec![entry(
        "a-1",
        OmObservationPriority::Medium,
        "active detail",
        "2026-03-04T00:00:01Z",
    )];
    let buffered = vec![entry(
        "b-1",
        OmObservationPriority::High,
        "priority:high buffered detail",
        "2026-03-04T00:00:02Z",
    )];

    let policy = OmHintPolicyV2 {
        include_buffered_entries: false,
        ..OmHintPolicyV2::default()
    };
    let snapshot = materialize_search_visible_snapshot(
        "thread:t-1",
        &activated,
        &buffered,
        None,
        "2026-03-04T00:00:10Z",
        policy,
    );

    assert_eq!(snapshot.visible_entries.len(), 1);
    assert_eq!(snapshot.visible_entries[0].entry_id, "a-1");
}
