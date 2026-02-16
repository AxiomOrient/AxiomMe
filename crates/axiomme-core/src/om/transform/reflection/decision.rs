use crate::om::addon::reflection_command_from_action;
use crate::om::config::ResolvedReflectionConfig;
use crate::om::model::OmRecord;

use super::super::types::{ReflectionAction, ReflectionEnqueueDecision};
#[must_use]
pub const fn should_trigger_reflector(observation_tokens: u32, threshold: u32) -> bool {
    observation_tokens > threshold
}

#[must_use]
pub fn select_reflection_action(
    observation_tokens: u32,
    reflection_threshold: u32,
    buffer_activation: Option<f32>,
    block_after: Option<u32>,
    has_buffered_reflection: bool,
    is_buffering_reflection: bool,
    is_reflecting: bool,
) -> ReflectionAction {
    if is_reflecting {
        return ReflectionAction::None;
    }

    let reflector_threshold_reached =
        should_trigger_reflector(observation_tokens, reflection_threshold);
    match buffer_activation {
        None => {
            if reflector_threshold_reached {
                ReflectionAction::Reflect
            } else {
                ReflectionAction::None
            }
        }
        Some(activation) => {
            if !(activation > 0.0 && activation <= 1.0) {
                return ReflectionAction::None;
            }
            if reflector_threshold_reached {
                if has_buffered_reflection {
                    return ReflectionAction::Reflect;
                }
                if block_after.is_some_and(|limit| observation_tokens >= limit) {
                    return ReflectionAction::Reflect;
                }
                if is_buffering_reflection {
                    return ReflectionAction::None;
                }
                return ReflectionAction::Buffer;
            }

            if has_buffered_reflection || is_buffering_reflection {
                return ReflectionAction::None;
            }

            let activation_point =
                (f64::from(reflection_threshold) * f64::from(activation)).max(0.0);
            if f64::from(observation_tokens) >= activation_point {
                ReflectionAction::Buffer
            } else {
                ReflectionAction::None
            }
        }
    }
}

#[must_use]
pub fn decide_reflection_enqueue(
    record: &OmRecord,
    reflection_config: ResolvedReflectionConfig,
    requested_at_rfc3339: &str,
) -> ReflectionEnqueueDecision {
    let has_buffered_reflection = record
        .buffered_reflection
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let action = select_reflection_action(
        record.observation_token_count,
        reflection_config.observation_tokens,
        reflection_config.buffer_activation,
        reflection_config.block_after,
        has_buffered_reflection,
        record.is_buffering_reflection,
        record.is_reflecting,
    );
    let command = reflection_command_from_action(
        action,
        &record.scope_key,
        record.generation_count,
        requested_at_rfc3339,
    );
    let (next_is_reflecting, next_is_buffering_reflection) = match action {
        ReflectionAction::None => (record.is_reflecting, record.is_buffering_reflection),
        ReflectionAction::Buffer => (record.is_reflecting, true),
        ReflectionAction::Reflect => (true, false),
    };
    ReflectionEnqueueDecision {
        action,
        command,
        should_increment_trigger_count: action != ReflectionAction::None,
        next_is_reflecting,
        next_is_buffering_reflection,
    }
}
