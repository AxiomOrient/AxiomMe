// OM pure contracts live in `episodic`. This module is the explicit runtime boundary:
// re-export pure types/transforms and keep only AxiomMe-specific rollout/error helpers local.
mod failure;
mod rollout;

pub use episodic::{
    ActivationBoundary, ActivationResult, AsyncObservationIntervalState,
    BUFFERED_OBSERVATIONS_SEPARATOR, BufferTokensInput, BufferedReflectionSlicePlan,
    DEFAULT_BLOCK_AFTER_MULTIPLIER, DEFAULT_OBSERVER_BUFFER_ACTIVATION,
    DEFAULT_OBSERVER_BUFFER_TOKENS_RATIO, DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH,
    DEFAULT_OBSERVER_MESSAGE_TOKENS, DEFAULT_REFLECTOR_BUFFER_ACTIVATION,
    DEFAULT_REFLECTOR_OBSERVATION_TOKENS, ObservationConfigInput, ObserverWriteDecision,
    OmApplyAddon, OmCommand, OmConfigError, OmConfigInput, OmInferenceModelConfig,
    OmInferenceUsage, OmMemorySection, OmMultiThreadObserverAggregate,
    OmMultiThreadObserverSection, OmObservationChunk, OmObserverAddon, OmObserverMessageCandidate,
    OmObserverPromptInput, OmObserverRequest, OmObserverResponse, OmObserverThreadMessages,
    OmOriginType, OmParseMode, OmPendingMessage, OmRecord, OmRecordInvariantViolation,
    OmReflectionCommand, OmReflectionCommandType, OmReflectorAddon, OmReflectorPromptInput,
    OmReflectorRequest, OmReflectorResponse, OmScope, OmTransformError, ProcessInputStepOptions,
    ProcessInputStepPlan, ProcessOutputResultPlan, ReflectionAction, ReflectionConfigInput,
    ReflectionDraft, ReflectionEnqueueDecision, ResolvedObservationConfig, ResolvedOmConfig,
    ResolvedReflectionConfig, activate_buffered_observations,
    aggregate_multi_thread_observer_sections, build_bounded_observation_hint,
    build_multi_thread_observer_system_prompt, build_multi_thread_observer_user_prompt,
    build_observer_system_prompt, build_observer_user_prompt, build_other_conversation_blocks,
    build_reflection_draft, build_reflector_system_prompt, build_reflector_user_prompt,
    build_scope_key, calculate_dynamic_threshold, combine_observations_for_buffering,
    compute_pending_tokens, decide_observer_write_action, decide_reflection_enqueue,
    evaluate_async_observation_interval, extract_list_items_only,
    filter_observer_candidates_by_last_observed_at,
    format_multi_thread_observer_messages_for_prompt, format_observer_messages_for_prompt,
    merge_activated_observations, merge_buffered_reflection, normalize_observation_buffer_boundary,
    parse_memory_section_xml, parse_memory_section_xml_accuracy_first,
    parse_multi_thread_observer_output, parse_multi_thread_observer_output_accuracy_first,
    plan_buffered_reflection_slice, plan_process_input_step, plan_process_output_result,
    reflection_command_from_action, reflector_compression_guidance, resolve_om_config,
    select_activation_boundary, select_observed_message_candidates,
    select_observer_message_candidates, select_reflection_action,
    should_skip_observer_continuation_hints, should_trigger_observer, should_trigger_reflector,
    split_pending_and_other_conversation_candidates, synthesize_observer_observations,
    validate_om_record_invariants, validate_reflection_compression,
};
pub(crate) use failure::{om_observer_error, om_reflector_error, om_status_kind};
pub(crate) use rollout::{resolve_observer_model_enabled, resolve_reflector_model_enabled};
