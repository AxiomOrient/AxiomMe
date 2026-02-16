use chrono::{TimeZone, Utc};

use super::*;
use crate::om::{OmOriginType, OmScope};

fn sample_record() -> OmRecord {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    OmRecord {
        id: "record-1".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s-1".to_string(),
        session_id: Some("s-1".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: String::new(),
        observation_token_count: 0,
        pending_message_tokens: 31_000,
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
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn process_input_step_plan_matches_initial_step_activation_semantics() {
    let record = sample_record();
    let plan = plan_process_input_step(
        &record,
        ResolvedObservationConfig {
            message_tokens_base: 30_000,
            total_budget: None,
            max_tokens_per_batch: 10_000,
            buffer_tokens: Some(6_000),
            buffer_activation: Some(0.8),
            block_after: Some(36_000),
        },
        ResolvedReflectionConfig {
            observation_tokens: 40_000,
            buffer_activation: Some(0.5),
            block_after: Some(48_000),
        },
        "2026-01-01T00:00:00Z",
        ProcessInputStepOptions {
            is_initial_step: true,
            read_only: false,
            has_buffered_observation_chunks: true,
        },
    );
    assert!(plan.should_activate_buffered_before_observer);
    assert!(plan.reflection_decision.is_some());
}

#[test]
fn process_output_result_plan_saves_only_when_writable_and_non_empty() {
    assert!(!plan_process_output_result(true, 2).should_save_unsaved_messages);
    assert!(!plan_process_output_result(false, 0).should_save_unsaved_messages);
    assert!(plan_process_output_result(false, 1).should_save_unsaved_messages);
}

#[test]
fn process_input_step_plan_read_only_disables_writes_and_reflection_decision() {
    let record = sample_record();
    let plan = plan_process_input_step(
        &record,
        ResolvedObservationConfig {
            message_tokens_base: 30_000,
            total_budget: None,
            max_tokens_per_batch: 10_000,
            buffer_tokens: Some(6_000),
            buffer_activation: Some(0.8),
            block_after: Some(36_000),
        },
        ResolvedReflectionConfig {
            observation_tokens: 40_000,
            buffer_activation: Some(0.5),
            block_after: Some(48_000),
        },
        "2026-01-01T00:00:00Z",
        ProcessInputStepOptions {
            is_initial_step: true,
            read_only: true,
            has_buffered_observation_chunks: true,
        },
    );
    assert!(!plan.should_activate_buffered_before_observer);
    assert!(!plan.should_run_observer);
    assert!(!plan.should_activate_buffered_after_observer);
    assert_eq!(plan.reflection_decision, None);
}

#[test]
fn process_input_step_plan_non_initial_step_does_not_activate_before_observer() {
    let record = sample_record();
    let plan = plan_process_input_step(
        &record,
        ResolvedObservationConfig {
            message_tokens_base: 30_000,
            total_budget: None,
            max_tokens_per_batch: 10_000,
            buffer_tokens: Some(6_000),
            buffer_activation: Some(0.8),
            block_after: Some(36_000),
        },
        ResolvedReflectionConfig {
            observation_tokens: 40_000,
            buffer_activation: Some(0.5),
            block_after: Some(48_000),
        },
        "2026-01-01T00:00:00Z",
        ProcessInputStepOptions {
            is_initial_step: false,
            read_only: false,
            has_buffered_observation_chunks: true,
        },
    );
    assert!(!plan.should_activate_buffered_before_observer);
    assert!(plan.should_run_observer);
    assert!(plan.reflection_decision.is_some());
}
