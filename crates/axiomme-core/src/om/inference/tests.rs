use serde_json::json;

use super::*;

fn sample_model() -> OmInferenceModelConfig {
    OmInferenceModelConfig {
        provider: "local-http".to_string(),
        model: "qwen2.5:7b-instruct".to_string(),
        max_output_tokens: 1200,
        temperature_milli: 300,
    }
}

#[test]
fn observer_request_roundtrip_preserves_optional_fields_when_present() {
    let request = OmObserverRequest {
        scope: OmScope::Thread,
        scope_key: "thread:t-1".to_string(),
        model: sample_model(),
        active_observations: "obs-a\nobs-b".to_string(),
        other_conversations: Some("peer-thread context".to_string()),
        pending_messages: vec![OmPendingMessage {
            id: "m-1".to_string(),
            role: "user".to_string(),
            text: "hello".to_string(),
            created_at_rfc3339: Some("2026-02-13T12:00:00Z".to_string()),
        }],
    };
    let encoded = serde_json::to_value(&request).expect("serialize");
    let decoded = serde_json::from_value::<OmObserverRequest>(encoded).expect("deserialize");
    assert_eq!(decoded, request);
}

#[test]
fn observer_request_deserialization_defaults_optional_fields_to_none() {
    let decoded = serde_json::from_value::<OmObserverRequest>(json!({
        "scope": "session",
        "scope_key": "session:s-1",
        "model": {
            "provider": "local-http",
            "model": "qwen2.5:7b-instruct",
            "max_output_tokens": 800,
            "temperature_milli": 0
        },
        "active_observations": "obs",
        "pending_messages": [
            {"id": "m-1", "role": "user", "text": "hello"}
        ]
    }))
    .expect("deserialize");
    assert_eq!(decoded.scope, OmScope::Session);
    assert_eq!(decoded.other_conversations, None);
    assert_eq!(decoded.pending_messages.len(), 1);
    assert_eq!(decoded.pending_messages[0].created_at_rfc3339, None);
}

#[test]
fn observer_response_roundtrip_skips_optional_fields_when_none() {
    let response = OmObserverResponse {
        observations: "Date: Feb 13, 2026\n* info".to_string(),
        observation_token_count: 42,
        observed_message_ids: vec!["m-1".to_string()],
        current_task: None,
        suggested_response: None,
        usage: OmInferenceUsage {
            input_tokens: 100,
            output_tokens: 20,
        },
    };
    let encoded = serde_json::to_value(&response).expect("serialize");
    assert!(encoded.get("current_task").is_none());
    assert!(encoded.get("suggested_response").is_none());
    let decoded = serde_json::from_value::<OmObserverResponse>(encoded).expect("deserialize");
    assert_eq!(decoded, response);
}

#[test]
fn reflector_request_and_response_roundtrip_support_optional_fields() {
    let request = OmReflectorRequest {
        scope: OmScope::Resource,
        scope_key: "resource:r-1".to_string(),
        model: sample_model(),
        generation_count: 7,
        active_observations: "obs-a\nobs-b".to_string(),
    };
    let request_encoded = serde_json::to_value(&request).expect("serialize request");
    let request_decoded =
        serde_json::from_value::<OmReflectorRequest>(request_encoded).expect("deserialize request");
    assert_eq!(request_decoded, request);

    let response = OmReflectorResponse {
        reflection: "condensed observations".to_string(),
        reflection_token_count: 55,
        reflected_observation_line_count: 12,
        current_task: Some("finish patch".to_string()),
        suggested_response: Some("apply and verify".to_string()),
        usage: OmInferenceUsage {
            input_tokens: 300,
            output_tokens: 55,
        },
    };
    let response_encoded = serde_json::to_value(&response).expect("serialize response");
    let response_decoded = serde_json::from_value::<OmReflectorResponse>(response_encoded)
        .expect("deserialize response");
    assert_eq!(response_decoded, response);
}
