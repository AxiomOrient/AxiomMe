use super::*;
use crate::om::OmPendingMessage;

#[test]
fn observer_prompt_includes_required_sections() {
    let history = format_observer_messages_for_prompt(&[
        OmPendingMessage {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "hello".to_string(),
            created_at_rfc3339: None,
        },
        OmPendingMessage {
            id: "m2".to_string(),
            role: "assistant".to_string(),
            text: "hi".to_string(),
            created_at_rfc3339: None,
        },
    ]);
    let prompt = build_observer_user_prompt(OmObserverPromptInput {
        request_json: None,
        existing_observations: Some("obs"),
        message_history: &history,
        other_conversation_context: Some("none"),
        skip_continuation_hints: false,
    });
    assert!(prompt.contains("## Previous Observations"));
    assert!(prompt.contains("## New Message History to Observe"));
    assert!(prompt.contains("**User"));
}

#[test]
fn multi_thread_prompt_formats_thread_blocks() {
    let prompt = build_multi_thread_observer_user_prompt(
        Some("existing"),
        &[
            OmObserverThreadMessages {
                thread_id: "thread-a".to_string(),
                message_history: "**User:** hello".to_string(),
            },
            OmObserverThreadMessages {
                thread_id: "thread-b".to_string(),
                message_history: "**Assistant:** hi".to_string(),
            },
        ],
        false,
    );
    assert!(prompt.contains("<thread id=\"thread-a\">"));
    assert!(prompt.contains("<thread id=\"thread-b\">"));
    assert!(prompt.contains("Example output format"));
}

#[test]
fn multi_thread_prompt_escapes_xml_sensitive_thread_values() {
    let prompt = build_multi_thread_observer_user_prompt(
        None,
        &[OmObserverThreadMessages {
            thread_id: "thread\"a&1".to_string(),
            message_history: "**User:** includes </thread> literal".to_string(),
        }],
        false,
    );
    assert!(prompt.contains("<thread id=\"thread&quot;a&amp;1\">"));
    assert!(prompt.contains("**User:** includes &lt;/thread&gt; literal"));
}

#[test]
fn multi_thread_prompt_can_skip_continuation_hints() {
    let prompt = build_multi_thread_observer_user_prompt(
        None,
        &[OmObserverThreadMessages {
            thread_id: "thread-a".to_string(),
            message_history: "**User:** hello".to_string(),
        }],
        true,
    );
    assert!(prompt.contains("Do NOT include <current-task> or <suggested-response>"));
}

#[test]
fn reflector_prompt_applies_guidance_and_skip_rule() {
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

#[test]
fn observer_system_prompt_exposes_output_contract_sections() {
    let system = build_observer_system_prompt();
    assert!(system.contains("=== OUTPUT FORMAT ==="));
    assert!(system.contains("<observations>"));
    assert!(system.contains("<current-task>"));
    assert!(system.contains("<suggested-response>"));
}

#[test]
fn multi_thread_observer_system_prompt_mentions_thread_blocks() {
    let system = build_multi_thread_observer_system_prompt();
    assert!(system.contains("=== MULTI-THREAD INPUT ==="));
    assert!(system.contains("<thread id=\"thread-1\">"));
}

#[test]
fn observer_system_prompt_uses_state_change_instructions() {
    let system = build_observer_system_prompt();
    assert!(system.contains("STATE CHANGES AND UPDATES:"));
}

#[test]
fn observer_system_prompt_is_deterministic() {
    let first = build_observer_system_prompt();
    let second = build_observer_system_prompt();
    assert_eq!(first, second);
}

#[test]
fn reflector_system_prompt_uses_base_output_contract() {
    let system = build_reflector_system_prompt();
    assert!(system.contains("Group related observations by date and list each with 24-hour time."));
    assert!(system.contains("<current-task>"));
}

#[test]
fn format_observer_messages_for_prompt_normalizes_role_and_formats_timestamp() {
    let formatted = format_observer_messages_for_prompt(&[OmPendingMessage {
        id: "m1".to_string(),
        role: " user ".to_string(),
        text: "hello".to_string(),
        created_at_rfc3339: Some("2026-02-14T08:30:00Z".to_string()),
    }]);
    assert!(formatted.contains("**User (Feb 14, 2026, 8:30 AM):**"));
    assert!(formatted.contains("hello"));
}

#[test]
fn format_observer_messages_for_prompt_uses_unknown_role_and_skips_invalid_timestamp() {
    let formatted = format_observer_messages_for_prompt(&[OmPendingMessage {
        id: "m1".to_string(),
        role: "   ".to_string(),
        text: "hello".to_string(),
        created_at_rfc3339: Some("not-a-timestamp".to_string()),
    }]);
    assert_eq!(formatted, "**Unknown:**\nhello");
}
