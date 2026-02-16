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
