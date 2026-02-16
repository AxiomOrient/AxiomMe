mod activation;
mod helpers;
mod observer;
mod reflection;
mod scope;
mod types;

pub use activation::{
    activate_buffered_observations, calculate_dynamic_threshold, merge_activated_observations,
    normalize_observation_buffer_boundary, select_activation_boundary,
};
pub use observer::{
    build_other_conversation_blocks, combine_observations_for_buffering, compute_pending_tokens,
    decide_observer_write_action, evaluate_async_observation_interval,
    filter_observer_candidates_by_last_observed_at, select_observed_message_candidates,
    select_observer_message_candidates, should_skip_observer_continuation_hints,
    should_trigger_observer, split_pending_and_other_conversation_candidates,
    synthesize_observer_observations,
};
pub use reflection::{
    build_reflection_draft, decide_reflection_enqueue, merge_buffered_reflection,
    plan_buffered_reflection_slice, reflector_compression_guidance, select_reflection_action,
    should_trigger_reflector, validate_reflection_compression,
};
pub use scope::build_scope_key;
pub use types::{
    ActivationBoundary, ActivationResult, AsyncObservationIntervalState,
    BUFFERED_OBSERVATIONS_SEPARATOR, BufferedReflectionSlicePlan, ObserverWriteDecision,
    OmObserverMessageCandidate, OmTransformError, ReflectionAction, ReflectionDraft,
    ReflectionEnqueueDecision,
};

#[cfg(test)]
mod tests;
