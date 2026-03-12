#![allow(
    dead_code,
    unused_imports,
    reason = "vendored OM engine keeps a broad pure-core surface while axiomsync uses a narrower runtime subset"
)]

pub(crate) mod addon;
pub(crate) mod config;
mod context;
pub(crate) mod inference;
pub(crate) mod model;
mod parse;
mod pipeline;
mod prompt;
mod transform;
pub(crate) mod xml;

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
pub use inference::{
    OmInferenceModelConfig, OmInferenceUsage, OmObserverRequest, OmObserverResponse,
    OmPendingMessage, OmReflectorRequest, OmReflectorResponse,
};
pub use model::{
    ContinuationPolicyV2, OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION, OmContinuationCandidateV2,
    OmContinuationSourceKind, OmContinuationStateV2, OmDeterministicEvidence,
    OmDeterministicEvidenceKind, OmDeterministicObserverResponseV2, OmHintPolicyV2,
    OmObservationChunk, OmObservationEntryInvariantViolation, OmObservationEntryV2,
    OmObservationOriginKind, OmObservationPriority, OmOriginType, OmRecord,
    OmRecordInvariantViolation, OmReflectionResponseV2, OmScope,
    OmSearchVisibleSnapshotInvariantViolation, OmSearchVisibleSnapshotV2, OmThreadRefV2,
    validate_observation_entry_v2_invariants, validate_om_record_invariants,
    validate_search_visible_snapshot_v2_invariants,
};
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
    OM_PROMPT_CONTRACT_NAME, OM_PROMPT_CONTRACT_VERSION, OM_PROTOCOL_VERSION,
    OmObserverPromptContractV2, OmObserverPromptInput, OmObserverThreadMessages,
    OmPromptContractHeader, OmPromptContractParseError, OmPromptLimitsV2, OmPromptOutputContractV2,
    OmPromptRequestKind, OmReflectorPromptContractV2, OmReflectorPromptInput,
    build_multi_thread_observer_prompt_contract_v2, build_multi_thread_observer_system_prompt,
    build_multi_thread_observer_user_prompt, build_observer_prompt_contract_v2,
    build_observer_system_prompt, build_observer_user_prompt, build_reflector_prompt_contract_v2,
    build_reflector_system_prompt, build_reflector_user_prompt,
    format_multi_thread_observer_messages_for_prompt, format_observer_messages_for_prompt,
    parse_observer_prompt_contract_v2, parse_reflector_prompt_contract_v2,
};
pub use transform::{
    ActivationBoundary, ActivationResult, AsyncObservationIntervalState,
    BUFFERED_OBSERVATIONS_SEPARATOR, BufferedReflectionSlicePlan, ObserverWriteDecision,
    OmObserverMessageCandidate, OmTransformError, ReflectionAction, ReflectionDraft,
    ReflectionEnqueueDecision, activate_buffered_observations, apply_reflection_response_v2,
    build_other_conversation_blocks, build_reflection_draft, build_scope_key,
    calculate_dynamic_threshold, combine_observations_for_buffering, compute_pending_tokens,
    decide_observer_write_action, decide_reflection_enqueue, evaluate_async_observation_interval,
    filter_observer_candidates_by_last_observed_at, infer_deterministic_continuation,
    infer_deterministic_observer_response, materialize_search_visible_snapshot,
    merge_activated_observations, merge_buffered_reflection, normalize_observation_buffer_boundary,
    plan_buffered_reflection_slice, reflector_compression_guidance, render_search_hint,
    resolve_canonical_thread_ref, resolve_continuation_update, select_activation_boundary,
    select_observed_message_candidates, select_observer_message_candidates,
    select_reflection_action, should_skip_observer_continuation_hints, should_trigger_observer,
    should_trigger_reflector, split_pending_and_other_conversation_candidates,
    synthesize_observer_observations, validate_reflection_compression,
};
