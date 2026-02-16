use crate::om::config::ResolvedObservationConfig;
use crate::om::model::OmRecord;

use super::super::types::{AsyncObservationIntervalState, ObserverWriteDecision};

#[must_use]
pub const fn should_skip_observer_continuation_hints(decision: ObserverWriteDecision) -> bool {
    decision.interval_triggered && !decision.threshold_reached
}

#[must_use]
pub const fn compute_pending_tokens(context_window_tokens: u32, cross_thread_tokens: u32) -> u32 {
    context_window_tokens.saturating_add(cross_thread_tokens)
}

#[must_use]
pub const fn should_trigger_observer(total_pending: u32, threshold: u32) -> bool {
    total_pending >= threshold
}

#[must_use]
pub fn evaluate_async_observation_interval(
    current_tokens: u32,
    buffer_tokens: Option<u32>,
    last_buffered_at_tokens: u32,
    min_new_tokens: Option<u32>,
) -> AsyncObservationIntervalState {
    let Some(interval_tokens) = buffer_tokens.filter(|value| *value > 0) else {
        return AsyncObservationIntervalState {
            interval_tokens: None,
            crossed_interval_boundary: false,
            new_tokens_since_last_boundary: 0,
            min_new_tokens_required: 0,
            debounce_passed: false,
            should_trigger: false,
        };
    };

    let current_interval = current_tokens / interval_tokens;
    let last_interval = last_buffered_at_tokens / interval_tokens;
    let crossed_interval_boundary = current_interval > last_interval;
    let new_tokens_since_last_boundary = current_tokens.saturating_sub(last_buffered_at_tokens);
    let min_new_tokens_required = min_new_tokens.unwrap_or_else(|| (interval_tokens / 2).max(1));
    let debounce_passed = new_tokens_since_last_boundary >= min_new_tokens_required;
    let should_trigger = crossed_interval_boundary && debounce_passed;

    AsyncObservationIntervalState {
        interval_tokens: Some(interval_tokens),
        crossed_interval_boundary,
        new_tokens_since_last_boundary,
        min_new_tokens_required,
        debounce_passed,
        should_trigger,
    }
}

#[must_use]
pub fn decide_observer_write_action(
    record: &OmRecord,
    observation_config: ResolvedObservationConfig,
) -> ObserverWriteDecision {
    let threshold = observation_config.dynamic_threshold(record.observation_token_count);
    let threshold_reached = should_trigger_observer(record.pending_message_tokens, threshold);
    let async_min_new_tokens = observation_config
        .buffer_tokens
        .map(|interval| (interval / 2).max(1));
    let interval_state = evaluate_async_observation_interval(
        record.pending_message_tokens,
        observation_config.buffer_tokens,
        record.last_buffered_at_tokens,
        async_min_new_tokens,
    );
    let interval_triggered = if threshold_reached {
        // Once the threshold is reached, defer less to debounce so buffering can catch up.
        interval_state.crossed_interval_boundary
    } else {
        interval_state.should_trigger
    };
    let block_after_exceeded = observation_config
        .block_after
        .is_some_and(|value| record.pending_message_tokens >= value);
    let async_observation_enabled = observation_config.buffer_tokens.is_some();
    let should_run_observer = if async_observation_enabled {
        interval_triggered || (threshold_reached && block_after_exceeded)
    } else {
        threshold_reached
    };
    let should_activate_after_observer = if async_observation_enabled {
        threshold_reached && block_after_exceeded
    } else {
        threshold_reached
    };
    ObserverWriteDecision {
        threshold,
        threshold_reached,
        interval_triggered,
        block_after_exceeded,
        should_run_observer,
        should_activate_after_observer,
    }
}
