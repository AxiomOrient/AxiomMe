mod addon;
mod config;
mod context;
mod failure;
mod inference;
mod model;
mod parse;
mod pipeline;
mod prompt;
mod rollout;
mod transform;
mod xml;

pub use addon::{
    OmApplyAddon, OmCommand, OmObserverAddon, OmReflectionCommand, OmReflectionCommandType,
    OmReflectorAddon, reflection_command_from_action,
};
pub use config::{
    BufferTokensInput, DEFAULT_BLOCK_AFTER_MULTIPLIER, DEFAULT_OBSERVER_BUFFER_ACTIVATION,
    DEFAULT_OBSERVER_BUFFER_TOKENS_RATIO, DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH,
    DEFAULT_OBSERVER_MESSAGE_TOKENS, DEFAULT_REFLECTOR_BUFFER_ACTIVATION,
    DEFAULT_REFLECTOR_OBSERVATION_TOKENS, ObservationConfigInput, OmConfigError, OmConfigInput,
    ReflectionConfigInput, ResolvedObservationConfig, ResolvedOmConfig, ResolvedReflectionConfig,
    resolve_om_config,
};
pub use context::build_bounded_observation_hint;
pub(crate) use failure::{om_observer_error, om_reflector_error, om_status_kind};
pub use inference::{
    OmInferenceModelConfig, OmInferenceUsage, OmObserverRequest, OmObserverResponse,
    OmPendingMessage, OmReflectorRequest, OmReflectorResponse,
};
pub use model::{OmObservationChunk, OmOriginType, OmRecord, OmScope};
pub use parse::{
    OmMemorySection, OmMultiThreadObserverAggregate, OmMultiThreadObserverSection, OmParseMode,
    aggregate_multi_thread_observer_sections, extract_list_items_only, parse_memory_section_xml,
    parse_memory_section_xml_accuracy_first, parse_multi_thread_observer_output,
    parse_multi_thread_observer_output_accuracy_first,
};
pub use pipeline::{
    ProcessInputStepOptions, ProcessInputStepPlan, ProcessOutputResultPlan,
    plan_process_input_step, plan_process_output_result,
};
pub use prompt::{
    OmObserverPromptInput, OmObserverThreadMessages, OmReflectorPromptInput,
    build_multi_thread_observer_system_prompt, build_multi_thread_observer_user_prompt,
    build_observer_system_prompt, build_observer_user_prompt, build_reflector_system_prompt,
    build_reflector_user_prompt, format_multi_thread_observer_messages_for_prompt,
    format_observer_messages_for_prompt,
};
pub(crate) use rollout::{resolve_observer_model_enabled, resolve_reflector_model_enabled};
pub use transform::{
    ActivationBoundary, ActivationResult, AsyncObservationIntervalState,
    BUFFERED_OBSERVATIONS_SEPARATOR, BufferedReflectionSlicePlan, ObserverWriteDecision,
    OmObserverMessageCandidate, OmTransformError, ReflectionAction, ReflectionDraft,
    ReflectionEnqueueDecision, activate_buffered_observations, build_other_conversation_blocks,
    build_reflection_draft, build_scope_key, calculate_dynamic_threshold,
    combine_observations_for_buffering, compute_pending_tokens, decide_observer_write_action,
    decide_reflection_enqueue, evaluate_async_observation_interval,
    filter_observer_candidates_by_last_observed_at, merge_activated_observations,
    merge_buffered_reflection, normalize_observation_buffer_boundary,
    plan_buffered_reflection_slice, reflector_compression_guidance, select_activation_boundary,
    select_observed_message_candidates, select_observer_message_candidates,
    select_reflection_action, should_skip_observer_continuation_hints, should_trigger_observer,
    should_trigger_reflector, split_pending_and_other_conversation_candidates,
    synthesize_observer_observations, validate_reflection_compression,
};
