use super::*;

#[test]
fn om_bridge_append_message_writes_session_scope_and_returns_scope_key() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let response = app
        .om_bridge_append_message(crate::om_bridge::OmMessageAppendRequestV1 {
            session_id: "s-om-bridge-session".to_string(),
            role: "user".to_string(),
            text: "x".repeat(26_000),
            scope_binding: None,
        })
        .expect("append");
    assert_eq!(response.session_id, "s-om-bridge-session");
    assert_eq!(response.scope_key, "session:s-om-bridge-session");
    assert!(!response.message_id.trim().is_empty());

    let record = app
        .state
        .get_om_record_by_scope_key("session:s-om-bridge-session")
        .expect("record lookup")
        .expect("record missing");
    assert_eq!(record.scope, crate::om::OmScope::Session);
}

#[test]
fn om_bridge_append_message_with_thread_scope_and_read_hint_state() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let append = app
        .om_bridge_append_message(crate::om_bridge::OmMessageAppendRequestV1 {
            session_id: "s-om-bridge-thread".to_string(),
            role: "user".to_string(),
            text: "x".repeat(26_000),
            scope_binding: Some(crate::om_bridge::OmScopeBindingInputV1 {
                scope: crate::om_bridge::OmScopeV1::Thread,
                thread_id: Some("t-om-bridge".to_string()),
                resource_id: Some("r-om-bridge".to_string()),
            }),
        })
        .expect("append thread");
    assert_eq!(append.scope_key, "thread:t-om-bridge");

    let replay = app.replay_outbox(50, false).expect("replay");
    assert!(replay.processed >= 1);

    let hint_state = app
        .om_bridge_read_hint_state(crate::om_bridge::OmHintReadRequestV1 {
            session_id: "s-om-bridge-thread".to_string(),
            scope_binding: None,
        })
        .expect("read hint")
        .expect("hint state");
    assert_eq!(hint_state.scope_key, "thread:t-om-bridge");
}

#[test]
fn om_bridge_read_hint_state_accepts_explicit_scope_binding() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let now = chrono::Utc::now();
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-bridge-read-thread".to_string(),
            scope: crate::om::OmScope::Thread,
            scope_key: "thread:t-om-bridge-read".to_string(),
            session_id: None,
            thread_id: Some("t-om-bridge-read".to_string()),
            resource_id: Some("r-om-bridge-read".to_string()),
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "thread scoped hint".to_string(),
            observation_token_count: 12,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 1,
            reflector_trigger_count_total: 1,
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
        })
        .expect("upsert om");

    let hint_state = app
        .om_bridge_read_hint_state(crate::om_bridge::OmHintReadRequestV1 {
            session_id: "s-om-bridge-read".to_string(),
            scope_binding: Some(crate::om_bridge::OmScopeBindingInputV1 {
                scope: crate::om_bridge::OmScopeV1::Thread,
                thread_id: Some("t-om-bridge-read".to_string()),
                resource_id: Some("r-om-bridge-read".to_string()),
            }),
        })
        .expect("read hint")
        .expect("hint state");
    assert_eq!(hint_state.scope_key, "thread:t-om-bridge-read");
    assert!(
        hint_state
            .hint
            .as_deref()
            .is_some_and(|value| value.contains("thread scoped hint"))
    );
}

#[test]
fn om_bridge_read_hint_state_rejects_invalid_scope_binding() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let err = app
        .om_bridge_read_hint_state(crate::om_bridge::OmHintReadRequestV1 {
            session_id: "s-om-bridge-read-invalid".to_string(),
            scope_binding: Some(crate::om_bridge::OmScopeBindingInputV1 {
                scope: crate::om_bridge::OmScopeV1::Thread,
                thread_id: None,
                resource_id: None,
            }),
        })
        .expect_err("must reject");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn om_bridge_append_message_rejects_empty_session_id() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let err = app
        .om_bridge_append_message(crate::om_bridge::OmMessageAppendRequestV1 {
            session_id: "   ".to_string(),
            role: "user".to_string(),
            text: "message".to_string(),
            scope_binding: None,
        })
        .expect_err("must reject");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn om_bridge_enqueue_observe_and_replay_applies_observation_chunk() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let session = app.session(Some("s-om-bridge-observe"));
    session.load().expect("load");
    session
        .add_message("user", "observe bridge request message")
        .expect("append");

    let scope_key = "session:s-om-bridge-observe";
    let record = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record lookup")
        .expect("record");

    let enqueue = app
        .om_bridge_enqueue_observe_request(crate::om_bridge::OmObserveBufferRequestedV1::new(
            scope_key,
            record.generation_count,
            chrono::Utc::now().to_rfc3339(),
            Some("s-om-bridge-observe"),
        ))
        .expect("enqueue observe");
    assert_eq!(enqueue.event_type, "om_observe_buffer_requested");
    assert_eq!(enqueue.scope_key, scope_key);

    let replay = app
        .om_bridge_replay(&crate::om_bridge::OmReplayRequestV1 {
            limit: 20,
            include_dead_letter: false,
            mode: crate::om_bridge::OmReplayModeV1::Full,
        })
        .expect("replay");
    assert!(replay.done >= 1);
    assert!(replay.scanned_count.is_none());
    assert!(replay.om_candidate_count.is_none());

    let record_after = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record after")
        .expect("record after missing");
    let chunks = app
        .state
        .list_om_observation_chunks(&record_after.id)
        .expect("chunks");
    assert!(!chunks.is_empty());
}

#[test]
fn om_bridge_enqueue_reflect_and_replay_applies_generation_cas() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let now = chrono::Utc::now();
    let scope_key = "session:s-om-bridge-reflect";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-bridge-reflect-record".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-bridge-reflect".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-a\nobs-b\nobs-c".to_string(),
            observation_token_count: 100_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: true,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed record");

    let enqueue = app
        .om_bridge_enqueue_reflect_request(crate::om_bridge::OmReflectRequestedV1::new(
            scope_key,
            0,
            chrono::Utc::now().to_rfc3339(),
        ))
        .expect("enqueue reflect");
    assert_eq!(enqueue.event_type, "om_reflect_requested");

    let replay = app
        .om_bridge_replay(&crate::om_bridge::OmReplayRequestV1 {
            limit: 20,
            include_dead_letter: false,
            mode: crate::om_bridge::OmReplayModeV1::Full,
        })
        .expect("replay");
    assert!(replay.done >= 1);

    let record_after = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record after")
        .expect("record after missing");
    assert_eq!(record_after.generation_count, 1);
    assert_eq!(
        record_after.last_applied_outbox_event_id,
        Some(enqueue.event_id)
    );
}

#[test]
fn om_bridge_enqueue_observe_rejects_schema_mismatch() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let err = app
        .om_bridge_enqueue_observe_request(crate::om_bridge::OmObserveBufferRequestedV1 {
            schema_version: 99,
            scope_key: "session:s-om-bridge-schema".to_string(),
            expected_generation: 0,
            requested_at: chrono::Utc::now().to_rfc3339(),
            session_id: Some("s-om-bridge-schema".to_string()),
        })
        .expect_err("must reject");
    assert!(matches!(err, AxiomError::Validation(_)));
}

#[test]
fn om_bridge_replay_om_only_does_not_process_non_om_events() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let non_om_event_id = app
        .state
        .enqueue(
            "semantic_scan",
            "axiom://resources/non-om",
            serde_json::json!({}),
        )
        .expect("enqueue non-om");

    let now = chrono::Utc::now();
    let scope_key = "session:s-om-bridge-mode";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-bridge-mode-record".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-bridge-mode".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-a\nobs-b".to_string(),
            observation_token_count: 90_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: true,
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
        })
        .expect("seed om");

    let om_enqueue = app
        .om_bridge_enqueue_reflect_request(crate::om_bridge::OmReflectRequestedV1::new(
            scope_key,
            0,
            chrono::Utc::now().to_rfc3339(),
        ))
        .expect("enqueue om");

    let replay = app
        .om_bridge_replay(&crate::om_bridge::OmReplayRequestV1 {
            limit: 20,
            include_dead_letter: false,
            mode: crate::om_bridge::OmReplayModeV1::OmOnly,
        })
        .expect("replay om-only");
    assert!(replay.done >= 1);
    assert_eq!(replay.scanned_count, Some(2));
    assert_eq!(replay.om_candidate_count, Some(1));

    let non_om_event = app
        .state
        .get_outbox_event(non_om_event_id)
        .expect("non-om lookup")
        .expect("non-om missing");
    assert_eq!(non_om_event.status, "new");

    let om_event = app
        .state
        .get_outbox_event(om_enqueue.event_id)
        .expect("om lookup")
        .expect("om missing");
    assert_eq!(om_event.status, "done");
}

#[test]
fn om_bridge_replay_om_only_clears_reflection_flags_after_dead_lettered_reflect_event() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let now = chrono::Utc::now();
    let scope_key = "session:s-om-bridge-deadletter-cleanup";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-bridge-deadletter-cleanup-record".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-bridge-deadletter-cleanup".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-a\nobs-b".to_string(),
            observation_token_count: 90_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: true,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om");

    let event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-bridge-deadletter-cleanup",
            serde_json::json!({
                "schema_version": 99,
                "scope_key": scope_key,
                "expected_generation": 0,
                "requested_at": chrono::Utc::now().to_rfc3339(),
            }),
        )
        .expect("enqueue malformed reflect");

    let replay = app
        .om_bridge_replay(&crate::om_bridge::OmReplayRequestV1 {
            limit: 20,
            include_dead_letter: false,
            mode: crate::om_bridge::OmReplayModeV1::OmOnly,
        })
        .expect("replay om-only");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let record_after = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record after lookup")
        .expect("record after missing");
    assert!(!record_after.is_reflecting);
    assert!(!record_after.is_buffering_reflection);

    let event_after = app
        .state
        .get_outbox_event(event_id)
        .expect("event lookup")
        .expect("event missing");
    assert_eq!(event_after.status, "dead_letter");
}

#[test]
fn om_bridge_replay_om_only_does_not_clear_reflection_flags_on_generation_mismatch() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let now = chrono::Utc::now();
    let scope_key = "session:s-om-bridge-deadletter-mismatch";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-bridge-deadletter-mismatch-record".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-bridge-deadletter-mismatch".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 1,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-a\nobs-b".to_string(),
            observation_token_count: 90_000,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: true,
            is_buffering_observation: false,
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om");

    let _event_id = app
        .state
        .enqueue(
            "om_reflect_requested",
            "axiom://session/s-om-bridge-deadletter-mismatch",
            serde_json::json!({
                "schema_version": 99,
                "scope_key": scope_key,
                "expected_generation": 0,
                "requested_at": chrono::Utc::now().to_rfc3339(),
            }),
        )
        .expect("enqueue malformed reflect");

    let replay = app
        .om_bridge_replay(&crate::om_bridge::OmReplayRequestV1 {
            limit: 20,
            include_dead_letter: false,
            mode: crate::om_bridge::OmReplayModeV1::OmOnly,
        })
        .expect("replay om-only");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let record_after = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record after lookup")
        .expect("record after missing");
    assert!(record_after.is_reflecting);
    assert!(record_after.is_buffering_reflection);
}

#[test]
fn om_bridge_replay_om_only_clears_reflection_flags_after_dead_lettered_reflect_buffer_event() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("init");

    let now = chrono::Utc::now();
    let scope_key = "session:s-om-bridge-deadletter-buffer-cleanup";
    app.state
        .upsert_om_record(&crate::om::OmRecord {
            id: "om-bridge-deadletter-buffer-cleanup-record".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-bridge-deadletter-buffer-cleanup".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "obs-a\nobs-b".to_string(),
            observation_token_count: 90_000,
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
            is_buffering_reflection: true,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        })
        .expect("seed om");

    let _event_id = app
        .state
        .enqueue(
            "om_reflect_buffer_requested",
            "axiom://session/s-om-bridge-deadletter-buffer-cleanup",
            serde_json::json!({
                "schema_version": 99,
                "scope_key": scope_key,
                "expected_generation": 0,
                "requested_at": chrono::Utc::now().to_rfc3339(),
            }),
        )
        .expect("enqueue malformed reflect buffer");

    let replay = app
        .om_bridge_replay(&crate::om_bridge::OmReplayRequestV1 {
            limit: 20,
            include_dead_letter: false,
            mode: crate::om_bridge::OmReplayModeV1::OmOnly,
        })
        .expect("replay om-only");
    assert_eq!(replay.dead_letter, 1);
    assert_eq!(replay.requeued, 0);

    let record_after = app
        .state
        .get_om_record_by_scope_key(scope_key)
        .expect("record after lookup")
        .expect("record after missing");
    assert!(!record_after.is_reflecting);
    assert!(!record_after.is_buffering_reflection);
}
