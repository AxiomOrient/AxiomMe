use super::*;

#[test]
fn synthesize_observer_observations_deduplicates_against_active_and_batch() {
    let pending = vec![
        OmPendingMessage {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "same text".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "m2".to_string(),
            role: "user".to_string(),
            text: "same text".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "m3".to_string(),
            role: "assistant".to_string(),
            text: "new detail".to_string(),
            created_at_rfc3339: None,
        },
    ];
    let out = synthesize_observer_observations("[user] same text", &pending, 256);
    assert_eq!(out, "[assistant] new detail");
}

#[test]
fn synthesize_observer_observations_uses_fallback_when_all_lines_deduped() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "same text".to_string(),
        created_at_rfc3339: None,
    }];
    let out = synthesize_observer_observations("[user] same text", &pending, 256);
    assert_eq!(out, "[user] same text");
}

#[test]
fn deterministic_continuation_emits_task_and_response_for_user_request() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "Please investigate queue replay drift and fix it".to_string(),
        created_at_rfc3339: None,
    }];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(
        task.as_deref(),
        Some("Please investigate queue replay drift and fix it")
    );
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("Respond to user request"))
    );
}

#[test]
fn deterministic_continuation_preserves_error_identifier_in_suggestion() {
    let pending = vec![
        OmPendingMessage {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "fix indexing pipeline for release".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "m2".to_string(),
            role: "tool".to_string(),
            text: "ERROR E409 conflict while writing index_state".to_string(),
            created_at_rfc3339: None,
        },
    ];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task.as_deref(), Some("fix indexing pipeline for release"));
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("E409") && value.contains("continue"))
    );
}

#[test]
fn deterministic_continuation_extracts_identifier_from_cjk_error_without_whitespace() {
    let pending = vec![
        OmPendingMessage {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "请修复索引漂移并继续".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "m2".to_string(),
            role: "tool".to_string(),
            text: "错误发生在worker:queue，操作失败".to_string(),
            created_at_rfc3339: None,
        },
    ];

    let (_, response) = infer_deterministic_continuation(&pending);
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("worker:queue")),
        "response was: {:?}",
        response
    );
}

#[test]
fn deterministic_continuation_returns_none_without_task_signal() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "assistant".to_string(),
        text: "background sync complete".to_string(),
        created_at_rfc3339: None,
    }];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task, None);
    assert_eq!(response, None);
}

#[test]
fn deterministic_continuation_recognizes_english_question_task_signal() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "Can you check this?".to_string(),
        created_at_rfc3339: None,
    }];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task.as_deref(), Some("Can you check this?"));
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("Respond to user request"))
    );
}

#[test]
fn deterministic_continuation_ignores_code_like_question_token_without_request_cues() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "status=ok?trace_id=E409".to_string(),
        created_at_rfc3339: None,
    }];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task, None);
    assert_eq!(response, None);
}

#[test]
fn deterministic_continuation_recognizes_korean_task_signal() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "큐 리플레이 드리프트를 조사하고 수정해줘".to_string(),
        created_at_rfc3339: None,
    }];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(
        task.as_deref(),
        Some("큐 리플레이 드리프트를 조사하고 수정해줘")
    );
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("사용자 요청에 응답"))
    );
}

#[test]
fn deterministic_continuation_recognizes_japanese_task_signal() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "設定ファイルを更新してください".to_string(),
        created_at_rfc3339: None,
    }];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task.as_deref(), Some("設定ファイルを更新してください"));
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("ユーザー要求に対応"))
    );
}

#[test]
fn deterministic_continuation_recognizes_chinese_task_signal() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "请修复索引漂移并更新配置".to_string(),
        created_at_rfc3339: None,
    }];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task.as_deref(), Some("请修复索引漂移并更新配置"));
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("回应用户请求"))
    );
}

#[test]
fn deterministic_continuation_uses_korean_error_template() {
    let pending = vec![
        OmPendingMessage {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "인덱스 동기화 문제를 확인하고 고쳐줘".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "m2".to_string(),
            role: "tool".to_string(),
            text: "오류 E_TIMEOUT 발생".to_string(),
            created_at_rfc3339: None,
        },
    ];

    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(
        task.as_deref(),
        Some("인덱스 동기화 문제를 확인하고 고쳐줘")
    );
    assert!(
        response.as_deref().is_some_and(|value| {
            value.contains("E_TIMEOUT") && value.contains("계속 진행")
        })
    );
}

#[test]
fn deterministic_continuation_accepts_localized_user_role_label() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "사용자".to_string(),
        text: "설정 파일 업데이트해줘".to_string(),
        created_at_rfc3339: None,
    }];
    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task.as_deref(), Some("설정 파일 업데이트해줘"));
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("사용자 요청에 응답"))
    );
}

#[test]
fn deterministic_continuation_ignores_non_task_korean_status_sentence() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "인덱스 동기화가 완료되었습니다".to_string(),
        created_at_rfc3339: None,
    }];
    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task, None);
    assert_eq!(response, None);
}

#[test]
fn deterministic_continuation_ignores_non_task_japanese_status_sentence() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "同期が完了しました".to_string(),
        created_at_rfc3339: None,
    }];
    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task, None);
    assert_eq!(response, None);
}

#[test]
fn deterministic_continuation_ignores_non_task_chinese_status_sentence() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "索引同步已完成".to_string(),
        created_at_rfc3339: None,
    }];
    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(task, None);
    assert_eq!(response, None);
}

#[test]
fn deterministic_continuation_handles_mixed_language_request() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "Please 큐 리플레이 드리프트를 조사하고 수정해줘".to_string(),
        created_at_rfc3339: None,
    }];
    let (task, response) = infer_deterministic_continuation(&pending);
    assert_eq!(
        task.as_deref(),
        Some("Please 큐 리플레이 드리프트를 조사하고 수정해줘")
    );
    assert!(
        response
            .as_deref()
            .is_some_and(|value| value.contains("사용자 요청에 응답"))
    );
}

#[test]
fn deterministic_observer_response_v2_emits_evidence_and_preserves_error_identifier() {
    let pending = vec![
        OmPendingMessage {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "fix indexing pipeline for release".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "m2".to_string(),
            role: "tool".to_string(),
            text: "ERROR E409 conflict while writing index_state".to_string(),
            created_at_rfc3339: None,
        },
    ];

    let response = infer_deterministic_observer_response("", &pending, 256);
    assert_eq!(
        response.current_task.as_deref(),
        Some("fix indexing pipeline for release")
    );
    assert!(
        response
            .suggested_response
            .as_deref()
            .is_some_and(|value| value.contains("E409"))
    );
    assert!(response.confidence_milli >= 700);
    assert_eq!(response.observed_message_ids, vec!["m1", "m2"]);
    assert!(response.evidence.iter().any(|item| {
        item.kind == OmDeterministicEvidenceKind::TaskSignal && item.message_id == "m1"
    }));
    assert!(response.evidence.iter().any(|item| {
        item.kind == OmDeterministicEvidenceKind::ErrorSignal && item.message_id == "m2"
    }));
    assert!(response.evidence.iter().any(|item| {
        item.kind == OmDeterministicEvidenceKind::ObservationLine && item.message_id == "m2"
    }));
}

#[test]
fn deterministic_observer_response_v2_suppresses_suggested_response_for_low_confidence_task() {
    let pending = vec![OmPendingMessage {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "Can you check this?".to_string(),
        created_at_rfc3339: None,
    }];

    let response = infer_deterministic_observer_response("", &pending, 256);
    assert_eq!(
        response.current_task.as_deref(),
        Some("Can you check this?")
    );
    assert_eq!(response.suggested_response, None);
    assert!(
        response.confidence_milli
            < crate::ContinuationPolicyV2::default().min_confidence_milli_for_suggested_response
    );
    assert!(response.evidence.iter().any(|item| {
        item.kind == OmDeterministicEvidenceKind::TaskSignal && item.message_id == "m1"
    }));
}

#[test]
fn select_observer_message_candidates_filters_and_keeps_recent_order() {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "a".to_string(),
            created_at: now,
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "b".to_string(),
            created_at: now + Duration::seconds(1),
            source_thread_id: Some("s2".to_string()),
            source_session_id: Some("s2".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m3".to_string(),
            role: "user".to_string(),
            text: "c".to_string(),
            created_at: now + Duration::seconds(2),
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
    ];
    let observed = vec!["m2".to_string()].into_iter().collect::<HashSet<_>>();
    let out = select_observer_message_candidates(&candidates, &observed, 2);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].id, "m1");
    assert_eq!(out[1].id, "m3");
}

#[test]
fn select_observer_message_candidates_is_deterministic_on_ties() {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m-b".to_string(),
            role: "user".to_string(),
            text: "b".to_string(),
            created_at: now,
            source_thread_id: Some("t-1".to_string()),
            source_session_id: Some("s-2".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-a".to_string(),
            role: "assistant".to_string(),
            text: "a".to_string(),
            created_at: now,
            source_thread_id: Some("t-1".to_string()),
            source_session_id: Some("s-1".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-c".to_string(),
            role: "assistant".to_string(),
            text: "c".to_string(),
            created_at: now,
            source_thread_id: Some("t-1".to_string()),
            source_session_id: Some("s-2".to_string()),
        },
    ];
    let out = select_observer_message_candidates(&candidates, &HashSet::new(), 10);
    assert_eq!(
        out.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
        vec!["m-a", "m-b", "m-c"]
    );
}

#[test]
fn select_observer_message_candidates_returns_empty_when_max_messages_zero() {
    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![OmObserverMessageCandidate {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "a".to_string(),
        created_at: now,
        source_thread_id: Some("s1".to_string()),
        source_session_id: Some("s1".to_string()),
    }];
    let out = select_observer_message_candidates(&candidates, &HashSet::new(), 0);
    assert!(out.is_empty());
}

#[test]
fn filter_observer_candidates_by_last_observed_at_keeps_messages_at_or_after_cutoff() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "old".to_string(),
            created_at: base,
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "new".to_string(),
            created_at: base + Duration::seconds(1),
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
    ];
    let out = filter_observer_candidates_by_last_observed_at(&candidates, Some(base));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].id, "m1");
    assert_eq!(out[1].id, "m2");
}

#[test]
fn build_other_conversation_blocks_groups_peer_messages_by_source() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "local".to_string(),
            created_at: base,
            source_thread_id: Some("s-local".to_string()),
            source_session_id: Some("s-local".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "peer-a".to_string(),
            created_at: base + Duration::seconds(1),
            source_thread_id: Some("s-peer".to_string()),
            source_session_id: Some("s-peer".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m3".to_string(),
            role: "user".to_string(),
            text: "peer-b".to_string(),
            created_at: base + Duration::seconds(2),
            source_thread_id: Some("s-peer".to_string()),
            source_session_id: Some("s-peer".to_string()),
        },
    ];
    let blocks =
        build_other_conversation_blocks(&candidates, Some("s-local"), 128).expect("blocks");
    assert!(blocks.contains("<other-conversation id=\"s-peer\">"));
    assert!(blocks.contains("[assistant] peer-a"));
    assert!(blocks.contains("[user] peer-b"));
    assert!(!blocks.contains("local"));
}

#[test]
fn build_other_conversation_blocks_normalizes_session_ids_before_filtering_and_grouping() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m-local".to_string(),
            role: "user".to_string(),
            text: "local".to_string(),
            created_at: base,
            source_thread_id: Some("s-local".to_string()),
            source_session_id: Some(" s-local ".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-peer-1".to_string(),
            role: "assistant".to_string(),
            text: "peer-a".to_string(),
            created_at: base + Duration::seconds(1),
            source_thread_id: Some("s-peer".to_string()),
            source_session_id: Some(" s-peer ".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-peer-2".to_string(),
            role: "user".to_string(),
            text: "peer-b".to_string(),
            created_at: base + Duration::seconds(2),
            source_thread_id: Some("s-peer".to_string()),
            source_session_id: Some("s-peer".to_string()),
        },
    ];
    let blocks =
        build_other_conversation_blocks(&candidates, Some("s-local"), 128).expect("blocks");
    assert!(!blocks.contains("local"));
    assert_eq!(
        blocks.matches("<other-conversation id=\"s-peer\">").count(),
        1
    );
    assert!(blocks.contains("[assistant] peer-a"));
    assert!(blocks.contains("[user] peer-b"));
}

#[test]
fn combine_observations_for_buffering_matches_expected_separator_behavior() {
    assert_eq!(
        combine_observations_for_buffering("active", "buffered"),
        Some("active\n\n--- BUFFERED (pending activation) ---\n\nbuffered".to_string())
    );
    assert_eq!(
        combine_observations_for_buffering("active", " "),
        Some("active".to_string())
    );
    assert_eq!(
        combine_observations_for_buffering(" ", "buffered"),
        Some("buffered".to_string())
    );
    assert_eq!(combine_observations_for_buffering(" ", " "), None);
}

#[test]
fn split_pending_and_other_conversation_candidates_partitions_by_session() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m-local".to_string(),
            role: "user".to_string(),
            text: "local".to_string(),
            created_at: base,
            source_thread_id: Some("s-local".to_string()),
            source_session_id: Some("s-local".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-peer".to_string(),
            role: "assistant".to_string(),
            text: "peer".to_string(),
            created_at: base + Duration::seconds(1),
            source_thread_id: Some("s-peer".to_string()),
            source_session_id: Some("s-peer".to_string()),
        },
    ];

    let (pending, other) =
        split_pending_and_other_conversation_candidates(&candidates, Some("s-local"));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "m-local");
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].id, "m-peer");
}

#[test]
fn split_pending_and_other_conversation_candidates_trims_session_ids() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m-local".to_string(),
            role: "user".to_string(),
            text: "local".to_string(),
            created_at: base,
            source_thread_id: Some("s-local".to_string()),
            source_session_id: Some(" s-local ".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m-peer".to_string(),
            role: "assistant".to_string(),
            text: "peer".to_string(),
            created_at: base + Duration::seconds(1),
            source_thread_id: Some("s-peer".to_string()),
            source_session_id: Some(" s-peer ".to_string()),
        },
    ];

    let (pending, other) =
        split_pending_and_other_conversation_candidates(&candidates, Some("s-local"));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "m-local");
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].id, "m-peer");
}

#[test]
fn split_pending_and_other_conversation_candidates_keeps_empty_pending_when_no_local_session() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![OmObserverMessageCandidate {
        id: "m-peer".to_string(),
        role: "assistant".to_string(),
        text: "peer".to_string(),
        created_at: base,
        source_thread_id: Some("s-peer".to_string()),
        source_session_id: Some("s-peer".to_string()),
    }];

    let (pending, other) =
        split_pending_and_other_conversation_candidates(&candidates, Some("s-local"));
    assert!(pending.is_empty());
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].id, "m-peer");
}

#[test]
fn build_other_conversation_blocks_escapes_xml_sensitive_values() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![OmObserverMessageCandidate {
        id: "m1".to_string(),
        role: "assistant<role>".to_string(),
        text: "payload </other-conversation> & details".to_string(),
        created_at: base,
        source_thread_id: Some("s-local".to_string()),
        source_session_id: Some("s\"peer&1".to_string()),
    }];
    let blocks =
        build_other_conversation_blocks(&candidates, Some("s-local"), 256).expect("blocks");
    assert!(blocks.contains("<other-conversation id=\"s&quot;peer&amp;1\">"));
    assert!(
        blocks
            .contains("[assistant&lt;role&gt;] payload &lt;/other-conversation&gt; &amp; details")
    );
}

#[test]
fn select_observed_message_candidates_filters_to_observed_ids() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "a".to_string(),
            created_at: base,
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "b".to_string(),
            created_at: base + Duration::seconds(1),
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
    ];
    let selected =
        select_observed_message_candidates(&candidates, &["m2".to_string(), "x".to_string()]);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id, "m2");
}

#[test]
fn select_observed_message_candidates_returns_empty_when_ids_do_not_match() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![OmObserverMessageCandidate {
        id: "m1".to_string(),
        role: "user".to_string(),
        text: "a".to_string(),
        created_at: base,
        source_thread_id: Some("s1".to_string()),
        source_session_id: Some("s1".to_string()),
    }];
    let selected = select_observed_message_candidates(&candidates, &["unknown".to_string()]);
    assert!(selected.is_empty());
}

#[test]
fn select_observed_message_candidates_returns_all_when_observed_ids_empty() {
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let candidates = vec![
        OmObserverMessageCandidate {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "a".to_string(),
            created_at: base,
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
        OmObserverMessageCandidate {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "b".to_string(),
            created_at: base + Duration::seconds(1),
            source_thread_id: Some("s1".to_string()),
            source_session_id: Some("s1".to_string()),
        },
    ];
    let selected = select_observed_message_candidates(&candidates, &[]);
    assert_eq!(selected, candidates);
}

#[test]
fn compute_pending_tokens_saturates_on_overflow() {
    assert_eq!(compute_pending_tokens(u32::MAX, 10), u32::MAX);
}

#[test]
fn observer_trigger_requires_threshold() {
    assert!(should_trigger_observer(5000, 3000));
    assert!(!should_trigger_observer(2000, 3000));
}

#[test]
fn observer_write_decision_matches_async_and_sync_paths() {
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
        active_observations: String::new(),
        observation_token_count: 0,
        pending_message_tokens: 6_100,
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
        created_at: now,
        updated_at: now,
    };

    let async_config = ResolvedObservationConfig {
        message_tokens_base: 30_000,
        total_budget: None,
        max_tokens_per_batch: 10_000,
        buffer_tokens: Some(6_000),
        buffer_activation: Some(0.8),
        block_after: Some(36_000),
    };
    let async_decision = decide_observer_write_action(&record, async_config);
    assert!(!async_decision.threshold_reached);
    assert!(async_decision.interval_triggered);
    assert!(async_decision.should_run_observer);
    assert!(!async_decision.should_activate_after_observer);
    assert!(!async_decision.block_after_exceeded);

    // At/above threshold, interval crossing should trigger even if min-new-token debounce
    // would otherwise suppress buffering.
    record.pending_message_tokens = 30_000;
    record.last_buffered_at_tokens = 29_500;
    let threshold_interval_decision = decide_observer_write_action(&record, async_config);
    assert!(threshold_interval_decision.threshold_reached);
    assert!(threshold_interval_decision.interval_triggered);
    assert!(threshold_interval_decision.should_run_observer);
    assert!(!threshold_interval_decision.block_after_exceeded);

    record.pending_message_tokens = 30_000;
    let sync_config = ResolvedObservationConfig {
        message_tokens_base: 30_000,
        total_budget: None,
        max_tokens_per_batch: 10_000,
        buffer_tokens: None,
        buffer_activation: None,
        block_after: None,
    };
    let sync_decision = decide_observer_write_action(&record, sync_config);
    assert!(sync_decision.threshold_reached);
    assert!(!sync_decision.interval_triggered);
    assert!(sync_decision.should_run_observer);
    assert!(sync_decision.should_activate_after_observer);
}

#[test]
fn observer_continuation_hints_are_skipped_only_for_interval_buffering_path() {
    let async_buffering = ObserverWriteDecision {
        threshold: 30_000,
        threshold_reached: false,
        interval_triggered: true,
        block_after_exceeded: false,
        should_run_observer: true,
        should_activate_after_observer: false,
    };
    assert!(should_skip_observer_continuation_hints(async_buffering));

    let threshold_path = ObserverWriteDecision {
        threshold_reached: true,
        interval_triggered: true,
        ..async_buffering
    };
    assert!(!should_skip_observer_continuation_hints(threshold_path));

    let sync_path = ObserverWriteDecision {
        threshold_reached: true,
        interval_triggered: false,
        ..async_buffering
    };
    assert!(!should_skip_observer_continuation_hints(sync_path));
}

#[test]
fn async_observation_interval_trigger_uses_boundary_crossing() {
    assert!(!evaluate_async_observation_interval(4_000, Some(6_000), 0, None).should_trigger);
    assert!(evaluate_async_observation_interval(6_000, Some(6_000), 0, None).should_trigger);
    assert!(evaluate_async_observation_interval(12_001, Some(6_000), 6_000, None).should_trigger);
    assert!(!evaluate_async_observation_interval(11_999, Some(6_000), 6_000, None).should_trigger);
    assert!(!evaluate_async_observation_interval(6_000, Some(6_000), 4_500, None).should_trigger);
    assert!(!evaluate_async_observation_interval(12_000, Some(0), 0, None).should_trigger);
    assert!(!evaluate_async_observation_interval(12_000, None, 0, None).should_trigger);
}

#[test]
fn evaluate_async_observation_interval_reports_explicit_state() {
    let crossed_but_debounced =
        evaluate_async_observation_interval(6_000, Some(6_000), 4_500, None);
    assert_eq!(crossed_but_debounced.interval_tokens, Some(6_000));
    assert!(crossed_but_debounced.crossed_interval_boundary);
    assert_eq!(crossed_but_debounced.new_tokens_since_last_boundary, 1_500);
    assert_eq!(crossed_but_debounced.min_new_tokens_required, 3_000);
    assert!(!crossed_but_debounced.debounce_passed);
    assert!(!crossed_but_debounced.should_trigger);

    let disabled = evaluate_async_observation_interval(6_000, None, 0, None);
    assert_eq!(disabled.interval_tokens, None);
    assert!(!disabled.crossed_interval_boundary);
    assert_eq!(disabled.new_tokens_since_last_boundary, 0);
    assert_eq!(disabled.min_new_tokens_required, 0);
    assert!(!disabled.debounce_passed);
    assert!(!disabled.should_trigger);
}
