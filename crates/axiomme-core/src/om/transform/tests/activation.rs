use super::*;

#[test]
fn dynamic_threshold_without_shared_budget_returns_base() {
    assert_eq!(calculate_dynamic_threshold(30_000, None, 12_345), 30_000);
}

#[test]
fn dynamic_threshold_with_shared_budget_uses_remaining_space() {
    assert_eq!(
        calculate_dynamic_threshold(30_000, Some(70_000), 10_000),
        60_000
    );
    assert_eq!(
        calculate_dynamic_threshold(30_000, Some(70_000), 50_000),
        30_000
    );
}

#[test]
fn select_activation_boundary_prefers_under_target_boundary() {
    let chunks = vec![
        chunk(1, 1000, 200, &["m1"]),
        chunk(2, 1200, 250, &["m2"]),
        chunk(3, 1800, 300, &["m3"]),
    ];

    let boundary = select_activation_boundary(&chunks, 0.8, 5000, 6000);
    assert_eq!(boundary.chunks_activated, 3);
    assert_eq!(boundary.message_tokens_activated, 4000);
    assert_eq!(boundary.observation_tokens_activated, 750);
    assert_eq!(boundary.activated_message_ids, vec!["m1", "m2", "m3"]);
}

#[test]
fn select_activation_boundary_returns_zero_for_empty_chunks() {
    let boundary = select_activation_boundary(&[], 0.8, 5000, 6000);
    assert_eq!(boundary.chunks_activated, 0);
    assert_eq!(boundary.message_tokens_activated, 0);
}

#[test]
fn select_activation_boundary_saturates_token_totals_to_u32_max() {
    let chunks = vec![
        chunk(1, u32::MAX, u32::MAX, &["m1"]),
        chunk(2, u32::MAX, u32::MAX, &["m2"]),
    ];
    let boundary = select_activation_boundary(&chunks, 1.0, 1, u32::MAX);
    assert_eq!(boundary.message_tokens_activated, u32::MAX);
    assert_eq!(boundary.observation_tokens_activated, u32::MAX);
}

#[test]
fn merge_activated_observations_appends_chunk_observations() {
    let chunks = vec![chunk(1, 1000, 200, &["m1"]), chunk(2, 1200, 250, &["m2"])];
    let merged = merge_activated_observations("active", &chunks);
    assert_eq!(merged, "active\n\nobs-1\n\nobs-2");
}
#[test]
fn normalize_observation_buffer_boundary_clamps_to_current_tokens() {
    assert_eq!(normalize_observation_buffer_boundary(100, 80), 80);
    assert_eq!(normalize_observation_buffer_boundary(100, 140), 100);
}

#[test]
fn activate_buffered_observations_updates_record_and_chunk_state() {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut record = OmRecord {
        id: "r1".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s1".to_string(),
        session_id: Some("s1".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: "active".to_string(),
        observation_token_count: 10,
        pending_message_tokens: 6_000,
        last_observed_at: None,
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: vec!["existing".to_string()],
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 0,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: true,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 6_000,
        last_buffered_at_time: Some(now),
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    let mut chunks = vec![
        OmObservationChunk {
            id: "c1".to_string(),
            record_id: "r1".to_string(),
            seq: 1,
            cycle_id: "cycle-1".to_string(),
            observations: "obs-1".to_string(),
            token_count: 200,
            message_tokens: 1_000,
            message_ids: vec!["m1".to_string()],
            last_observed_at: now,
            created_at: now,
        },
        OmObservationChunk {
            id: "c2".to_string(),
            record_id: "r1".to_string(),
            seq: 2,
            cycle_id: "cycle-2".to_string(),
            observations: "obs-2".to_string(),
            token_count: 250,
            message_tokens: 1_200,
            message_ids: vec!["m2".to_string()],
            last_observed_at: now + Duration::minutes(1),
            created_at: now + Duration::minutes(1),
        },
    ];

    let activated =
        activate_buffered_observations(&mut record, &mut chunks, 0.8, 5_000).expect("activate");
    assert_eq!(activated.activated_max_seq, 2);
    assert_eq!(activated.chunks_activated, 2);
    assert_eq!(record.active_observations, "active\n\nobs-1\n\nobs-2");
    assert_eq!(record.observation_token_count, 460);
    assert_eq!(record.pending_message_tokens, 3_800);
    assert_eq!(
        record.last_activated_message_ids,
        vec!["existing".to_string(), "m1".to_string(), "m2".to_string()]
    );
    assert!(!record.is_buffering_observation);
    assert_eq!(record.last_buffered_at_tokens, 0);
    assert_eq!(record.last_buffered_at_time, None);
    assert!(chunks.is_empty());
}

#[test]
fn activate_buffered_observations_returns_result_for_seq_zero_chunks() {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut record = OmRecord {
        id: "r-seq0".to_string(),
        scope: OmScope::Session,
        scope_key: "session:s1".to_string(),
        session_id: Some("s1".to_string()),
        thread_id: None,
        resource_id: None,
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: String::new(),
        observation_token_count: 0,
        pending_message_tokens: 1_000,
        last_observed_at: None,
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 0,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: true,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 1_000,
        last_buffered_at_time: Some(now),
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    };
    let mut chunks = vec![OmObservationChunk {
        id: "c-seq0".to_string(),
        record_id: "r-seq0".to_string(),
        seq: 0,
        cycle_id: "cycle-0".to_string(),
        observations: "obs-seq0".to_string(),
        token_count: 50,
        message_tokens: 1_000,
        message_ids: vec!["m-seq0".to_string()],
        last_observed_at: now,
        created_at: now,
    }];

    let activated =
        activate_buffered_observations(&mut record, &mut chunks, 0.8, 5_000).expect("activate");
    assert_eq!(activated.activated_max_seq, 0);
    assert_eq!(activated.chunks_activated, 1);
    assert_eq!(record.active_observations, "obs-seq0");
    assert_eq!(record.observation_token_count, 50);
    assert_eq!(record.pending_message_tokens, 0);
    assert!(chunks.is_empty());
}
