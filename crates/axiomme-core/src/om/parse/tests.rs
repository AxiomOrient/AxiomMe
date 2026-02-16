use super::*;

#[test]
fn observations_sections_join_multiple_blocks_and_ignore_inline_mentions() {
    let text = concat!(
        "note: user wrote <observations>inline</observations> in plain text\n",
        "<observations>\n",
        "line-a\n",
        "</observations>\n",
        "noise\n",
        "  <observations>\n",
        "line-b\n",
        "  </observations>\n",
    );
    let parsed = extract_observations_sections(text, OmParseMode::Lenient).expect("sections");
    assert_eq!(parsed, "line-a\nline-b");
}

#[test]
fn tag_content_requires_line_anchored_tags() {
    let text = concat!(
        "keep literal <current-task>not-a-tag</current-task> mention\n",
        "<current-task>\n",
        "Primary: implement parity\n",
        "</current-task>\n",
    );
    let parsed = extract_tag_content(text, "current-task", OmParseMode::Lenient).expect("tag");
    assert_eq!(parsed, "Primary: implement parity");
}

#[test]
fn tag_content_prefers_last_non_empty_section_when_multiple_exist() {
    let text = concat!(
        "<current-task>\n",
        "old task\n",
        "</current-task>\n",
        "<current-task>\n",
        "new task\n",
        "</current-task>\n",
    );
    let parsed = extract_tag_content(text, "current-task", OmParseMode::Lenient).expect("tag");
    assert_eq!(parsed, "new task");
}

#[test]
fn parsing_is_ascii_case_insensitive_for_tag_tokens() {
    let text = "<OBSERVATIONS>\nline-a\n</OBSERVATIONS>";
    let parsed = extract_observations_sections(text, OmParseMode::Lenient).expect("sections");
    assert_eq!(parsed, "line-a");
}

#[test]
fn list_item_fallback_preserves_nested_indentation() {
    let text = concat!(
        "heading\n",
        "* one\n",
        "  * nested\n",
        "    1. numbered\n",
        "plain paragraph\n",
    );
    let parsed = extract_list_items_only(text);
    assert_eq!(parsed, "* one\n  * nested\n    1. numbered");
}

#[test]
fn parse_memory_section_xml_reads_observations_and_tags() {
    let text = concat!(
        "<observations>\n",
        "Date: Dec 4, 2025\n",
        "* 🔴 (14:30) User prefers direct answers\n",
        "</observations>\n",
        "<current-task>\n",
        "Primary: feature X\n",
        "</current-task>\n",
        "<suggested-response>\n",
        "Ask for confirmation\n",
        "</suggested-response>\n",
    );
    let parsed = parse_memory_section_xml(text, OmParseMode::Lenient);
    assert!(parsed.observations.contains("User prefers direct answers"));
    assert_eq!(parsed.current_task.as_deref(), Some("Primary: feature X"));
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Ask for confirmation")
    );
}

#[test]
fn parse_memory_section_xml_parses_single_line_task_and_response_tags() {
    let text = concat!(
        "<observations>\n",
        "* 🔴 (14:30) User prefers direct answers\n",
        "</observations>\n",
        "<current-task>Primary: feature X</current-task>\n",
        "<suggested-response>Ask for confirmation</suggested-response>\n",
    );
    let parsed = parse_memory_section_xml(text, OmParseMode::Lenient);
    assert_eq!(parsed.current_task.as_deref(), Some("Primary: feature X"));
    assert_eq!(
        parsed.suggested_response.as_deref(),
        Some("Ask for confirmation")
    );
}

#[test]
fn parse_memory_section_xml_prefers_last_task_and_response_tags() {
    let text = concat!(
        "<observations>\n",
        "* first\n",
        "</observations>\n",
        "<current-task>old task</current-task>\n",
        "<suggested-response>old response</suggested-response>\n",
        "<current-task>new task</current-task>\n",
        "<suggested-response>new response</suggested-response>\n",
    );
    let parsed = parse_memory_section_xml(text, OmParseMode::Lenient);
    assert_eq!(parsed.current_task.as_deref(), Some("new task"));
    assert_eq!(parsed.suggested_response.as_deref(), Some("new response"));
}

#[test]
fn parse_memory_section_xml_falls_back_to_list_items() {
    let text = "header\n* one\n  * two\nplain";
    let parsed = parse_memory_section_xml(text, OmParseMode::Lenient);
    assert_eq!(parsed.observations, "* one\n  * two");
    assert_eq!(parsed.current_task, None);
    assert_eq!(parsed.suggested_response, None);
}

#[test]
fn parse_multi_thread_observer_output_reads_thread_sections() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"thread-1\">\n",
        "Date: Dec 4, 2025\n",
        "* 🔴 (14:30) User prefers direct answers\n",
        "<current-task>Working on feature X</current-task>\n",
        "<suggested-response>Continue implementation</suggested-response>\n",
        "</thread>\n",
        "<thread id=\"thread-2\">\n",
        "* 🟡 (09:10) Debugging API timeout\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].thread_id, "thread-1");
    assert!(
        parsed[0]
            .observations
            .contains("User prefers direct answers")
    );
    assert_eq!(
        parsed[0].current_task.as_deref(),
        Some("Working on feature X")
    );
    assert_eq!(
        parsed[0].suggested_response.as_deref(),
        Some("Continue implementation")
    );
    assert_eq!(parsed[1].thread_id, "thread-2");
    assert!(parsed[1].observations.contains("Debugging API timeout"));
}

#[test]
fn parse_multi_thread_observer_output_ignores_invalid_thread_id() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"\">\n",
        "* 🟡 invalid\n",
        "</thread>\n",
        "<thread id=\"thread-2\">\n",
        "* 🟡 valid\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].thread_id, "thread-2");
    assert!(parsed[0].observations.contains("valid"));
}

#[test]
fn parse_multi_thread_observer_output_recovers_from_unclosed_leading_block() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"broken\">\n",
        "* 🟡 missing close before next thread\n",
        "<thread id='thread-2'>\n",
        "* 🟡 valid\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].thread_id, "thread-2");
    assert!(parsed[0].observations.contains("valid"));
}

#[test]
fn observations_sections_recover_from_overlapping_sections() {
    let text = concat!(
        "<observations>\n",
        "broken start\n",
        "<observations>\n",
        "good section\n",
        "</observations>\n",
    );
    let parsed = extract_observations_sections(text, OmParseMode::Lenient).expect("sections");
    assert_eq!(parsed, "good section");
}

#[test]
fn strict_mode_does_not_recover_overlapping_observation_sections() {
    let text = concat!(
        "<observations>\n",
        "broken start\n",
        "<observations>\n",
        "good section\n",
        "</observations>\n",
    );
    let parsed = extract_observations_sections(text, OmParseMode::Strict);
    assert_eq!(parsed, None);
}

#[test]
fn strict_mode_does_not_recover_unclosed_leading_thread_block() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"broken\">\n",
        "* 🟡 missing close before next thread\n",
        "<thread id='thread-2'>\n",
        "* 🟡 valid\n",
        "</thread>\n",
        "</observations>\n",
    );
    let strict = parse_multi_thread_observer_output(text, OmParseMode::Strict);
    let lenient = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert!(strict.is_empty());
    assert_eq!(lenient.len(), 1);
    assert_eq!(lenient[0].thread_id, "thread-2");
}

#[test]
fn aggregate_multi_thread_sections_prefers_primary_thread_metadata() {
    let aggregate = aggregate_multi_thread_observer_sections(
        &[
            OmMultiThreadObserverSection {
                thread_id: "thread-1".to_string(),
                observations: "* 🟡 one".to_string(),
                current_task: Some("task-1".to_string()),
                suggested_response: None,
            },
            OmMultiThreadObserverSection {
                thread_id: "thread-2".to_string(),
                observations: "* 🟡 two".to_string(),
                current_task: Some("task-2".to_string()),
                suggested_response: Some("reply-2".to_string()),
            },
        ],
        Some("thread-2"),
    );
    assert!(aggregate.observations.contains("<thread id=\"thread-1\">"));
    assert!(aggregate.observations.contains("<thread id=\"thread-2\">"));
    assert_eq!(aggregate.current_task.as_deref(), Some("task-2"));
    assert_eq!(aggregate.suggested_response.as_deref(), Some("reply-2"));
}

#[test]
fn aggregate_multi_thread_sections_escapes_xml_sensitive_values() {
    let aggregate = aggregate_multi_thread_observer_sections(
        &[OmMultiThreadObserverSection {
            thread_id: "thread\"a&1".to_string(),
            observations: "* literal </thread> & data".to_string(),
            current_task: None,
            suggested_response: None,
        }],
        None,
    );
    assert!(
        aggregate
            .observations
            .contains("<thread id=\"thread&quot;a&amp;1\">")
    );
    assert!(
        aggregate
            .observations
            .contains("* literal &lt;/thread&gt; &amp; data")
    );
}

#[test]
fn thread_section_removal_ignores_inline_tag_literals() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"thread-1\">\n",
        "literal <current-task>inline</current-task> mention\n",
        "<current-task>\n",
        "anchored task\n",
        "</current-task>\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].current_task.as_deref(), Some("anchored task"));
    assert!(
        parsed[0]
            .observations
            .contains("literal <current-task>inline</current-task> mention")
    );
}

#[test]
fn thread_section_removal_prefers_line_anchored_closing_tag_over_inline_token() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"thread-1\">\n",
        "* 🟡 (10:00) observation line\n",
        "<current-task>\n",
        "Investigate literal token </current-task> in logs\n",
        "Actual anchored task line\n",
        "</current-task>\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert_eq!(parsed.len(), 1);
    assert_eq!(
        parsed[0].current_task.as_deref(),
        Some("Investigate literal token </current-task> in logs\nActual anchored task line")
    );
}

#[test]
fn thread_section_removal_prefers_latest_metadata_and_strips_all_metadata_blocks() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"thread-1\">\n",
        "Date: Dec 4, 2025\n",
        "* one\n",
        "<current-task>old task</current-task>\n",
        "<suggested-response>old response</suggested-response>\n",
        "<current-task>new task</current-task>\n",
        "<suggested-response>new response</suggested-response>\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].current_task.as_deref(), Some("new task"));
    assert_eq!(
        parsed[0].suggested_response.as_deref(),
        Some("new response")
    );
    assert!(!parsed[0].observations.contains("<current-task>"));
    assert!(!parsed[0].observations.contains("<suggested-response>"));
}

#[test]
fn thread_block_extraction_prefers_line_anchored_close_over_inline_token() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"thread-1\">\n",
        "* 🟡 literal </thread> token in log line\n",
        "* 🟡 keep parsing until anchored close\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].thread_id, "thread-1");
    assert!(
        parsed[0]
            .observations
            .contains("* 🟡 literal </thread> token in log line")
    );
    assert!(
        parsed[0]
            .observations
            .contains("* 🟡 keep parsing until anchored close")
    );
}

#[test]
fn parse_memory_section_accuracy_first_falls_back_to_lenient_when_strict_empty() {
    let text = concat!(
        "<observations>\n",
        "broken start\n",
        "<observations>\n",
        "good section\n",
        "</observations>\n",
    );
    let parsed = parse_memory_section_xml_accuracy_first(text);
    assert_eq!(parsed.observations, "good section");
}

#[test]
fn parse_memory_section_accuracy_first_prefers_strict_when_usable() {
    let text = concat!(
        "<observations>\n",
        "strict section\n",
        "</observations>\n",
        "<observations>\n",
        "broken start\n",
        "<observations>\n",
        "lenient-only recovery\n",
        "</observations>\n",
    );
    let strict = parse_memory_section_xml(text, OmParseMode::Strict);
    let lenient = parse_memory_section_xml(text, OmParseMode::Lenient);
    let accuracy = parse_memory_section_xml_accuracy_first(text);
    assert_eq!(strict.observations, "strict section");
    assert_eq!(
        lenient.observations,
        "strict section\nlenient-only recovery"
    );
    assert_eq!(accuracy.observations, strict.observations);
}

#[test]
fn parse_memory_section_accuracy_first_prefers_lenient_when_only_lenient_has_observations() {
    let text = concat!(
        "<current-task>strict task</current-task>\n",
        "<observations>\n",
        "broken start\n",
        "<observations>\n",
        "lenient section\n",
        "</observations>\n",
    );
    let strict = parse_memory_section_xml(text, OmParseMode::Strict);
    let lenient = parse_memory_section_xml(text, OmParseMode::Lenient);
    let accuracy = parse_memory_section_xml_accuracy_first(text);
    assert_eq!(strict.current_task.as_deref(), Some("strict task"));
    assert!(strict.observations.is_empty());
    assert_eq!(lenient.observations, "lenient section");
    assert_eq!(accuracy.observations, "lenient section");
}

#[test]
fn parse_multi_thread_accuracy_first_falls_back_when_strict_has_no_observations() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"broken\">\n",
        "missing close before next thread\n",
        "<thread id='thread-2'>\n",
        "* 🟡 valid\n",
        "</thread>\n",
        "</observations>\n",
    );
    let parsed = parse_multi_thread_observer_output_accuracy_first(text);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].thread_id, "thread-2");
}

#[test]
fn parse_multi_thread_accuracy_first_prefers_strict_when_observations_exist() {
    let text = concat!(
        "<observations>\n",
        "<thread id=\"thread-1\">\n",
        "* 🟡 strict section\n",
        "</thread>\n",
        "<thread id=\"broken\">\n",
        "missing close before next thread\n",
        "<thread id='thread-2'>\n",
        "* 🟡 lenient-only recovery\n",
        "</thread>\n",
        "</thread>\n",
        "</observations>\n",
    );
    let strict = parse_multi_thread_observer_output(text, OmParseMode::Strict);
    let lenient = parse_multi_thread_observer_output(text, OmParseMode::Lenient);
    let accuracy = parse_multi_thread_observer_output_accuracy_first(text);
    assert_eq!(strict.len(), 1);
    assert_eq!(lenient.len(), 2);
    assert_eq!(accuracy, strict);
}
