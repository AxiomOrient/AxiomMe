use axiomme_core::om::{
    BufferTokensInput, ObservationConfigInput, OmConfigInput, OmObserverMessageCandidate,
    OmOriginType, OmParseMode, OmPendingMessage, OmScope, ProcessInputStepOptions,
    ReflectionConfigInput, aggregate_multi_thread_observer_sections, parse_memory_section_xml,
    parse_memory_section_xml_accuracy_first, parse_multi_thread_observer_output,
    parse_multi_thread_observer_output_accuracy_first, plan_process_input_step, resolve_om_config,
    select_observed_message_candidates, synthesize_observer_observations,
};
use chrono::Utc;

// NOTE: Keep this helper explicit so end-to-end behavior is easy to reason about.
fn sample_record(
    observation_token_count: u32,
    pending_message_tokens: u32,
) -> axiomme_core::om::OmRecord {
    let now = Utc::now();
    axiomme_core::om::OmRecord {
        id: "om-runtime-validation".to_string(),
        scope: OmScope::Thread,
        scope_key: "thread:runtime-validation".to_string(),
        session_id: Some("s-main".to_string()),
        thread_id: Some("t-main".to_string()),
        resource_id: None,
        generation_count: 1,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "* baseline observation".to_string(),
        observation_token_count,
        pending_message_tokens,
        last_observed_at: Some(now),
        current_task: Some("Primary: validation".to_string()),
        suggested_response: None,
        last_activated_message_ids: vec!["m-0".to_string()],
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
fn runtime_parses_structured_multi_thread_payload() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"s-main\">\n",
        "Date: 2026-02-14\n",
        "* 🔴 User asked for deterministic validation\n",
        "<current-task>Primary: verify OM behavior</current-task>\n",
        "<suggested-response>Report concrete test results</suggested-response>\n",
        "</thread>\n",
        "<thread id=\"s-peer\">\n",
        "* 🟡 Background indexing in progress\n",
        "</thread>\n",
        "</observations>\n",
    );
    let sections = parse_multi_thread_observer_output_accuracy_first(text);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].thread_id, "s-main");
    assert!(
        sections[0]
            .observations
            .contains("User asked for deterministic validation")
    );
    let aggregate = aggregate_multi_thread_observer_sections(&sections, Some("s-main"));
    assert!(aggregate.observations.contains("<thread id=\"s-main\">"));
    assert_eq!(
        aggregate.current_task.as_deref(),
        Some("Primary: verify OM behavior")
    );
    assert_eq!(
        aggregate.suggested_response.as_deref(),
        Some("Report concrete test results")
    );
}

#[test]
fn runtime_prefers_lenient_only_when_strict_has_no_observation_signal() {
    let malformed = concat!(
        "<observations>\n",
        "<thread id=\"broken\">\n",
        "missing close before next thread\n",
        "<thread id='s-main'>\n",
        "* 🟡 recovered line\n",
        "</thread>\n",
        "</observations>\n",
    );
    let strict = parse_multi_thread_observer_output(malformed, OmParseMode::Strict);
    let accuracy = parse_multi_thread_observer_output_accuracy_first(malformed);
    assert!(strict.is_empty());
    assert_eq!(accuracy.len(), 1);
    assert_eq!(accuracy[0].thread_id, "s-main");
}

#[test]
fn runtime_handles_garbage_input_without_panic_or_nondeterminism() {
    let garbage_cases = [
        "",
        "just plain text without tags",
        "<<<>>> < / > random",
        "<observations><thread id=\"x\">unterminated",
        "<current-task>inline </current-task> literal <suggested-response>x",
        "0\x0b\x0c binary-like control chars are ignored safely",
    ];

    for case in garbage_cases {
        let a = parse_memory_section_xml_accuracy_first(case);
        let b = parse_memory_section_xml_accuracy_first(case);
        assert_eq!(a, b);

        let threads_a = parse_multi_thread_observer_output_accuracy_first(case);
        let threads_b = parse_multi_thread_observer_output_accuracy_first(case);
        assert_eq!(threads_a, threads_b);
    }
}

#[test]
fn runtime_pipeline_decision_matches_realistic_load_thresholds() {
    let resolved = resolve_om_config(OmConfigInput {
        scope: OmScope::Thread,
        share_token_budget: false,
        observation: ObservationConfigInput {
            message_tokens: Some(30_000),
            max_tokens_per_batch: Some(10_000),
            buffer_tokens: Some(BufferTokensInput::Ratio(0.2)),
            buffer_activation: Some(0.8),
            block_after: Some(1.2),
        },
        reflection: ReflectionConfigInput {
            observation_tokens: Some(40_000),
            buffer_activation: Some(0.5),
            block_after: Some(1.2),
        },
    })
    .expect("config must resolve");
    let record = sample_record(8_000, 36_000);

    let plan = plan_process_input_step(
        &record,
        resolved.observation,
        resolved.reflection,
        &Utc::now().to_rfc3339(),
        ProcessInputStepOptions {
            is_initial_step: true,
            read_only: false,
            has_buffered_observation_chunks: true,
        },
    );

    assert!(plan.should_run_observer);
    assert!(plan.should_activate_buffered_before_observer);
    assert!(plan.should_activate_buffered_after_observer);
    assert!(plan.reflection_decision.is_some());
}

#[test]
fn runtime_selects_observed_candidates_from_mixed_and_noisy_ids() {
    let now = Utc::now();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "first".to_string(),
            created_at: now,
            source_thread_id: Some("t-main".to_string()),
            source_session_id: Some("s-main".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "second".to_string(),
            created_at: now,
            source_thread_id: Some("t-main".to_string()),
            source_session_id: Some("s-main".to_string()),
        },
    ];
    let selected =
        select_observed_message_candidates(&candidates, &["noise".to_string(), "m2".to_string()]);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, "m2");
}

#[test]
fn runtime_synthesizes_observations_with_dedup_and_forward_progress() {
    let pending = vec![
        OmPendingMessage {
            id: "p1".to_string(),
            role: "user".to_string(),
            text: "Need strict validation".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "p2".to_string(),
            role: "user".to_string(),
            text: "Need strict validation".to_string(),
            created_at_rfc3339: None,
        },
    ];
    let synthesized =
        synthesize_observer_observations("[user] Need strict validation", &pending, 200);
    assert!(synthesized.contains("[user] Need strict validation"));
}

#[test]
fn runtime_parses_precise_single_section_xml_with_metadata() {
    let precise = concat!(
        "<observations>\n",
        "* 🔴 User prefers explicit data contracts\n",
        "</observations>\n",
        "<current-task>Primary: validate parser determinism</current-task>\n",
        "<suggested-response>Provide test evidence only</suggested-response>\n",
    );
    let parsed = parse_memory_section_xml(precise, OmParseMode::Strict);
    let parsed_accuracy = parse_memory_section_xml_accuracy_first(precise);
    assert_eq!(parsed.observations, parsed_accuracy.observations);
    assert_eq!(
        parsed.current_task.as_deref(),
        Some("Primary: validate parser determinism")
    );
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Provide test evidence only")
    );
}
