use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};

use crate::config::{OmObserverConfigSnapshot, OmRuntimeLimitsConfig, OmScopeConfig};
use crate::error::{AxiomError, OmInferenceFailureKind, Result};
use crate::llm_io::{
    estimate_text_tokens, extract_json_fragment, extract_llm_content,
    parse_local_loopback_endpoint, parse_u32_value,
};
use crate::om::{
    OmApplyAddon, OmCommand, OmInferenceModelConfig, OmInferenceUsage, OmObservationChunk,
    OmObserverMessageCandidate, OmObserverPromptInput, OmObserverRequest, OmObserverResponse,
    OmObserverThreadMessages, OmOriginType, OmPendingMessage, OmRecord, OmReflectionCommandType,
    OmScope, ReflectionEnqueueDecision, ResolvedOmConfig, aggregate_multi_thread_observer_sections,
    build_multi_thread_observer_system_prompt, build_multi_thread_observer_user_prompt,
    build_observer_system_prompt, build_observer_user_prompt, build_other_conversation_blocks,
    combine_observations_for_buffering, filter_observer_candidates_by_last_observed_at,
    format_observer_messages_for_prompt, om_observer_error, om_status_kind,
    parse_memory_section_xml_accuracy_first, parse_multi_thread_observer_output_accuracy_first,
    resolve_observer_model_enabled, select_observed_message_candidates,
    select_observer_message_candidates, split_pending_and_other_conversation_candidates,
    synthesize_observer_observations,
};
use crate::om_bridge::{
    OmObserveBufferRequestedV1, OmReflectBufferRequestedV1, OmReflectRequestedV1,
};

use super::Session;
#[cfg(test)]
use runtime_config::RuntimeOmEnv;
use runtime_config::{resolve_runtime_om_config, runtime_om_env_from_config};

mod observer;
mod runtime_config;
mod scope_binding;
mod write_path;
#[cfg(test)]
use observer::*;
use observer::{
    build_observation_chunk, collect_last_observed_by_thread, merge_observe_after_cursor,
    new_om_record, observed_message_ids_set, record_with_buffered_observation_context,
    resolve_observer_response_with_config, resolve_observer_thread_group_id,
};
#[cfg(test)]
const ENV_OM_SCOPE: &str = scope_binding::ENV_OM_SCOPE;

const DEFAULT_OM_ACTIVATION_RATIO: f32 = 0.8;
const DEFAULT_OM_OBSERVER_MODE: &str = "auto";
const DEFAULT_OM_OBSERVER_LLM_ENDPOINT: &str = "http://127.0.0.1:11434/api/chat";
const DEFAULT_OM_OBSERVER_LLM_MODEL: &str = "qwen2.5:7b-instruct";
const DEFAULT_OM_OBSERVER_LLM_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_OM_OBSERVER_LLM_MAX_OUTPUT_TOKENS: u32 = 1_200;
const DEFAULT_OM_OBSERVER_LLM_TEMPERATURE_MILLI: u16 = 0;
const DEFAULT_OM_OBSERVER_LLM_MAX_CHARS_PER_MESSAGE: usize = 1_200;
const DEFAULT_OM_OBSERVER_LLM_MAX_INPUT_TOKENS: u32 = 12_000;
const ENV_OM_OBSERVER_MAX_TOKENS_PER_BATCH: &str = "AXIOMME_OM_OBSERVER_MAX_TOKENS_PER_BATCH";
const ENV_OM_MESSAGE_TOKENS: &str = "AXIOMME_OM_MESSAGE_TOKENS";
const ENV_OM_REFLECTOR_OBSERVATION_TOKENS: &str = "AXIOMME_OM_REFLECTOR_OBSERVATION_TOKENS";
const ENV_OM_ACTIVATION_RATIO: &str = "AXIOMME_OM_ACTIVATION_RATIO";
const ENV_OM_BUFFER_TOKENS: &str = "AXIOMME_OM_BUFFER_TOKENS";
const ENV_OM_OBSERVER_BLOCK_AFTER: &str = "AXIOMME_OM_OBSERVER_BLOCK_AFTER";
const ENV_OM_REFLECTOR_BUFFER_ACTIVATION: &str = "AXIOMME_OM_REFLECTOR_BUFFER_ACTIVATION";
const ENV_OM_REFLECTOR_BLOCK_AFTER: &str = "AXIOMME_OM_REFLECTOR_BLOCK_AFTER";
const EVENT_OM_OBSERVE_BUFFER_REQUESTED: &str = "om_observe_buffer_requested";
const EVENT_OM_REFLECT_BUFFER_REQUESTED: &str = "om_reflect_buffer_requested";
const EVENT_OM_REFLECT_REQUESTED: &str = "om_reflect_requested";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OmObserverMode {
    Auto,
    Deterministic,
    Llm,
}

impl OmObserverMode {
    fn parse(raw: Option<&str>) -> Self {
        match raw
            .unwrap_or(DEFAULT_OM_OBSERVER_MODE)
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "deterministic" | "local" | "draft" => Self::Deterministic,
            "llm" | "model" => Self::Llm,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone)]
struct OmObserverConfig {
    mode: OmObserverMode,
    model_enabled: bool,
    llm: OmObserverLlmConfig,
    text_budget: OmObserverTextBudget,
}

#[derive(Debug, Clone)]
struct OmObserverLlmConfig {
    endpoint: String,
    model: String,
    timeout_ms: u64,
    max_output_tokens: u32,
    temperature_milli: u16,
    strict: bool,
    max_chars_per_message: usize,
    max_input_tokens: u32,
}

#[derive(Debug, Clone, Copy)]
struct OmObserverTextBudget {
    observation_max_chars: usize,
    active_observations_max_chars: usize,
    other_conversation_max_part_chars: usize,
}

impl OmObserverConfig {
    fn from_snapshot(snapshot: &OmObserverConfigSnapshot, limits: OmRuntimeLimitsConfig) -> Self {
        Self {
            mode: OmObserverMode::parse(snapshot.mode.as_deref()),
            model_enabled: resolve_observer_model_enabled(
                snapshot.explicit_model_enabled,
                snapshot.rollout_profile.as_deref(),
            ),
            llm: OmObserverLlmConfig {
                endpoint: snapshot
                    .llm_endpoint
                    .clone()
                    .unwrap_or_else(|| DEFAULT_OM_OBSERVER_LLM_ENDPOINT.to_string()),
                model: snapshot
                    .llm_model
                    .clone()
                    .unwrap_or_else(|| DEFAULT_OM_OBSERVER_LLM_MODEL.to_string()),
                timeout_ms: snapshot
                    .llm_timeout_ms
                    .unwrap_or(DEFAULT_OM_OBSERVER_LLM_TIMEOUT_MS),
                max_output_tokens: snapshot
                    .llm_max_output_tokens
                    .unwrap_or(DEFAULT_OM_OBSERVER_LLM_MAX_OUTPUT_TOKENS),
                temperature_milli: snapshot
                    .llm_temperature_milli
                    .unwrap_or(DEFAULT_OM_OBSERVER_LLM_TEMPERATURE_MILLI),
                strict: snapshot.llm_strict,
                max_chars_per_message: snapshot
                    .llm_max_chars_per_message
                    .unwrap_or(DEFAULT_OM_OBSERVER_LLM_MAX_CHARS_PER_MESSAGE),
                max_input_tokens: snapshot
                    .llm_max_input_tokens
                    .unwrap_or(DEFAULT_OM_OBSERVER_LLM_MAX_INPUT_TOKENS),
            },
            text_budget: OmObserverTextBudget {
                observation_max_chars: limits.observation_max_chars,
                active_observations_max_chars: limits.observer_active_observations_max_chars,
                other_conversation_max_part_chars: limits
                    .observer_other_conversation_max_part_chars,
            },
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedObserverOutput {
    selected_messages: Vec<OmObserverMessageCandidate>,
    response: OmObserverResponse,
    thread_states: Vec<ObserverThreadStateUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObserverThreadStateUpdate {
    thread_id: String,
    current_task: Option<String>,
    suggested_response: Option<String>,
}

#[derive(Debug, Clone)]
struct ObserverBatchTask {
    index: usize,
    threads: Vec<OmObserverThreadMessages>,
    known_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct ObserverBatchResult {
    index: usize,
    response: OmObserverResponse,
    thread_states: Vec<ObserverThreadStateUpdate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ObserverRunOptions {
    skip_continuation_hints: bool,
    increment_trigger_count: bool,
    observe_outbox_event_id: Option<i64>,
    observe_expected_generation: Option<u32>,
    observe_cursor_after: Option<DateTime<Utc>>,
    strict_cursor_filtering: bool,
}

#[derive(Debug, Clone, Copy)]
struct ObserverRunContext<'a> {
    scope: OmScope,
    scope_key: &'a str,
    now: DateTime<Utc>,
}

struct MultiThreadObserverRunContext<'a> {
    request: &'a OmObserverRequest,
    bounded_selected: &'a [OmObserverMessageCandidate],
    thread_messages: &'a [OmObserverThreadMessages],
    scope: OmScope,
    scope_key: &'a str,
    current_session_id: &'a str,
    max_tokens_per_batch: u32,
    skip_continuation_hints: bool,
}

#[derive(Clone, Copy)]
struct MessageWriteContext<'a> {
    scope: OmScope,
    scope_key: &'a str,
    session_uri: &'a str,
    now: DateTime<Utc>,
    runtime_config: ResolvedOmConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmScopeBinding {
    pub scope: OmScope,
    pub scope_key: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub resource_id: Option<String>,
}

pub(crate) fn resolve_om_scope_binding_for_session_with_config(
    session_id: &str,
    config: &OmScopeConfig,
) -> Result<OmScopeBinding> {
    scope_binding::resolve_om_scope_binding_for_session_with_config(session_id, config)
}

#[cfg(test)]
fn resolve_om_scope_binding(
    session_id: &str,
    scope_raw: Option<&str>,
    thread_id: Option<&str>,
    resource_id: Option<&str>,
) -> Result<OmScopeBinding> {
    scope_binding::resolve_om_scope_binding(session_id, scope_raw, thread_id, resource_id)
}

pub fn resolve_om_scope_binding_explicit(
    session_id: &str,
    scope: OmScope,
    thread_id: Option<&str>,
    resource_id: Option<&str>,
) -> Result<OmScopeBinding> {
    scope_binding::resolve_om_scope_binding_explicit(session_id, scope, thread_id, resource_id)
}

struct SessionOmApplyAddon<'a> {
    session: &'a Session,
    session_uri: String,
}

impl OmApplyAddon for SessionOmApplyAddon<'_> {
    type Error = AxiomError;

    fn apply(&mut self, command: OmCommand) -> std::result::Result<(), Self::Error> {
        match command {
            OmCommand::EnqueueReflection(event) => {
                let (event_type, payload) = match event.command_type {
                    OmReflectionCommandType::BufferRequested => (
                        EVENT_OM_REFLECT_BUFFER_REQUESTED,
                        serde_json::to_value(OmReflectBufferRequestedV1::new(
                            &event.scope_key,
                            event.expected_generation,
                            event.requested_at_rfc3339,
                        ))?,
                    ),
                    OmReflectionCommandType::ReflectRequested => (
                        EVENT_OM_REFLECT_REQUESTED,
                        serde_json::to_value(OmReflectRequestedV1::new(
                            &event.scope_key,
                            event.expected_generation,
                            event.requested_at_rfc3339,
                        ))?,
                    ),
                };
                self.session
                    .state
                    .enqueue(event_type, &self.session_uri, payload)?;
                Ok(())
            }
        }
    }
}

impl Session {
    fn om_enabled(&self) -> bool {
        self.config.om.enabled
    }

    fn observer_config(&self) -> OmObserverConfig {
        OmObserverConfig::from_snapshot(&self.config.om.observer, self.config.om.limits)
    }

    fn effective_om_scope_binding(&self) -> Result<OmScopeBinding> {
        if let Some(binding) = self.om_scope_binding_override.clone() {
            return Ok(binding);
        }
        resolve_om_scope_binding_for_session_with_config(&self.session_id, &self.config.om.scope)
    }

    fn collect_observer_messages_for_scope(
        &self,
        scope: OmScope,
        scope_key: &str,
        last_observed_at: Option<DateTime<Utc>>,
        observed_message_ids: &HashSet<String>,
        max_messages: usize,
        strict_cursor_filtering: bool,
    ) -> Result<Vec<OmObserverMessageCandidate>> {
        let mut candidates = self
            .read_messages()?
            .into_iter()
            .map(|message| OmObserverMessageCandidate {
                id: message.id,
                role: message.role,
                text: message.text,
                created_at: message.created_at,
                source_thread_id: Some(self.session_id.clone()),
                source_session_id: Some(self.session_id.clone()),
            })
            .collect::<Vec<_>>();

        if scope != OmScope::Session {
            let peer_limit = self
                .config
                .om
                .limits
                .resource_scope_cross_session_limit
                .saturating_add(1);
            let peer_sessions = self.state.list_om_scope_sessions(scope_key, peer_limit)?;
            for peer_session_id in peer_sessions {
                if peer_session_id == self.session_id {
                    continue;
                }
                let peer = Self::new(
                    peer_session_id.clone(),
                    self.fs.clone(),
                    self.state.clone(),
                    self.index.clone(),
                );
                if let Ok(peer_messages) = peer.read_messages() {
                    candidates.extend(peer_messages.into_iter().map(|message| {
                        OmObserverMessageCandidate {
                            id: message.id,
                            role: message.role,
                            text: message.text,
                            created_at: message.created_at,
                            source_thread_id: Some(peer_session_id.clone()),
                            source_session_id: Some(peer_session_id.clone()),
                        }
                    }));
                }
            }
        }

        let unobserved_candidates =
            filter_observer_candidates_by_last_observed_at(&candidates, last_observed_at);
        let candidate_pool = if strict_cursor_filtering {
            &unobserved_candidates
        } else if unobserved_candidates.is_empty() {
            &candidates
        } else {
            &unobserved_candidates
        };
        Ok(select_observer_message_candidates(
            candidate_pool,
            observed_message_ids,
            max_messages,
        ))
    }

    fn apply_reflection_decision(
        &self,
        reflection_decision: ReflectionEnqueueDecision,
        session_uri: &str,
        record: &mut OmRecord,
    ) -> Result<()> {
        if let Some(command) = reflection_decision.command {
            let mut addon = SessionOmApplyAddon {
                session: self,
                session_uri: session_uri.to_string(),
            };
            addon.apply(command)?;
            if reflection_decision.should_increment_trigger_count {
                record.reflector_trigger_count_total =
                    record.reflector_trigger_count_total.saturating_add(1);
            }
            record.is_reflecting = reflection_decision.next_is_reflecting;
            record.is_buffering_reflection = reflection_decision.next_is_buffering_reflection;
        }
        Ok(())
    }

    fn enqueue_observer_buffer_request(
        &self,
        session_uri: &str,
        scope_key: &str,
        expected_generation: u32,
        requested_at: DateTime<Utc>,
    ) -> Result<()> {
        let payload = serde_json::to_value(OmObserveBufferRequestedV1::new(
            scope_key,
            expected_generation,
            requested_at.to_rfc3339(),
            Some(&self.session_id),
        ))?;
        self.state
            .enqueue(EVENT_OM_OBSERVE_BUFFER_REQUESTED, session_uri, payload)?;
        Ok(())
    }

    fn run_observer_pass(
        &self,
        context: ObserverRunContext<'_>,
        max_tokens_per_batch: u32,
        record: &mut OmRecord,
        buffered_chunks: &mut Vec<OmObservationChunk>,
        options: ObserverRunOptions,
    ) -> Result<bool> {
        if options.increment_trigger_count {
            record.observer_trigger_count_total =
                record.observer_trigger_count_total.saturating_add(1);
        }
        let observe_after =
            merge_observe_after_cursor(record.last_observed_at, options.observe_cursor_after);
        let observed_message_ids =
            observed_message_ids_set(&record.last_activated_message_ids, buffered_chunks);
        let selected = self.collect_observer_messages_for_scope(
            context.scope,
            context.scope_key,
            observe_after,
            &observed_message_ids,
            self.config.om.limits.observer_max_messages,
            options.strict_cursor_filtering,
        )?;
        let observer_config = self.observer_config();
        let observer_context_record = record_with_buffered_observation_context(
            record,
            buffered_chunks,
            observer_config.text_budget.active_observations_max_chars,
        );
        let observer_output = resolve_observer_response_with_config(
            &observer_context_record,
            context.scope_key,
            &selected,
            &self.session_id,
            max_tokens_per_batch,
            options.skip_continuation_hints,
            &observer_config,
        )?;

        if !options.skip_continuation_hints {
            record
                .current_task
                .clone_from(&observer_output.response.current_task);
            record
                .suggested_response
                .clone_from(&observer_output.response.suggested_response);
        }
        self.upsert_observer_thread_states(context, &observer_output)?;
        self.append_observer_chunk(
            context,
            options,
            record,
            buffered_chunks,
            &observer_output,
            observer_config.text_budget.observation_max_chars,
        )
    }

    fn upsert_observer_thread_states(
        &self,
        context: ObserverRunContext<'_>,
        observer_output: &ResolvedObserverOutput,
    ) -> Result<()> {
        if context.scope == OmScope::Session {
            return Ok(());
        }
        let primary_thread_id = resolve_observer_thread_group_id(
            context.scope,
            context.scope_key,
            Some(&self.session_id),
            Some(&self.session_id),
            &self.session_id,
        );
        let last_observed_by_thread = collect_last_observed_by_thread(
            context.scope,
            context.scope_key,
            &self.session_id,
            &observer_output.selected_messages,
        );
        let mut thread_state_updates = observer_output
            .thread_states
            .iter()
            .map(|state| (state.thread_id.clone(), state))
            .collect::<BTreeMap<_, _>>();
        for (thread_id, last_observed_at) in &last_observed_by_thread {
            let state = thread_state_updates.remove(thread_id);
            let (current_task, suggested_response) = match state {
                Some(value) => (
                    value.current_task.as_deref(),
                    value.suggested_response.as_deref(),
                ),
                None if thread_id == &primary_thread_id => (
                    observer_output.response.current_task.as_deref(),
                    observer_output.response.suggested_response.as_deref(),
                ),
                None => (None, None),
            };
            self.state.upsert_om_thread_state(
                context.scope_key,
                thread_id,
                Some(*last_observed_at),
                current_task,
                suggested_response,
            )?;
        }
        for (thread_id, state) in thread_state_updates {
            self.state.upsert_om_thread_state(
                context.scope_key,
                &thread_id,
                None,
                state.current_task.as_deref(),
                state.suggested_response.as_deref(),
            )?;
        }
        Ok(())
    }

    fn append_observer_chunk(
        &self,
        context: ObserverRunContext<'_>,
        options: ObserverRunOptions,
        record: &mut OmRecord,
        buffered_chunks: &mut Vec<OmObservationChunk>,
        observer_output: &ResolvedObserverOutput,
        observation_max_chars: usize,
    ) -> Result<bool> {
        let Some(chunk) = build_observation_chunk(
            &record.id,
            &observer_output.selected_messages,
            buffered_chunks,
            context.now,
            &observer_output.response.observations,
            observation_max_chars,
        ) else {
            return Ok(false);
        };

        let appended = if let Some(outbox_event_id) = options.observe_outbox_event_id {
            self.state.append_om_observation_chunk_with_event_cas(
                context.scope_key,
                options
                    .observe_expected_generation
                    .unwrap_or(record.generation_count),
                outbox_event_id,
                &chunk,
            )?
        } else {
            self.state.append_om_observation_chunk(&chunk)?;
            true
        };
        if !appended {
            return Ok(false);
        }

        record.is_buffering_observation = true;
        // Persist boundary as current pending tokens to support interval crossing checks.
        record.last_buffered_at_tokens = record.pending_message_tokens;
        record.last_buffered_at_time = Some(chunk.last_observed_at);
        buffered_chunks.push(chunk);
        Ok(true)
    }

    pub(crate) fn process_om_observe_buffer_requested(
        &self,
        scope_key: &str,
        expected_generation: u32,
        outbox_event_id: i64,
    ) -> Result<bool> {
        if !self.om_enabled() {
            return Ok(true);
        }
        if self.state.om_observer_event_applied(outbox_event_id)? {
            return Ok(true);
        }

        let Some(mut record) = self.state.get_om_record_by_scope_key(scope_key)? else {
            return Ok(true);
        };
        if record.generation_count != expected_generation {
            return Ok(true);
        }

        let runtime_env = runtime_om_env_from_config(&self.config.om.runtime_env);
        let runtime_config = resolve_runtime_om_config(&runtime_env, record.scope)?;

        let now = Utc::now();
        let observe_cursor_after = record.last_buffered_at_time;
        let mut buffered_chunks = self.state.list_om_observation_chunks(&record.id)?;
        let appended = self.run_observer_pass(
            ObserverRunContext {
                scope: record.scope,
                scope_key,
                now,
            },
            runtime_config.observation.max_tokens_per_batch,
            &mut record,
            &mut buffered_chunks,
            ObserverRunOptions {
                skip_continuation_hints: true,
                increment_trigger_count: false,
                observe_outbox_event_id: Some(outbox_event_id),
                observe_expected_generation: Some(expected_generation),
                observe_cursor_after,
                strict_cursor_filtering: true,
            },
        )?;
        if !appended {
            return Ok(true);
        }
        record.updated_at = now;
        self.state.upsert_om_record(&record)?;
        Ok(true)
    }

    pub(super) fn record_observer_failure(&self, err: &AxiomError) {
        if !self.om_enabled() {
            return;
        }
        let uri = format!("axiom://session/{}", self.session_id);
        let (failure_kind, failure_source) = match err {
            AxiomError::OmInference {
                inference_source,
                kind,
                ..
            } => (Some(kind.as_str()), Some(inference_source.as_str())),
            _ => (None, None),
        };
        let payload = serde_json::json!({
            "session_id": self.session_id,
            "error_code": err.code(),
            "error": err.to_string(),
            "om_failure_kind": failure_kind,
            "om_failure_source": failure_source,
        });
        let _ = self
            .state
            .enqueue_dead_letter("om_observer_failed", &uri, payload);
    }
}

#[cfg(test)]
mod tests;
