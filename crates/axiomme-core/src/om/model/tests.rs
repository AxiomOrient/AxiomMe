use chrono::{TimeZone, Utc};
use serde_json::{Value, json};

use super::{OmOriginType, OmRecord, OmScope};

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
        reflected_observation_line_count: Some(4),
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
