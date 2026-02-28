use chrono::Utc;

use crate::om::{OmOriginType, OmScope};

use super::*;

fn reflector_config(mode: OmReflectorMode, model_enabled: bool) -> OmReflectorConfig {
    OmReflectorConfig {
        mode,
        model_enabled,
        llm_endpoint: "http://127.0.0.1:11434/api/chat".to_string(),
        llm_model: "qwen2.5:7b-instruct".to_string(),
        llm_timeout_ms: DEFAULT_OM_REFLECTOR_LLM_TIMEOUT_MS,
        llm_max_output_tokens: DEFAULT_OM_REFLECTOR_LLM_MAX_OUTPUT_TOKENS,
        llm_temperature_milli: DEFAULT_OM_REFLECTOR_LLM_TEMPERATURE_MILLI,
        llm_strict: false,
        llm_target_observation_tokens: DEFAULT_REFLECTOR_OBSERVATION_TOKENS,
        llm_buffer_activation: DEFAULT_REFLECTOR_BUFFER_ACTIVATION,
        max_chars: DEFAULT_OM_REFLECTOR_MAX_CHARS,
    }
}

fn om_record(active_observations: &str) -> crate::om::OmRecord {
    let now = Utc::now();
    crate::om::OmRecord {
        id: "om-reflector-test".to_string(),
        scope: OmScope::Session,
        scope_key: "session:reflector-test".to_string(),
        session_id: Some("reflector-test".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: active_observations.to_string(),
        observation_token_count: 10,
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

#[test]
fn parse_reflector_response_value_reads_object_payload() {
    let payload = serde_json::json!({
        "reflection": "one two three",
        "reflected_observation_line_count": 3,
        "usage": {"input_tokens": 11, "output_tokens": 7},
        "reflection_token_count": 4,
        "current_task": "Primary: summarize",
        "suggested_response": "Ask for confirmation"
    });
    let parsed =
        parse_reflector_response_value(&payload, "a\nb\nc", DEFAULT_OM_REFLECTOR_MAX_CHARS)
            .expect("parsed");
    assert_eq!(parsed.reflection, "one two three");
    assert_eq!(parsed.reflected_observation_line_count, 3);
    assert_eq!(parsed.usage.input_tokens, 11);
    assert_eq!(parsed.usage.output_tokens, 7);
    assert_eq!(parsed.reflection_token_count, 4);
    assert_eq!(parsed.current_task, None);
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Ask for confirmation")
    );
}

#[test]
fn parse_reflector_response_value_accepts_mastra_alias_fields() {
    let payload = serde_json::json!({
        "observations": "compact summary",
        "reflectedObservationLineCount": 2,
        "usage": {"inputTokens": 9, "outputTokens": 4},
        "reflectionTokenCount": 3,
        "currentTask": "Primary: summarize",
        "suggestedContinuation": "Wait for user confirmation"
    });
    let parsed =
        parse_reflector_response_value(&payload, "a\nb\nc", DEFAULT_OM_REFLECTOR_MAX_CHARS)
            .expect("parsed");
    assert_eq!(parsed.reflection, "compact summary");
    assert_eq!(parsed.reflected_observation_line_count, 2);
    assert_eq!(parsed.usage.input_tokens, 9);
    assert_eq!(parsed.usage.output_tokens, 4);
    assert_eq!(parsed.reflection_token_count, 3);
    assert_eq!(parsed.current_task, None);
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Wait for user confirmation")
    );
}

#[test]
fn parse_llm_reflector_response_accepts_embedded_json_content() {
    let payload = serde_json::json!({
        "message": {
            "content": "```json\n{\"reflection\":\"compact summary\",\"reflected_observation_line_count\":2}\n```"
        }
    });
    let parsed = parse_llm_reflector_response(
        &payload,
        "line-1\nline-2\nline-3",
        DEFAULT_OM_REFLECTOR_MAX_CHARS,
    )
    .expect("parsed");
    assert_eq!(parsed.reflection, "compact summary");
    assert_eq!(parsed.reflected_observation_line_count, 2);
}

#[test]
fn parse_llm_reflector_response_accepts_xml_observations_content() {
    let payload = serde_json::json!({
        "message": {
            "content": "<observations>\n* 🔴 user prefers direct answers\n* 🟡 agent updated auth flow\n</observations>\n<current-task>\nPrimary: debug auth\n</current-task>\n<suggested-response>\nAsk user to confirm\n</suggested-response>"
        }
    });
    let parsed = parse_llm_reflector_response(
        &payload,
        "line-1\nline-2\nline-3",
        DEFAULT_OM_REFLECTOR_MAX_CHARS,
    )
    .expect("parsed");
    assert!(parsed.reflection.contains("user prefers direct answers"));
    assert!(parsed.reflection.contains("agent updated auth flow"));
    assert_eq!(parsed.reflected_observation_line_count, 3);
    assert!(parsed.reflection_token_count > 0);
    assert_eq!(parsed.current_task, None);
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Ask user to confirm")
    );
}

#[test]
fn parse_llm_reflector_response_accepts_list_items_without_xml_tags() {
    let payload = serde_json::json!({
        "message": {
            "content": "* 🔴 user prefers direct answers\n- 🟡 agent updated auth flow\n1. 🟢 assistant suggested follow-up"
        }
    });
    let parsed = parse_llm_reflector_response(
        &payload,
        "line-1\nline-2\nline-3",
        DEFAULT_OM_REFLECTOR_MAX_CHARS,
    )
    .expect("parsed");
    assert!(parsed.reflection.contains("user prefers direct answers"));
    assert!(parsed.reflection.contains("agent updated auth flow"));
    assert!(parsed.reflection.contains("assistant suggested follow-up"));
}

#[test]
fn parse_llm_reflector_response_uses_trimmed_content_when_no_xml_or_list() {
    let payload = serde_json::json!({
        "message": {
            "content": "keep concise summary for future turns"
        }
    });
    let parsed = parse_llm_reflector_response(
        &payload,
        "line-1\nline-2\nline-3",
        DEFAULT_OM_REFLECTOR_MAX_CHARS,
    )
    .expect("parsed");
    assert_eq!(parsed.reflection, "keep concise summary for future turns");
}

#[test]
fn parse_llm_reflector_response_returns_schema_taxonomy_for_invalid_payload() {
    let payload = serde_json::json!({"unexpected": "shape"});
    let err = parse_llm_reflector_response(
        &payload,
        "line-1\nline-2\nline-3",
        DEFAULT_OM_REFLECTOR_MAX_CHARS,
    )
    .expect_err("must fail");
    match err {
        AxiomError::OmInference {
            inference_source,
            kind,
            ..
        } => {
            assert_eq!(inference_source, OmInferenceSource::Reflector);
            assert_eq!(kind, OmInferenceFailureKind::Schema);
        }
        other => panic!("unexpected error type: {other}"),
    }
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
fn deterministic_reflector_response_handles_empty_input() {
    let parsed = deterministic_reflector_response(" \n \n", DEFAULT_OM_REFLECTOR_MAX_CHARS);
    assert!(parsed.reflection.is_empty());
    assert_eq!(parsed.reflected_observation_line_count, 0);
    assert_eq!(parsed.reflection_token_count, 0);
}

#[test]
fn reflector_model_feature_flag_off_forces_deterministic_output() {
    let record = om_record("line-1\nline-2");
    let config = reflector_config(OmReflectorMode::Llm, false);

    let resolved = resolve_reflector_response_with_config(
        &record,
        "session:reflector-test",
        0,
        OmReflectorCallOptions::DEFAULT,
        &config,
    )
    .expect("resolve");
    let expected = deterministic_reflector_response(
        &record.active_observations,
        DEFAULT_OM_REFLECTOR_MAX_CHARS,
    );

    assert_eq!(resolved.reflection, expected.reflection);
    assert_eq!(
        resolved.reflected_observation_line_count,
        expected.reflected_observation_line_count
    );
    assert_eq!(
        resolved.reflection_token_count,
        expected.reflection_token_count
    );
}

#[test]
fn reflector_rollout_profile_overrides_model_enabled_flag() {
    assert!(!resolve_reflector_model_enabled(
        true,
        Some("observer_only")
    ));
    assert!(resolve_reflector_model_enabled(false, Some("full_model")));
    assert!(!resolve_reflector_model_enabled(true, Some("baseline")));
    assert!(resolve_reflector_model_enabled(true, Some("unknown")));
    assert!(!resolve_reflector_model_enabled(false, Some("unknown")));
}

#[test]
fn prepare_reflector_attempt_input_buffered_uses_slice_plan() {
    let mut record = om_record("l1\nl2\nl3\nl4");
    record.observation_token_count = 100;
    let mut config = reflector_config(OmReflectorMode::Deterministic, true);
    config.llm_target_observation_tokens = 80;
    config.llm_buffer_activation = 0.5;

    let prepared =
        prepare_reflector_attempt_input(&record, OmReflectorCallOptions::BUFFERED, &config);
    assert_eq!(prepared.active_observations, "l1");
    assert_eq!(prepared.target_threshold_tokens, 13);
    assert_eq!(prepared.reflected_observation_line_count_override, Some(1));
    assert_eq!(prepared.reflection_input_tokens_override, Some(25));
}

#[test]
fn prepare_reflector_attempt_input_default_uses_full_observations() {
    let record = om_record("line-1\nline-2");
    let config = reflector_config(OmReflectorMode::Deterministic, true);

    let prepared =
        prepare_reflector_attempt_input(&record, OmReflectorCallOptions::DEFAULT, &config);
    assert_eq!(prepared.active_observations, "line-1\nline-2");
    assert_eq!(
        prepared.target_threshold_tokens,
        DEFAULT_REFLECTOR_OBSERVATION_TOKENS
    );
    assert_eq!(prepared.reflected_observation_line_count_override, None);
    assert_eq!(prepared.reflection_input_tokens_override, None);
}

#[test]
fn validate_reflection_compression_is_strictly_less_than_target() {
    assert!(validate_reflection_compression(39_999, 40_000));
    assert!(!validate_reflection_compression(40_000, 40_000));
    assert!(!validate_reflection_compression(40_001, 40_000));
}

#[test]
fn build_reflector_user_prompt_includes_guidance_and_skip_sections() {
    let prompt = build_reflector_user_prompt(OmReflectorPromptInput {
        observations: "* High user prefers direct answers",
        request_json: Some("{}"),
        manual_prompt: None,
        compression_level: 2,
        skip_continuation_hints: true,
    });
    assert!(prompt.contains("AGGRESSIVE COMPRESSION REQUIRED"));
    assert!(prompt.contains("Do NOT include <current-task> or <suggested-response>"));
}
