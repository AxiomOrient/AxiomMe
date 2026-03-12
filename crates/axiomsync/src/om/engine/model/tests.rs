use chrono::{TimeZone, Utc};
use serde_json::{Value, json};

use super::{
    ContinuationPolicyV2, OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION, OmContinuationSourceKind,
    OmContinuationStateV2, OmDeterministicEvidence, OmDeterministicEvidenceKind,
    OmDeterministicObserverResponseV2, OmHintPolicyV2, OmObservationEntryInvariantViolation,
    OmObservationEntryV2, OmObservationOriginKind, OmObservationPriority, OmOriginType, OmRecord,
    OmRecordInvariantViolation, OmReflectionResponseV2, OmScope,
    OmSearchVisibleSnapshotInvariantViolation, OmSearchVisibleSnapshotV2, OmThreadRefV2,
    validate_observation_entry_v2_invariants, validate_om_record_invariants,
    validate_search_visible_snapshot_v2_invariants,
};

fn sample_record() -> OmRecord {
    let now = Utc.with_ymd_and_hms(2026, 2, 14, 0, 0, 0).unwrap();
    OmRecord {
        id: "record-1".to_string(),
        scope: OmScope::Thread,
        scope_key: "thread:t-1".to_string(),
        session_id: Some("s-1".to_string()),
        thread_id: Some("t-1".to_string()),
        resource_id: None,
        generation_count: 3,
        last_applied_outbox_event_id: Some(42),
        origin_type: OmOriginType::Reflection,
        active_observations: "obs".to_string(),
        observation_token_count: 120,
        pending_message_tokens: 30,
        last_observed_at: Some(now),
        current_task: Some("do work".to_string()),
        suggested_response: Some("reply".to_string()),
        last_activated_message_ids: vec!["m-1".to_string()],
        observer_trigger_count_total: 7,
        reflector_trigger_count_total: 2,
        is_observing: false,
        is_reflecting: true,
        is_buffering_observation: false,
        is_buffering_reflection: true,
        last_buffered_at_tokens: 10,
        last_buffered_at_time: Some(now),
        buffered_reflection: Some("buffer".to_string()),
        buffered_reflection_tokens: Some(11),
        buffered_reflection_input_tokens: Some(12),
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn om_scope_and_origin_type_string_contracts_are_stable() {
    let scope_cases = [
        (OmScope::Session, "session"),
        (OmScope::Thread, "thread"),
        (OmScope::Resource, "resource"),
    ];
    for (scope, raw) in scope_cases {
        assert_eq!(scope.as_str(), raw);
        assert_eq!(OmScope::parse(raw), Some(scope));
        assert_eq!(
            serde_json::to_value(scope).expect("serialize scope"),
            json!(raw)
        );
        let decoded =
            serde_json::from_value::<OmScope>(json!(raw)).expect("deserialize known scope");
        assert_eq!(decoded, scope);
    }
    assert_eq!(OmScope::parse("Session"), None);
    assert_eq!(OmScope::parse(" session "), None);
    assert!(serde_json::from_value::<OmScope>(json!("Session")).is_err());

    let origin_cases = [
        (OmOriginType::Initial, "initial"),
        (OmOriginType::Reflection, "reflection"),
    ];
    for (origin, raw) in origin_cases {
        assert_eq!(origin.as_str(), raw);
        assert_eq!(OmOriginType::parse(raw), Some(origin));
        assert_eq!(
            serde_json::to_value(origin).expect("serialize origin"),
            json!(raw)
        );
        let decoded =
            serde_json::from_value::<OmOriginType>(json!(raw)).expect("deserialize known origin");
        assert_eq!(decoded, origin);
    }
    assert_eq!(OmOriginType::parse("INITIAL"), None);
    assert!(serde_json::from_value::<OmOriginType>(json!("INITIAL")).is_err());
}

#[test]
fn om_record_deserialization_applies_defaults_for_optional_collections_and_counters() {
    let mut encoded = serde_json::to_value(sample_record()).expect("serialize record");
    let object = encoded.as_object_mut().expect("record object");
    object.remove("current_task");
    object.remove("suggested_response");
    object.remove("last_activated_message_ids");
    object.remove("observer_trigger_count_total");
    object.remove("reflector_trigger_count_total");

    let decoded = serde_json::from_value::<OmRecord>(encoded).expect("deserialize record");
    assert_eq!(decoded.current_task, None);
    assert_eq!(decoded.suggested_response, None);
    assert!(decoded.last_activated_message_ids.is_empty());
    assert_eq!(decoded.observer_trigger_count_total, 0);
    assert_eq!(decoded.reflector_trigger_count_total, 0);
}

#[test]
fn om_record_serialization_skips_none_optional_text_fields() {
    let mut record = sample_record();
    record.current_task = None;
    record.suggested_response = None;

    let encoded = serde_json::to_value(record).expect("serialize");
    let object = encoded.as_object().expect("object");
    assert_eq!(object.get("current_task"), None);
    assert_eq!(object.get("suggested_response"), None);
}

#[test]
fn om_record_roundtrip_preserves_explicit_data() {
    let record = sample_record();
    let encoded = serde_json::to_value(&record).expect("serialize");
    let decoded = serde_json::from_value::<OmRecord>(encoded).expect("deserialize");
    assert_eq!(decoded, record);
}

#[test]
fn om_record_rejects_unknown_scope_value() {
    let mut encoded = serde_json::to_value(sample_record()).expect("serialize record");
    let object = encoded.as_object_mut().expect("record object");
    object.insert("scope".to_string(), Value::String("unknown".to_string()));
    assert!(serde_json::from_value::<OmRecord>(encoded).is_err());
}

#[test]
fn om_record_invariants_accept_valid_sample_record() {
    let violations = validate_om_record_invariants(&sample_record());
    assert!(violations.is_empty());
}

#[test]
fn om_record_invariants_report_scope_identifier_and_scope_key_contract_mismatch() {
    let mut record = sample_record();
    record.scope = OmScope::Session;
    record.session_id = None;
    record.scope_key = "thread:t-1".to_string();

    let violations = validate_om_record_invariants(&record);
    assert!(
        violations.contains(&OmRecordInvariantViolation::MissingScopeIdentifier {
            field: "session_id",
        })
    );
    assert!(
        violations.contains(&OmRecordInvariantViolation::ScopeKeyPrefixMismatch {
            expected_prefix: "session:",
        })
    );
}

#[test]
fn om_record_invariants_report_scope_key_identifier_mismatch() {
    let mut record = sample_record();
    record.scope = OmScope::Thread;
    record.thread_id = Some("t-expected".to_string());
    record.scope_key = "thread:t-actual".to_string();

    let violations = validate_om_record_invariants(&record);
    assert!(violations.iter().any(|item| matches!(
        item,
        OmRecordInvariantViolation::ScopeKeyIdentifierMismatch {
            expected_identifier,
            actual_identifier,
        } if expected_identifier == "t-expected" && actual_identifier == "t-actual"
    )));
}

#[test]
fn om_record_invariants_report_resource_scope_contract_mismatch() {
    let mut record = sample_record();
    record.scope = OmScope::Resource;
    record.resource_id = None;
    record.scope_key = "thread:t-1".to_string();

    let violations = validate_om_record_invariants(&record);
    assert!(
        violations.contains(&OmRecordInvariantViolation::MissingScopeIdentifier {
            field: "resource_id",
        })
    );
    assert!(
        violations.contains(&OmRecordInvariantViolation::ScopeKeyPrefixMismatch {
            expected_prefix: "resource:",
        })
    );
}

#[test]
fn om_record_invariants_report_resource_scope_key_identifier_mismatch() {
    let mut record = sample_record();
    record.scope = OmScope::Resource;
    record.resource_id = Some("r-expected".to_string());
    record.scope_key = "resource:r-actual".to_string();

    let violations = validate_om_record_invariants(&record);
    assert!(violations.iter().any(|item| matches!(
        item,
        OmRecordInvariantViolation::ScopeKeyIdentifierMismatch {
            expected_identifier,
            actual_identifier,
        } if expected_identifier == "r-expected" && actual_identifier == "r-actual"
    )));
}

#[test]
fn om_record_invariants_report_empty_identifiers_and_orphan_reflection_metadata() {
    let mut record = sample_record();
    record.scope = OmScope::Thread;
    record.thread_id = Some(" ".to_string());
    record.scope_key = "thread:t-1".to_string();
    record.buffered_reflection = None;
    record.buffered_reflection_tokens = Some(11);
    record.buffered_reflection_input_tokens = Some(12);

    let violations = validate_om_record_invariants(&record);
    assert!(
        violations.contains(&OmRecordInvariantViolation::EmptyIdentifier { field: "thread_id" })
    );
    assert!(
        violations
            .contains(&OmRecordInvariantViolation::MissingScopeIdentifier { field: "thread_id" })
    );
    assert!(violations.contains(
        &OmRecordInvariantViolation::BufferedReflectionMetadataWithoutText {
            field: "buffered_reflection_tokens",
        }
    ));
    assert!(violations.contains(
        &OmRecordInvariantViolation::BufferedReflectionMetadataWithoutText {
            field: "buffered_reflection_input_tokens",
        }
    ));
}

#[test]
fn om_record_invariants_report_empty_scope_key_and_empty_buffered_reflection() {
    let mut record = sample_record();
    record.scope_key = " ".to_string();
    record.buffered_reflection = Some(" \n\t ".to_string());
    record.buffered_reflection_tokens = Some(11);

    let violations = validate_om_record_invariants(&record);
    assert!(violations.contains(&OmRecordInvariantViolation::EmptyScopeKey));
    assert!(violations.contains(&OmRecordInvariantViolation::EmptyBufferedReflection));
    assert!(violations.contains(
        &OmRecordInvariantViolation::BufferedReflectionMetadataWithoutText {
            field: "buffered_reflection_tokens",
        }
    ));
}

#[test]
fn om_record_invariants_report_empty_resource_identifier() {
    let mut record = sample_record();
    record.scope = OmScope::Resource;
    record.resource_id = Some("   ".to_string());
    record.scope_key = "resource:r-1".to_string();

    let violations = validate_om_record_invariants(&record);
    assert!(
        violations.contains(&OmRecordInvariantViolation::EmptyIdentifier {
            field: "resource_id",
        })
    );
    assert!(
        violations.contains(&OmRecordInvariantViolation::MissingScopeIdentifier {
            field: "resource_id",
        })
    );
}

#[test]
fn om_thread_ref_v2_roundtrip_and_optional_field_omission_is_stable() {
    let value = OmThreadRefV2 {
        canonical_thread_id: "thread:t-main".to_string(),
        scope: OmScope::Resource,
        scope_key: "resource:docs/om.md".to_string(),
        origin_thread_id: Some("t-main".to_string()),
        origin_session_id: None,
        resource_id: Some("docs/om.md".to_string()),
    };
    let encoded = serde_json::to_value(&value).expect("serialize thread ref");
    assert_eq!(encoded["canonical_thread_id"], json!("thread:t-main"));
    assert_eq!(encoded.get("origin_session_id"), None);

    let decoded = serde_json::from_value::<OmThreadRefV2>(encoded).expect("deserialize thread ref");
    assert_eq!(decoded, value);
}

#[test]
fn continuation_policy_v2_default_thresholds_are_stable() {
    let policy = ContinuationPolicyV2::default();
    assert_eq!(policy.min_confidence_milli_for_task, 500);
    assert_eq!(policy.min_confidence_milli_for_suggested_response, 700);
    assert!(policy.preserve_existing_task_on_weaker_update);
    assert!(policy.only_improve_suggested_response);
}

#[test]
fn continuation_state_v2_roundtrip_and_optional_field_omission_is_stable() {
    let value = OmContinuationStateV2 {
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        current_task: Some("ship release".to_string()),
        suggested_response: None,
        confidence_milli: 820,
        source_kind: OmContinuationSourceKind::ObserverDeterministic,
        source_message_ids: vec!["m-1".to_string(), "m-2".to_string()],
        updated_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
        staleness_budget_ms: 120_000,
    };

    let encoded = serde_json::to_value(&value).expect("serialize continuation state");
    assert_eq!(encoded.get("suggested_response"), None);

    let decoded = serde_json::from_value::<OmContinuationStateV2>(encoded)
        .expect("deserialize continuation state");
    assert_eq!(decoded, value);
}

#[test]
fn observation_entry_and_reflection_response_v2_roundtrip_is_stable() {
    let entry = OmObservationEntryV2 {
        entry_id: "entry-1".to_string(),
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        priority: OmObservationPriority::High,
        text: "priority:high check queue drift".to_string(),
        source_message_ids: vec!["m-1".to_string()],
        origin_kind: OmObservationOriginKind::Observation,
        created_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
        superseded_by: None,
    };
    let entry_encoded = serde_json::to_value(&entry).expect("serialize observation entry");
    assert_eq!(entry_encoded.get("superseded_by"), None);
    let entry_decoded =
        serde_json::from_value::<OmObservationEntryV2>(entry_encoded).expect("deserialize entry");
    assert_eq!(entry_decoded, entry);

    let reflection = OmReflectionResponseV2 {
        covers_entry_ids: vec!["entry-1".to_string(), "entry-2".to_string()],
        reflection_text: "compressed summary".to_string(),
        current_task: Some("ship release".to_string()),
        suggested_response: Some("respond with release status".to_string()),
    };
    let reflection_encoded = serde_json::to_value(&reflection).expect("serialize reflection");
    assert_eq!(
        reflection_encoded["covers_entry_ids"],
        json!(["entry-1", "entry-2"])
    );
    let reflection_decoded = serde_json::from_value::<OmReflectionResponseV2>(reflection_encoded)
        .expect("deserialize reflection");
    assert_eq!(reflection_decoded, reflection);
}

#[test]
fn deterministic_observer_response_v2_roundtrip_and_evidence_contract_are_stable() {
    let value = OmDeterministicObserverResponseV2 {
        observations: "[user] fix index drift".to_string(),
        observation_token_count: 6,
        observed_message_ids: vec!["m-1".to_string()],
        current_task: Some("fix index drift".to_string()),
        suggested_response: None,
        confidence_milli: 640,
        evidence: vec![OmDeterministicEvidence {
            message_id: "m-1".to_string(),
            role: "user".to_string(),
            kind: OmDeterministicEvidenceKind::TaskSignal,
            excerpt: "fix index drift".to_string(),
        }],
    };

    let encoded = serde_json::to_value(&value).expect("serialize deterministic response");
    assert_eq!(encoded.get("suggested_response"), None);
    assert_eq!(encoded["evidence"][0]["kind"], json!("task_signal"));

    let decoded = serde_json::from_value::<OmDeterministicObserverResponseV2>(encoded)
        .expect("deserialize deterministic response");
    assert_eq!(decoded, value);
}

#[test]
fn hint_policy_v2_default_contract_is_stable() {
    let policy = OmHintPolicyV2::default();
    assert_eq!(policy.max_lines, 4);
    assert_eq!(policy.max_chars, 240);
    assert!(policy.reserve_current_task_line);
    assert!(policy.reserve_suggested_response_line);
    assert_eq!(policy.high_priority_slots, 1);
    assert!(policy.include_buffered_entries);
}

#[test]
fn search_visible_snapshot_v2_roundtrip_and_optional_field_omission_is_stable() {
    let value = OmSearchVisibleSnapshotV2 {
        scope_key: "thread:t-1".to_string(),
        activated_entry_ids: vec!["a-1".to_string()],
        buffered_entry_ids: vec!["b-1".to_string()],
        current_task: Some("ship release".to_string()),
        suggested_response: None,
        rendered_hint: Some("om: current-task: ship release".to_string()),
        materialized_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
        snapshot_version: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION.to_string(),
        visible_entries: vec![OmObservationEntryV2 {
            entry_id: "a-1".to_string(),
            scope_key: "thread:t-1".to_string(),
            thread_id: "t-1".to_string(),
            priority: OmObservationPriority::High,
            text: "priority:high release blockers".to_string(),
            source_message_ids: vec!["m-1".to_string()],
            origin_kind: OmObservationOriginKind::Observation,
            created_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
            superseded_by: None,
        }],
    };

    let encoded = serde_json::to_value(&value).expect("serialize snapshot");
    assert_eq!(encoded.get("suggested_response"), None);
    assert_eq!(
        encoded["snapshot_version"],
        json!(OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION)
    );
    let decoded =
        serde_json::from_value::<OmSearchVisibleSnapshotV2>(encoded).expect("deserialize snapshot");
    assert_eq!(decoded, value);
}

#[test]
fn observation_entry_v2_invariants_report_empty_required_fields() {
    let entry = OmObservationEntryV2 {
        entry_id: " ".to_string(),
        scope_key: " ".to_string(),
        thread_id: " ".to_string(),
        priority: OmObservationPriority::Medium,
        text: " ".to_string(),
        source_message_ids: vec!["m-1".to_string(), " ".to_string()],
        origin_kind: OmObservationOriginKind::Observation,
        created_at_rfc3339: " ".to_string(),
        superseded_by: Some(" ".to_string()),
    };
    let violations = validate_observation_entry_v2_invariants(&entry);
    assert!(
        violations
            .contains(&OmObservationEntryInvariantViolation::EmptyField { field: "entry_id" })
    );
    assert!(
        violations
            .contains(&OmObservationEntryInvariantViolation::EmptyField { field: "scope_key" })
    );
    assert!(
        violations
            .contains(&OmObservationEntryInvariantViolation::EmptyField { field: "thread_id" })
    );
    assert!(
        violations.contains(&OmObservationEntryInvariantViolation::EmptyField { field: "text" })
    );
    assert!(
        violations.contains(&OmObservationEntryInvariantViolation::EmptyField {
            field: "created_at_rfc3339"
        })
    );
    assert!(violations.contains(&OmObservationEntryInvariantViolation::EmptySourceMessageId));
    assert!(violations.contains(&OmObservationEntryInvariantViolation::EmptySupersededBy));
}

#[test]
fn observation_entry_v2_invariants_report_invalid_rfc3339_timestamp() {
    let entry = OmObservationEntryV2 {
        entry_id: "entry-1".to_string(),
        scope_key: "thread:t-1".to_string(),
        thread_id: "t-1".to_string(),
        priority: OmObservationPriority::Medium,
        text: "stable observation".to_string(),
        source_message_ids: vec!["m-1".to_string()],
        origin_kind: OmObservationOriginKind::Observation,
        created_at_rfc3339: "not-rfc3339".to_string(),
        superseded_by: None,
    };
    let violations = validate_observation_entry_v2_invariants(&entry);
    assert!(
        violations.contains(&OmObservationEntryInvariantViolation::InvalidRfc3339 {
            field: "created_at_rfc3339",
            value: "not-rfc3339".to_string(),
        })
    );
}

#[test]
fn search_visible_snapshot_v2_invariants_validate_snapshot_and_visible_entries() {
    let snapshot = OmSearchVisibleSnapshotV2 {
        scope_key: "thread:t-1".to_string(),
        activated_entry_ids: vec!["a-1".to_string()],
        buffered_entry_ids: vec!["b-1".to_string()],
        current_task: Some("ship release".to_string()),
        suggested_response: None,
        rendered_hint: Some("om: current-task: ship release".to_string()),
        materialized_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
        snapshot_version: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION.to_string(),
        visible_entries: vec![OmObservationEntryV2 {
            entry_id: "a-1".to_string(),
            scope_key: "thread:t-1".to_string(),
            thread_id: "t-1".to_string(),
            priority: OmObservationPriority::High,
            text: "priority:high release blockers".to_string(),
            source_message_ids: vec!["m-1".to_string()],
            origin_kind: OmObservationOriginKind::Observation,
            created_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
            superseded_by: None,
        }],
    };
    assert!(validate_search_visible_snapshot_v2_invariants(&snapshot).is_empty());

    let invalid = OmSearchVisibleSnapshotV2 {
        scope_key: " ".to_string(),
        activated_entry_ids: vec![" ".to_string()],
        buffered_entry_ids: vec![" ".to_string()],
        current_task: None,
        suggested_response: None,
        rendered_hint: None,
        materialized_at_rfc3339: " ".to_string(),
        snapshot_version: "invalid-version".to_string(),
        visible_entries: vec![OmObservationEntryV2 {
            entry_id: "e-1".to_string(),
            scope_key: "thread:other".to_string(),
            thread_id: " ".to_string(),
            priority: OmObservationPriority::Medium,
            text: " ".to_string(),
            source_message_ids: vec![" ".to_string()],
            origin_kind: OmObservationOriginKind::Observation,
            created_at_rfc3339: " ".to_string(),
            superseded_by: None,
        }],
    };
    let violations = validate_search_visible_snapshot_v2_invariants(&invalid);
    assert!(
        violations.contains(&OmSearchVisibleSnapshotInvariantViolation::EmptyField {
            field: "scope_key"
        })
    );
    assert!(violations.contains(
        &OmSearchVisibleSnapshotInvariantViolation::SnapshotVersionMismatch {
            expected: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION,
            actual: "invalid-version".to_string(),
        }
    ));
    assert!(violations.contains(&OmSearchVisibleSnapshotInvariantViolation::EmptyActivatedEntryId));
    assert!(violations.contains(&OmSearchVisibleSnapshotInvariantViolation::EmptyBufferedEntryId));
    assert!(violations.iter().any(|item| matches!(
        item,
        OmSearchVisibleSnapshotInvariantViolation::VisibleEntryScopeMismatch {
            entry_id,
            scope_key
        } if entry_id == "e-1" && scope_key == "thread:other"
    )));
    assert!(violations.iter().any(|item| matches!(
        item,
        OmSearchVisibleSnapshotInvariantViolation::VisibleEntryInvariant { entry_id, .. } if entry_id == "e-1"
    )));
}

#[test]
fn search_visible_snapshot_v2_invariants_report_invalid_rfc3339_timestamp() {
    let snapshot = OmSearchVisibleSnapshotV2 {
        scope_key: "thread:t-1".to_string(),
        activated_entry_ids: vec!["a-1".to_string()],
        buffered_entry_ids: vec![],
        current_task: None,
        suggested_response: None,
        rendered_hint: None,
        materialized_at_rfc3339: "invalid-ts".to_string(),
        snapshot_version: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION.to_string(),
        visible_entries: vec![],
    };
    let violations = validate_search_visible_snapshot_v2_invariants(&snapshot);
    assert!(
        violations.contains(&OmSearchVisibleSnapshotInvariantViolation::InvalidRfc3339 {
            field: "materialized_at_rfc3339",
            value: "invalid-ts".to_string(),
        })
    );
}
