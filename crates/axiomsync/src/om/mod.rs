// OM pure contracts are vendored into `engine`. This module is the explicit runtime boundary:
// re-export pure types/transforms and keep only AxiomSync-specific rollout/error helpers local.
pub(crate) mod engine;
mod failure;
mod rollout;
mod thread_identity;

pub use engine::{
    ActivationBoundary, ActivationResult, AsyncObservationIntervalState,
    BUFFERED_OBSERVATIONS_SEPARATOR, BufferTokensInput, BufferedReflectionSlicePlan,
    ContinuationPolicyV2, DEFAULT_BLOCK_AFTER_MULTIPLIER, DEFAULT_OBSERVER_BUFFER_ACTIVATION,
    DEFAULT_OBSERVER_BUFFER_TOKENS_RATIO, DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH,
    DEFAULT_OBSERVER_MESSAGE_TOKENS, DEFAULT_REFLECTOR_BUFFER_ACTIVATION,
    DEFAULT_REFLECTOR_OBSERVATION_TOKENS, OM_PROMPT_CONTRACT_VERSION, OM_PROTOCOL_VERSION,
    OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION, ObservationConfigInput, ObserverWriteDecision,
    OmApplyAddon, OmCommand, OmConfigError, OmConfigInput, OmContinuationCandidateV2,
    OmContinuationSourceKind, OmContinuationStateV2, OmDeterministicObserverResponseV2,
    OmHintPolicyV2, OmInferenceModelConfig, OmInferenceUsage, OmMemorySection,
    OmMultiThreadObserverAggregate, OmMultiThreadObserverSection, OmObservationChunk,
    OmObservationEntryV2, OmObservationOriginKind, OmObservationPriority, OmObserverAddon,
    OmObserverMessageCandidate, OmObserverPromptContractV2, OmObserverPromptInput,
    OmObserverRequest, OmObserverResponse, OmObserverThreadMessages, OmOriginType, OmParseMode,
    OmPendingMessage, OmPromptContractHeader, OmPromptLimitsV2, OmPromptOutputContractV2,
    OmPromptRequestKind, OmRecord, OmRecordInvariantViolation, OmReflectionCommand,
    OmReflectionCommandType, OmReflectorAddon, OmReflectorPromptContractV2, OmReflectorPromptInput,
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
    infer_deterministic_observer_response, materialize_search_visible_snapshot,
    merge_activated_observations, merge_buffered_reflection, normalize_observation_buffer_boundary,
    parse_memory_section_xml, parse_memory_section_xml_accuracy_first,
    parse_multi_thread_observer_output, parse_multi_thread_observer_output_accuracy_first,
    plan_buffered_reflection_slice, plan_process_input_step, plan_process_output_result,
    reflection_command_from_action, reflector_compression_guidance, resolve_continuation_update,
    resolve_om_config, select_activation_boundary, select_observed_message_candidates,
    select_observer_message_candidates, select_reflection_action,
    should_skip_observer_continuation_hints, should_trigger_observer, should_trigger_reflector,
    split_pending_and_other_conversation_candidates, synthesize_observer_observations,
    validate_om_record_invariants, validate_reflection_compression,
};
pub(crate) use failure::{om_observer_error, om_reflector_error, om_status_kind};
pub(crate) use rollout::{resolve_observer_model_enabled, resolve_reflector_model_enabled};
pub use thread_identity::resolve_canonical_thread_id;

pub const OM_PROMPT_CONTRACT_NAME: &str = "axiomsync.om.prompt";

#[must_use]
pub fn build_observer_prompt_contract_v2(
    request: &OmObserverRequest,
    known_message_ids: &[String],
    skip_continuation_hints: bool,
    preferred_thread_id: Option<&str>,
    observation_max_chars: usize,
) -> OmObserverPromptContractV2 {
    let mut contract = engine::build_observer_prompt_contract_v2(
        request,
        known_message_ids,
        skip_continuation_hints,
        preferred_thread_id,
        observation_max_chars,
    );
    contract.header.contract_name = OM_PROMPT_CONTRACT_NAME.to_string();
    contract
}

#[must_use]
pub fn build_multi_thread_observer_prompt_contract_v2(
    request: &OmObserverRequest,
    known_message_ids: &[String],
    skip_continuation_hints: bool,
    preferred_thread_id: Option<&str>,
    observation_max_chars: usize,
) -> OmObserverPromptContractV2 {
    let mut contract = engine::build_multi_thread_observer_prompt_contract_v2(
        request,
        known_message_ids,
        skip_continuation_hints,
        preferred_thread_id,
        observation_max_chars,
    );
    contract.header.contract_name = OM_PROMPT_CONTRACT_NAME.to_string();
    contract
}

#[must_use]
pub fn build_reflector_prompt_contract_v2(
    request: &OmReflectorRequest,
    compression_level: u8,
    skip_continuation_hints: bool,
    reflection_max_chars: usize,
) -> OmReflectorPromptContractV2 {
    let mut contract = engine::build_reflector_prompt_contract_v2(
        request,
        compression_level,
        skip_continuation_hints,
        reflection_max_chars,
    );
    contract.header.contract_name = OM_PROMPT_CONTRACT_NAME.to_string();
    contract
}
