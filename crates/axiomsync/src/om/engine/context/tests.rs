use super::*;

#[test]
fn bounded_observation_hint_uses_recent_lines_and_prefix() {
    let hint = build_bounded_observation_hint("a\nb\nc", 2, 32).expect("hint");
    assert_eq!(hint, "om: b c");
}

#[test]
fn bounded_observation_hint_returns_none_on_zero_limits_or_empty_input() {
    assert!(build_bounded_observation_hint("a", 0, 10).is_none());
    assert!(build_bounded_observation_hint("a", 2, 0).is_none());
    assert!(build_bounded_observation_hint(" \n ", 2, 10).is_none());
}

#[test]
fn bounded_observation_hint_compacts_whitespace_and_obeys_char_budget() {
    let hint = build_bounded_observation_hint("a   b\n\n  c   d", 2, 6).expect("hint");
    assert_eq!(hint, "om: a b c");
}

#[test]
fn bounded_observation_hint_preserves_high_priority_signal() {
    let hint = build_bounded_observation_hint("noise-1\n🔴 critical-fact\nnoise-2\nnoise-3", 2, 64)
        .expect("hint");
    assert_eq!(hint, "om: 🔴 critical-fact noise-3");
}

#[test]
fn bounded_observation_hint_reserves_continuation_line() {
    let hint = build_bounded_observation_hint(
        "context-a\ncurrent-task: finalize om-v2\ncontext-b\ncontext-c",
        2,
        96,
    )
    .expect("hint");
    assert_eq!(hint, "om: current-task: finalize om-v2 context-c");
}

#[test]
fn bounded_observation_hint_treats_priority_high_prefix_as_high_priority() {
    let hint =
        build_bounded_observation_hint("priority:high keep-this\nnoise-tail", 1, 96).expect("hint");
    assert_eq!(hint, "om: priority:high keep-this");
}

#[test]
fn bounded_observation_hint_deduplicates_identical_lines() {
    let hint = build_bounded_observation_hint("dup\ndup\ntail", 3, 96).expect("hint");
    assert_eq!(hint, "om: dup tail");
}

#[test]
fn bounded_observation_hint_treats_embedded_priority_high_as_high_priority() {
    let hint = build_bounded_observation_hint("noise\nnote priority:high keep\ntail", 1, 96)
        .expect("hint");
    assert_eq!(hint, "om: note priority:high keep");
}

#[test]
fn bounded_observation_hint_counts_multibyte_char_budget_by_chars() {
    let hint = build_bounded_observation_hint("한글테스트", 1, 2).expect("hint");
    assert_eq!(hint, "om: 한글");
}
