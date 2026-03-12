use std::collections::HashSet;
use std::time::Instant;

use chrono::Utc;

use crate::config::{
    OmHintBounds, OmHintPolicy, OmHintReaderMode, RETRIEVAL_BACKEND_MEMORY,
    RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY,
};
use crate::context_ops::validate_filter;
use crate::error::{AxiomError, Result};
use crate::llm_io::estimate_text_tokens;
use crate::models::{
    FindResult, Message, MetadataFilter, RequestLogEntry, RuntimeHint, SearchBudget, SearchOptions,
    SearchRequest,
};
use crate::om::{
    OmHintPolicyV2, OmObservationEntryV2, OmScope, build_bounded_observation_hint,
    materialize_search_visible_snapshot, resolve_canonical_thread_id,
};
use crate::om_bridge::OmHintReadStateV1;
use crate::session::resolve_om_scope_binding_for_session_with_config;
use crate::state::OmActiveEntry;
use crate::uri::AxiomUri;

use super::AxiomSync;

mod backend;
mod reranker;
mod result;
mod snapshot;
mod telemetry;

use result::{
    annotate_trace_relation_metrics, annotate_typed_edge_query_plan_visibility, budget_to_json,
    metadata_filter_to_search_filter, normalize_budget,
};
use snapshot::{
    build_snapshot_activated_entries, build_snapshot_buffered_entries,
    build_snapshot_continuation_state, snapshot_visible_entry_selection,
    snapshot_visible_observation_text,
};
#[cfg(test)]
use snapshot::{infer_buffered_entry_priority, snapshot_visible_entry_source_key};
use telemetry::{
    SearchRequestLogEvent, SearchRequestLogInput, annotate_om_query_plan_visibility,
    search_request_details,
};

const DEFAULT_OM_SCOPE_LOOKUP_FALLBACK_LIMIT: usize = 4;
const OM_HINT_SNAPSHOT_BUFFERED_TAIL_LIMIT: usize = 2;
const OM_HINT_COMPACTION_RESERVED_HIGH_LIMIT: usize = 1;

#[derive(Debug, Clone, Default)]
struct OmSearchMetrics {
    context_tokens_before_om: u32,
    context_tokens_after_om: u32,
    observation_tokens_active: u32,
    observer_trigger_count: u32,
    reflector_trigger_count: u32,
    om_hint_applied: bool,
    om_hint_reader_snapshot_v2: bool,
    om_snapshot_buffered_chunk_count: u32,
    om_snapshot_buffered_chunk_ids: Vec<String>,
    om_hint_compaction_priority_v2: bool,
    om_hint_high_priority_selected_count: u32,
    om_snapshot_visible_entry_ids: Vec<String>,
    om_snapshot_visible_activated_entry_ids: Vec<String>,
    om_snapshot_visible_buffered_entry_ids: Vec<String>,
    om_hint_current_task_reserved: bool,
    session_recent_hint_count: u32,
    session_hint_count_final: u32,
    om_filtered_message_count: u32,
}

#[derive(Debug, Clone, Default)]
struct SearchHintLayers {
    runtime: Vec<String>,
    recent: Vec<String>,
    om_hint: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct SessionHintSnapshot {
    recent_hints: Vec<String>,
    om_hint: Option<String>,
    metrics: OmSearchMetrics,
}

#[derive(Debug, Clone, Default)]
struct OmHintSnapshotV2 {
    scope_key: String,
    snapshot_version: String,
    materialized_at: String,
    search_visible_observations: String,
    rendered_hint: Option<String>,
    current_task: Option<String>,
    suggested_response: Option<String>,
    activated_message_ids: Vec<String>,
    buffered_chunk_ids: Vec<String>,
    selected_entry_ids: Vec<String>,
    activated_visible_entry_ids: Vec<String>,
    buffered_visible_entry_ids: Vec<String>,
    observation_tokens_active: u32,
    observer_trigger_count_total: u32,
    reflector_trigger_count_total: u32,
    buffered_chunk_count: usize,
    high_priority_selected_count: usize,
}

impl OmHintSnapshotV2 {
    fn to_read_state(&self, bounds: OmHintBounds) -> OmHintReadStateV1 {
        OmHintReadStateV1 {
            scope_key: self.scope_key.clone(),
            hint: self.rendered_hint.clone().or_else(|| {
                bounded_om_hint_from_snapshot(
                    &self.search_visible_observations,
                    self.current_task.as_deref(),
                    self.suggested_response.as_deref(),
                    bounds,
                )
            }),
            snapshot_version: Some(self.snapshot_version.clone()),
            materialized_at: Some(self.materialized_at.clone()),
            activated_message_ids: self.activated_message_ids.clone(),
            buffered_chunk_ids: self.buffered_chunk_ids.clone(),
            selected_entry_ids: self.selected_entry_ids.clone(),
            observation_tokens_active: self.observation_tokens_active,
            observer_trigger_count_total: self.observer_trigger_count_total,
            reflector_trigger_count_total: self.reflector_trigger_count_total,
        }
    }
}

#[derive(Debug)]
struct SearchOptionsInput {
    query: String,
    target_uri: Option<AxiomUri>,
    session: Option<String>,
    session_hints: Vec<String>,
    budget: Option<SearchBudget>,
    requested_limit: usize,
    score_threshold: Option<f32>,
    min_match_tokens: Option<usize>,
    filter: Option<MetadataFilter>,
    request_type: &'static str,
}

#[derive(Debug)]
struct SnapshotEntryInputs {
    fallback_thread_id: String,
    reserved_high_texts: Vec<String>,
    high_priority_selected_count: usize,
    buffered_entries: Vec<OmObservationEntryV2>,
    buffered_chunk_ids: Vec<String>,
}

impl AxiomSync {
    #[inline]
    fn om_snapshot_hint_reader_enabled(&self) -> bool {
        self.config.om.enabled && self.config.om.hint_reader == OmHintReaderMode::SnapshotV2
    }

    #[must_use]
    pub fn search_requires_runtime_prepare(&self) -> bool {
        true
    }

    pub fn find(
        &self,
        query: &str,
        target_uri: Option<&str>,
        limit: Option<usize>,
        score_threshold: Option<f32>,
        filter: Option<MetadataFilter>,
    ) -> Result<FindResult> {
        self.find_with_budget(query, target_uri, limit, score_threshold, filter, None)
    }

    pub fn find_with_budget(
        &self,
        query: &str,
        target_uri: Option<&str>,
        limit: Option<usize>,
        score_threshold: Option<f32>,
        filter: Option<MetadataFilter>,
        budget: Option<SearchBudget>,
    ) -> Result<FindResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let target_raw = target_uri.map(ToString::to_string);
        let requested_limit = limit.unwrap_or(10);
        let budget = normalize_budget(budget);
        let typed_edge_enrichment = self.config.search.typed_edge_enrichment;

        let output = (|| -> Result<FindResult> {
            validate_filter(filter.as_ref())?;
            validate_search_cutoff_options(score_threshold, None)?;
            let target = parse_optional_target_uri(target_uri)?;
            let options = build_search_options(SearchOptionsInput {
                query: query.to_string(),
                target_uri: target,
                session: None,
                session_hints: Vec::new(),
                budget: budget.clone(),
                requested_limit,
                score_threshold,
                min_match_tokens: None,
                filter,
                request_type: "find",
            });

            let mut result = self.run_retrieval_memory_only(&options)?;
            self.enrich_find_result_relations(&mut result, 5, typed_edge_enrichment)?;
            annotate_trace_relation_metrics(&mut result);
            annotate_typed_edge_query_plan_visibility(&mut result, typed_edge_enrichment);
            self.persist_trace_result(&mut result)?;
            Ok(result)
        })();

        match output {
            Ok(result) => {
                let trace_id = result.trace.as_ref().map(|x| x.trace_id.clone());
                let details = serde_json::json!({
                    "query": query,
                    "result_count": result.query_results.len(),
                    "limit": requested_limit,
                    "budget": budget_to_json(budget.as_ref()),
                    "retrieval_backend": RETRIEVAL_BACKEND_MEMORY,
                    "retrieval_backend_policy": RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY,
                    "typed_edge_enrichment": typed_edge_enrichment,
                });
                self.try_log_request(&RequestLogEntry {
                    request_id,
                    operation: "find".to_string(),
                    status: "ok".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id,
                    target_uri: target_raw,
                    error_code: None,
                    error_message: None,
                    details: Some(details),
                });
                Ok(result)
            }
            Err(err) => {
                self.try_log_request(&RequestLogEntry {
                    request_id,
                    operation: "find".to_string(),
                    status: "error".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id: None,
                    target_uri: target_raw,
                    error_code: Some(err.code().to_string()),
                    error_message: Some(err.to_string()),
                    details: Some(serde_json::json!({
                        "query": query,
                        "limit": requested_limit,
                        "budget": budget_to_json(budget.as_ref()),
                        "retrieval_backend": RETRIEVAL_BACKEND_MEMORY,
                        "retrieval_backend_policy": RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY,
                        "typed_edge_enrichment": typed_edge_enrichment,
                    })),
                });
                Err(err)
            }
        }
    }

    pub fn search(
        &self,
        query: &str,
        target_uri: Option<&str>,
        session: Option<&str>,
        limit: Option<usize>,
        score_threshold: Option<f32>,
        filter: Option<MetadataFilter>,
    ) -> Result<FindResult> {
        self.search_with_request(SearchRequest {
            query: query.to_string(),
            target_uri: target_uri.map(ToString::to_string),
            session: session.map(ToString::to_string),
            limit,
            score_threshold,
            min_match_tokens: None,
            filter,
            budget: None,
            runtime_hints: Vec::new(),
        })
    }

    fn build_search_session_hints(
        &self,
        session_id: &str,
        query: &str,
        hint_policy: OmHintPolicy,
        hint_bounds: OmHintBounds,
    ) -> Result<SessionHintSnapshot> {
        let snapshot_reader_enabled = self.om_snapshot_hint_reader_enabled();
        let mut metrics = OmSearchMetrics {
            om_hint_reader_snapshot_v2: snapshot_reader_enabled,
            ..OmSearchMetrics::default()
        };
        let ctx = self.session(Some(session_id)).get_context_for_search(
            query,
            hint_policy.context_max_archives,
            hint_policy.context_max_messages,
        )?;
        let om_snapshot =
            self.fetch_session_om_hint_snapshot_with_enabled(session_id, snapshot_reader_enabled)?;
        metrics.om_snapshot_buffered_chunk_count = om_snapshot.as_ref().map_or(0, |snapshot| {
            saturating_usize_to_u32(snapshot.buffered_chunk_count)
        });
        metrics.om_snapshot_buffered_chunk_ids = om_snapshot
            .as_ref()
            .map_or_else(Vec::new, |snapshot| snapshot.buffered_chunk_ids.clone());
        metrics.om_hint_compaction_priority_v2 = om_snapshot.is_some();
        metrics.om_hint_high_priority_selected_count = om_snapshot.as_ref().map_or(0, |snapshot| {
            saturating_usize_to_u32(snapshot.high_priority_selected_count)
        });
        metrics.om_snapshot_visible_entry_ids = om_snapshot
            .as_ref()
            .map_or_else(Vec::new, |snapshot| snapshot.selected_entry_ids.clone());
        metrics.om_snapshot_visible_activated_entry_ids =
            om_snapshot.as_ref().map_or_else(Vec::new, |snapshot| {
                snapshot.activated_visible_entry_ids.clone()
            });
        metrics.om_snapshot_visible_buffered_entry_ids =
            om_snapshot.as_ref().map_or_else(Vec::new, |snapshot| {
                snapshot.buffered_visible_entry_ids.clone()
            });
        metrics.om_hint_current_task_reserved = om_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.current_task.is_some());
        let om_state = om_snapshot
            .as_ref()
            .map(|snapshot| snapshot.to_read_state(hint_bounds));
        let pre_om_recent_hints =
            collect_recent_hints(&ctx.recent_messages, hint_policy.recent_hint_limit);
        let pre_om_hints = merge_recent_and_om_hints(&pre_om_recent_hints, None, hint_policy);
        let om_hint = om_state
            .as_ref()
            .and_then(|state| state.hint.as_deref())
            .map(ToString::to_string);
        let filtered_recent_messages =
            om_state
                .as_ref()
                .filter(|state| state.hint.is_some())
                .map(|state| {
                    filter_recent_messages_by_ids(
                        &ctx.recent_messages,
                        &state.activated_message_ids,
                    )
                });
        let recent_messages_for_hints = filtered_recent_messages
            .as_deref()
            .unwrap_or(ctx.recent_messages.as_slice());
        let recent_hints =
            collect_recent_hints(recent_messages_for_hints, hint_policy.recent_hint_limit);
        let merged_hints = merge_runtime_om_recent_hints(
            &[],
            om_hint.as_deref(),
            &recent_hints,
            hint_policy,
            hint_bounds.max_chars,
        );

        metrics.context_tokens_before_om = estimate_hint_tokens(&pre_om_hints);
        metrics.om_hint_applied = om_state
            .as_ref()
            .and_then(|state| state.hint.as_ref())
            .is_some();
        metrics.observer_trigger_count = om_state
            .as_ref()
            .map_or(0, |state| state.observer_trigger_count_total);
        metrics.reflector_trigger_count = om_state
            .as_ref()
            .map_or(0, |state| state.reflector_trigger_count_total);
        metrics.observation_tokens_active = om_state
            .as_ref()
            .map_or(0, |state| state.observation_tokens_active);
        metrics.session_recent_hint_count = saturating_usize_to_u32(pre_om_recent_hints.len());
        metrics.session_hint_count_final = saturating_usize_to_u32(merged_hints.len());
        metrics.om_filtered_message_count =
            saturating_usize_to_u32(filtered_recent_messages.as_ref().map_or(0, |messages| {
                ctx.recent_messages.len().saturating_sub(messages.len())
            }));
        metrics.context_tokens_after_om = estimate_hint_tokens(&merged_hints);

        Ok(SessionHintSnapshot {
            recent_hints,
            om_hint,
            metrics,
        })
    }

    fn resolve_search_hints(
        &self,
        session_id: Option<&str>,
        query: &str,
        runtime_hints: &[RuntimeHint],
        hint_policy: OmHintPolicy,
        hint_bounds: OmHintBounds,
    ) -> Result<(Vec<String>, OmSearchMetrics)> {
        let normalized_runtime_hints = normalize_runtime_hints(
            runtime_hints,
            hint_policy.total_hint_limit,
            hint_bounds.max_chars,
        );
        let mut hint_layers = SearchHintLayers {
            runtime: normalized_runtime_hints,
            ..SearchHintLayers::default()
        };
        let mut om_metrics = OmSearchMetrics::default();
        if let Some(session_id) = session_id {
            let snapshot =
                self.build_search_session_hints(session_id, query, hint_policy, hint_bounds)?;
            hint_layers.recent = snapshot.recent_hints;
            hint_layers.om_hint = snapshot.om_hint;
            om_metrics = snapshot.metrics;
        }
        let session_hints = merge_runtime_om_recent_hints(
            &hint_layers.runtime,
            hint_layers.om_hint.as_deref(),
            &hint_layers.recent,
            hint_policy,
            hint_bounds.max_chars,
        );
        if session_id.is_some() {
            om_metrics.session_hint_count_final = saturating_usize_to_u32(session_hints.len());
            om_metrics.context_tokens_after_om = estimate_hint_tokens(&session_hints);
        }
        Ok((session_hints, om_metrics))
    }

    fn try_log_search_request(&self, event: SearchRequestLogEvent<'_>) {
        self.try_log_request(&RequestLogEntry {
            request_id: event.request_id.to_string(),
            operation: "search".to_string(),
            status: event.status.to_string(),
            latency_ms: event.started.elapsed().as_millis(),
            created_at: Utc::now().to_rfc3339(),
            trace_id: event.trace_id.map(ToString::to_string),
            target_uri: event.target_uri.map(ToString::to_string),
            error_code: event.error.map(|err| err.code().to_string()),
            error_message: event.error.map(ToString::to_string),
            details: Some(event.details),
        });
    }

    pub fn search_with_request(&self, request: SearchRequest) -> Result<FindResult> {
        let SearchRequest {
            query,
            target_uri,
            session,
            limit,
            score_threshold,
            min_match_tokens,
            filter,
            budget,
            runtime_hints,
        } = request;
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let target_raw = target_uri.clone();
        let session_raw = session.clone();
        let requested_limit = limit.unwrap_or(10);
        let budget = normalize_budget(budget);
        let hint_policy = self.config.search.om_hint_policy;
        let hint_bounds = self.config.search.om_hint_bounds;
        let typed_edge_enrichment = self.config.search.typed_edge_enrichment;
        let mut om_metrics = OmSearchMetrics::default();

        let output = (|| -> Result<FindResult> {
            validate_filter(filter.as_ref())?;
            validate_search_cutoff_options(score_threshold, min_match_tokens)?;
            let target = parse_optional_target_uri(target_uri.as_deref())?;
            let (session_hints, resolved_metrics) = self.resolve_search_hints(
                session.as_deref(),
                &query,
                &runtime_hints,
                hint_policy,
                hint_bounds,
            )?;
            om_metrics = resolved_metrics;

            let options = build_search_options(SearchOptionsInput {
                query: query.clone(),
                target_uri: target,
                session: session.clone(),
                session_hints,
                budget: budget.clone(),
                requested_limit,
                score_threshold,
                min_match_tokens,
                filter,
                request_type: "search",
            });

            let mut result = self.run_retrieval_memory_only(&options)?;
            self.enrich_find_result_relations(&mut result, 5, typed_edge_enrichment)?;
            annotate_trace_relation_metrics(&mut result);
            annotate_typed_edge_query_plan_visibility(&mut result, typed_edge_enrichment);
            annotate_om_query_plan_visibility(&mut result, &om_metrics, hint_policy);
            self.persist_trace_result(&mut result)?;
            Ok(result)
        })();

        match output {
            Ok(result) => {
                let trace_id = result.trace.as_ref().map(|x| x.trace_id.clone());
                let details = search_request_details(SearchRequestLogInput {
                    query: &query,
                    requested_limit,
                    session: session_raw.as_deref(),
                    budget: budget.as_ref(),
                    score_threshold,
                    min_match_tokens,
                    metrics: om_metrics,
                    hint_policy,
                    typed_edge_enrichment,
                    result_count: Some(result.query_results.len()),
                });
                self.try_log_search_request(SearchRequestLogEvent {
                    request_id: &request_id,
                    started,
                    target_uri: target_raw.as_deref(),
                    trace_id: trace_id.as_deref(),
                    status: "ok",
                    error: None,
                    details,
                });
                Ok(result)
            }
            Err(err) => {
                let details = search_request_details(SearchRequestLogInput {
                    query: &query,
                    requested_limit,
                    session: session_raw.as_deref(),
                    budget: budget.as_ref(),
                    score_threshold,
                    min_match_tokens,
                    metrics: om_metrics,
                    hint_policy,
                    typed_edge_enrichment,
                    result_count: None,
                });
                self.try_log_search_request(SearchRequestLogEvent {
                    request_id: &request_id,
                    started,
                    target_uri: target_raw.as_deref(),
                    trace_id: None,
                    status: "error",
                    error: Some(&err),
                    details,
                });
                Err(err)
            }
        }
    }

    pub(crate) fn fetch_session_om_state(
        &self,
        session_id: &str,
    ) -> Result<Option<OmHintReadStateV1>> {
        self.fetch_session_om_state_with_enabled(session_id, self.om_snapshot_hint_reader_enabled())
    }

    pub(crate) fn fetch_om_state_by_scope_key(
        &self,
        scope_key: &str,
        preferred_thread_id: Option<&str>,
    ) -> Result<Option<OmHintReadStateV1>> {
        self.fetch_om_state_by_scope_key_with_enabled(
            scope_key,
            preferred_thread_id,
            self.om_snapshot_hint_reader_enabled(),
        )
    }

    pub(crate) fn fetch_om_state_by_scope_key_with_enabled(
        &self,
        scope_key: &str,
        preferred_thread_id: Option<&str>,
        om_enabled: bool,
    ) -> Result<Option<OmHintReadStateV1>> {
        let snapshot = self.fetch_om_hint_snapshot_by_scope_key_with_enabled(
            scope_key,
            preferred_thread_id,
            om_enabled,
        )?;
        Ok(snapshot.map(|snapshot| snapshot.to_read_state(self.config.search.om_hint_bounds)))
    }

    pub(crate) fn fetch_session_om_state_with_enabled(
        &self,
        session_id: &str,
        om_enabled: bool,
    ) -> Result<Option<OmHintReadStateV1>> {
        let snapshot = self.fetch_session_om_hint_snapshot_with_enabled(session_id, om_enabled)?;
        Ok(snapshot.map(|snapshot| snapshot.to_read_state(self.config.search.om_hint_bounds)))
    }

    fn fetch_om_hint_snapshot_by_scope_key_with_enabled(
        &self,
        scope_key: &str,
        preferred_thread_id: Option<&str>,
        om_enabled: bool,
    ) -> Result<Option<OmHintSnapshotV2>> {
        if !om_enabled {
            return Ok(None);
        }

        // Prioritize in-memory index
        let record = {
            let index = self
                .index
                .read()
                .map_err(|_| AxiomError::lock_poisoned("index"))?;
            index.get_om_record(scope_key).cloned()
        };

        let record = if let Some(record) = record {
            record
        } else {
            // Fallback to state store if not in index (should not happen if synchronized)
            let Some(record) = self.state.get_om_record_by_scope_key(scope_key)? else {
                return Ok(None);
            };
            record
        };

        let preferred_thread_id = preferred_thread_id.map(|thread_id| {
            resolve_canonical_thread_id(
                record.scope,
                &record.scope_key,
                Some(thread_id),
                record.session_id.as_deref(),
                thread_id,
            )
        });
        Ok(Some(self.build_om_hint_snapshot_from_record(
            &record,
            preferred_thread_id.as_deref(),
        )?))
    }

    fn fetch_session_om_hint_snapshot_with_enabled(
        &self,
        session_id: &str,
        om_enabled: bool,
    ) -> Result<Option<OmHintSnapshotV2>> {
        if !om_enabled {
            return Ok(None);
        }
        let scope_binding =
            resolve_om_scope_binding_for_session_with_config(session_id, &self.config.om.scope)?;
        let record = if let Some(record) = self
            .state
            .get_om_record_by_scope_key(&scope_binding.scope_key)?
        {
            record
        } else if scope_binding.scope == OmScope::Session {
            let mut resolved = None;
            let fallback_scope_keys = self.state.list_om_scope_keys_for_session(
                session_id,
                DEFAULT_OM_SCOPE_LOOKUP_FALLBACK_LIMIT,
            )?;
            for fallback_scope_key in fallback_scope_keys {
                if fallback_scope_key == scope_binding.scope_key {
                    continue;
                }
                if let Some(candidate) =
                    self.state.get_om_record_by_scope_key(&fallback_scope_key)?
                {
                    resolved = Some(candidate);
                    break;
                }
            }
            let Some(record) = resolved else {
                return Ok(None);
            };
            record
        } else {
            return Ok(None);
        };

        let preferred_thread_id = match record.scope {
            OmScope::Session => None,
            OmScope::Thread | OmScope::Resource => Some(resolve_canonical_thread_id(
                record.scope,
                &record.scope_key,
                scope_binding
                    .thread_id
                    .as_deref()
                    .or(record.thread_id.as_deref()),
                Some(session_id),
                session_id,
            )),
        };
        Ok(Some(self.build_om_hint_snapshot_from_record(
            &record,
            preferred_thread_id.as_deref(),
        )?))
    }

    fn build_om_hint_snapshot_from_record(
        &self,
        record: &crate::om::OmRecord,
        preferred_thread_id: Option<&str>,
    ) -> Result<OmHintSnapshotV2> {
        let preferred_thread_id = preferred_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let continuation_state = self
            .state
            .resolve_om_continuation_state(&record.scope_key, preferred_thread_id)?;
        let continuation_current_task = continuation_state
            .as_ref()
            .and_then(|state| state.current_task.as_deref());
        let continuation_suggested_response = continuation_state
            .as_ref()
            .and_then(|state| state.suggested_response.as_deref());
        let (current_task, suggested_response) = self.resolve_snapshot_continuation_fields(
            record,
            preferred_thread_id,
            continuation_current_task,
            continuation_suggested_response,
        )?;
        let active_entries = self.state.list_om_active_entries(&record.scope_key)?;
        let snapshot_entry_inputs =
            self.collect_snapshot_entry_inputs(record, preferred_thread_id, &active_entries)?;
        let materialized_at = record.updated_at.to_rfc3339();
        let continuation_for_snapshot = build_snapshot_continuation_state(
            record,
            snapshot_entry_inputs.fallback_thread_id.as_str(),
            &current_task,
            &suggested_response,
            &materialized_at,
        );
        let hint_policy_v2 = build_om_hint_policy_v2(self.config.search.om_hint_bounds);
        let snapshot = materialize_search_visible_snapshot(
            &record.scope_key,
            &build_snapshot_activated_entries(&record.scope_key, &active_entries),
            &snapshot_entry_inputs.buffered_entries,
            continuation_for_snapshot.as_ref(),
            &materialized_at,
            hint_policy_v2,
        );
        let visible_entry_selection = snapshot_visible_entry_selection(&snapshot.visible_entries);
        let search_visible_observations = compact_observation_text_for_hint(
            &snapshot_visible_observation_text(&snapshot.visible_entries),
            &snapshot_entry_inputs.reserved_high_texts,
        );

        Ok(OmHintSnapshotV2 {
            scope_key: record.scope_key.clone(),
            snapshot_version: snapshot.snapshot_version.clone(),
            materialized_at: snapshot.materialized_at_rfc3339.clone(),
            search_visible_observations,
            rendered_hint: snapshot.rendered_hint,
            current_task,
            suggested_response,
            activated_message_ids: record.last_activated_message_ids.clone(),
            buffered_chunk_ids: snapshot_entry_inputs.buffered_chunk_ids,
            selected_entry_ids: visible_entry_selection.selected_entry_ids,
            activated_visible_entry_ids: visible_entry_selection.activated_visible_entry_ids,
            buffered_visible_entry_ids: visible_entry_selection.buffered_visible_entry_ids,
            observation_tokens_active: record.observation_token_count,
            observer_trigger_count_total: record.observer_trigger_count_total,
            reflector_trigger_count_total: record.reflector_trigger_count_total,
            buffered_chunk_count: snapshot_entry_inputs.buffered_entries.len(),
            high_priority_selected_count: snapshot_entry_inputs.high_priority_selected_count,
        })
    }

    fn resolve_snapshot_continuation_fields(
        &self,
        record: &crate::om::OmRecord,
        preferred_thread_id: Option<&str>,
        continuation_current_task: Option<&str>,
        continuation_suggested_response: Option<&str>,
    ) -> Result<(Option<String>, Option<String>)> {
        let continuation_current_task = continuation_current_task.and_then(non_empty_trimmed);
        let continuation_suggested_response =
            continuation_suggested_response.and_then(non_empty_trimmed);
        let has_continuation =
            continuation_current_task.is_some() || continuation_suggested_response.is_some();
        if has_continuation {
            return Ok((
                continuation_current_task.map(ToString::to_string),
                continuation_suggested_response.map(ToString::to_string),
            ));
        }
        if record.scope == OmScope::Session {
            return Ok((None, None));
        }
        let thread_states = self.state.list_om_thread_states(&record.scope_key)?;
        let selected_state = preferred_thread_id
            .and_then(|thread_id| {
                thread_states
                    .iter()
                    .find(|state| state.thread_id == thread_id)
            })
            .or_else(|| thread_states.first());
        let current_task = selected_state
            .and_then(|state| state.current_task.as_deref())
            .and_then(non_empty_trimmed)
            .map(ToString::to_string);
        let suggested_response = selected_state
            .and_then(|state| state.suggested_response.as_deref())
            .and_then(non_empty_trimmed)
            .map(ToString::to_string);
        Ok((current_task, suggested_response))
    }

    fn collect_snapshot_entry_inputs(
        &self,
        record: &crate::om::OmRecord,
        preferred_thread_id: Option<&str>,
        active_entries: &[OmActiveEntry],
    ) -> Result<SnapshotEntryInputs> {
        let (reserved_high_texts, reserved_high_ids) = select_reserved_high_priority_entries(
            active_entries,
            preferred_thread_id,
            OM_HINT_COMPACTION_RESERVED_HIGH_LIMIT,
        );
        let buffered_chunks = self.state.list_om_observation_chunks(&record.id)?;
        let fallback_thread_id = preferred_thread_id
            .or(record.thread_id.as_deref())
            .or(record.session_id.as_deref())
            .unwrap_or(record.scope_key.as_str())
            .to_string();
        let (buffered_entries, buffered_chunk_ids) = build_snapshot_buffered_entries(
            &record.scope_key,
            fallback_thread_id.as_str(),
            &buffered_chunks,
        );
        Ok(SnapshotEntryInputs {
            fallback_thread_id,
            reserved_high_texts,
            high_priority_selected_count: reserved_high_ids.len(),
            buffered_entries,
            buffered_chunk_ids,
        })
    }
}

fn build_om_hint_policy_v2(bounds: OmHintBounds) -> OmHintPolicyV2 {
    OmHintPolicyV2 {
        max_lines: bounds.max_lines,
        max_chars: bounds.max_chars,
        reserve_current_task_line: true,
        reserve_suggested_response_line: true,
        high_priority_slots: OM_HINT_COMPACTION_RESERVED_HIGH_LIMIT,
        include_buffered_entries: true,
    }
}

fn filter_recent_messages_by_ids(
    messages: &[Message],
    activated_message_ids: &[String],
) -> Vec<Message> {
    if activated_message_ids.is_empty() {
        return messages.to_vec();
    }
    let ids = activated_message_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    messages
        .iter()
        .filter(|message| !ids.contains(message.id.as_str()))
        .cloned()
        .collect::<Vec<_>>()
}

fn select_reserved_high_priority_entries(
    entries: &[OmActiveEntry],
    preferred_thread_id: Option<&str>,
    limit: usize,
) -> (Vec<String>, Vec<String>) {
    if limit == 0 {
        return (Vec::new(), Vec::new());
    }
    let preferred_thread_id = preferred_thread_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut high_entries = entries
        .iter()
        .filter(|entry| is_high_priority(entry.priority.as_str()))
        .map(|entry| {
            let preferred = preferred_thread_id
                .is_some_and(|thread_id| thread_id == entry.canonical_thread_id.as_str());
            (
                preferred,
                entry.created_at,
                entry.entry_id.as_str(),
                entry.text.as_str(),
            )
        })
        .collect::<Vec<_>>();

    high_entries.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(b.2))
    });

    let selected = high_entries.into_iter().take(limit).collect::<Vec<_>>();
    let mut texts = Vec::<String>::with_capacity(selected.len());
    let mut ids = Vec::<String>::with_capacity(selected.len());
    for (_, _, entry_id, text) in selected {
        if let Some(normalized) = non_empty_trimmed(text) {
            texts.push(normalized.to_string());
            ids.push(entry_id.to_string());
        }
    }
    (texts, ids)
}

fn compact_observation_text_for_hint(base: &str, reserved_high_texts: &[String]) -> String {
    let mut sections = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    for text in reserved_high_texts {
        if let Some(normalized) = non_empty_trimmed(text) {
            let value = normalized.to_string();
            if seen.insert(value.clone()) {
                sections.push(value);
            }
        }
    }

    if let Some(base_normalized) = non_empty_trimmed(base) {
        let base_value = base_normalized.to_string();
        if seen.insert(base_value.clone()) {
            sections.push(base_value);
        }
    }

    sections.join("\n\n")
}

fn is_high_priority(priority: &str) -> bool {
    priority.eq_ignore_ascii_case("high")
}

fn non_empty_trimmed(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn parse_optional_target_uri(target_uri: Option<&str>) -> Result<Option<AxiomUri>> {
    target_uri.map(AxiomUri::parse).transpose()
}

fn build_search_options(input: SearchOptionsInput) -> SearchOptions {
    let SearchOptionsInput {
        query,
        target_uri,
        session,
        session_hints,
        budget,
        requested_limit,
        score_threshold,
        min_match_tokens,
        filter,
        request_type,
    } = input;
    SearchOptions {
        query,
        target_uri,
        session,
        session_hints,
        budget,
        limit: requested_limit,
        score_threshold,
        min_match_tokens,
        filter: metadata_filter_to_search_filter(filter),
        request_type: request_type.to_string(),
    }
}

fn saturating_usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn collect_recent_hints(messages: &[Message], limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    messages
        .iter()
        .rev()
        .take(limit)
        .map(|message| message.text.clone())
        .collect::<Vec<_>>()
}

fn merge_recent_and_om_hints(
    recent_hints: &[String],
    om_hint: Option<&str>,
    policy: OmHintPolicy,
) -> Vec<String> {
    if policy.total_hint_limit == 0 {
        return Vec::new();
    }

    if let Some(om_hint) = om_hint {
        let mut out = Vec::<String>::with_capacity(policy.total_hint_limit);
        let keep_recent_cap = policy.total_hint_limit.saturating_sub(1);
        let keep_recent = policy
            .keep_recent_with_om
            .min(keep_recent_cap)
            .min(recent_hints.len());
        out.extend(recent_hints.iter().take(keep_recent).cloned());
        out.push(om_hint.to_string());

        if out.len() < policy.total_hint_limit {
            let fill = policy.total_hint_limit.saturating_sub(out.len());
            out.extend(recent_hints.iter().skip(keep_recent).take(fill).cloned());
        }
        return out;
    }

    recent_hints
        .iter()
        .take(policy.total_hint_limit)
        .cloned()
        .collect::<Vec<_>>()
}

fn normalize_runtime_hints(
    runtime_hints: &[RuntimeHint],
    max_hints: usize,
    max_chars: usize,
) -> Vec<String> {
    if max_chars == 0 || max_hints == 0 {
        return Vec::new();
    }
    let mut out = Vec::<String>::with_capacity(max_hints.min(runtime_hints.len()));
    let mut seen = HashSet::<String>::new();
    for hint in runtime_hints {
        let Some(normalized) = normalize_hint_text(hint.text.as_str(), max_chars) else {
            continue;
        };
        if seen.insert(normalized.clone()) {
            out.push(normalized);
            if out.len() >= max_hints {
                break;
            }
        }
    }
    out
}

fn merge_runtime_om_recent_hints(
    runtime: &[String],
    om: Option<&str>,
    recent: &[String],
    policy: OmHintPolicy,
    max_chars: usize,
) -> Vec<String> {
    if policy.total_hint_limit == 0 {
        return Vec::new();
    }

    let mut out = Vec::<String>::with_capacity(policy.total_hint_limit);
    let mut seen = HashSet::<String>::new();
    let mut push_hint = |value: &str| {
        if out.len() >= policy.total_hint_limit {
            return;
        }
        let Some(normalized) = normalize_hint_text(value, max_chars) else {
            return;
        };
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    };

    if let Some(om_hint) = om {
        let keep_recent = policy
            .keep_recent_with_om
            .min(recent.len())
            .min(policy.total_hint_limit.saturating_sub(1));

        for hint in recent.iter().take(keep_recent) {
            push_hint(hint);
        }
        push_hint(om_hint);
        for hint in runtime {
            push_hint(hint);
        }
        for hint in recent.iter().skip(keep_recent) {
            push_hint(hint);
        }
        return out;
    }

    for hint in runtime {
        push_hint(hint);
    }
    for hint in recent {
        push_hint(hint);
    }
    out
}

fn normalize_hint_text(value: &str, max_chars: usize) -> Option<String> {
    collapse_and_clip_whitespace(value, max_chars)
}

fn collapse_and_clip_whitespace(value: &str, max_chars: usize) -> Option<String> {
    if max_chars == 0 {
        return None;
    }
    let mut out = String::new();
    let mut used = 0usize;
    let mut has_word = false;

    for word in value.split_whitespace() {
        if word.is_empty() {
            continue;
        }
        if has_word {
            // Keep at least one char budget for the next token; avoid trailing
            // whitespace-only clipping (e.g., "alpha ").
            if used.saturating_add(1) >= max_chars {
                break;
            }
            out.push(' ');
            used = used.saturating_add(1);
        }
        for ch in word.chars() {
            if used >= max_chars {
                break;
            }
            out.push(ch);
            used = used.saturating_add(1);
        }
        if used >= max_chars {
            break;
        }
        has_word = true;
    }

    if out.is_empty() { None } else { Some(out) }
}

fn estimate_hint_tokens(hints: &[String]) -> u32 {
    hints.iter().fold(0u32, |sum, hint| {
        sum.saturating_add(estimate_text_tokens(hint))
    })
}

fn validate_search_cutoff_options(
    score_threshold: Option<f32>,
    min_match_tokens: Option<usize>,
) -> Result<()> {
    if let Some(threshold) = score_threshold
        && (!threshold.is_finite() || !(0.0..=1.0).contains(&threshold))
    {
        return Err(AxiomError::Validation(format!(
            "score_threshold must be within [0.0, 1.0], got {threshold}"
        )));
    }
    if let Some(min_match_tokens) = min_match_tokens
        && min_match_tokens < 2
    {
        return Err(AxiomError::Validation(
            "min_match_tokens must be >= 2 when provided".to_string(),
        ));
    }
    Ok(())
}

fn bounded_om_hint_from_snapshot(
    search_visible_observations: &str,
    current_task: Option<&str>,
    suggested_response: Option<&str>,
    bounds: OmHintBounds,
) -> Option<String> {
    let continuation_hint =
        compose_continuation_hint(current_task, suggested_response, bounds.max_suggested_chars);
    merge_observation_hint_with_suggested_response(
        build_bounded_observation_hint(
            search_visible_observations,
            bounds.max_lines,
            bounds.max_chars,
        ),
        continuation_hint.as_deref(),
        bounds.max_suggested_chars,
    )
}

fn compose_continuation_hint(
    current_task: Option<&str>,
    suggested_response: Option<&str>,
    max_chars: usize,
) -> Option<String> {
    if max_chars == 0 {
        return None;
    }
    let current_task =
        current_task.and_then(|value| normalize_hint_text(value, max_chars.saturating_sub(6)));
    let suggested_response =
        suggested_response.and_then(|value| normalize_hint_text(value, max_chars));

    match (current_task, suggested_response) {
        (Some(task), Some(next)) => Some(format!("task: {task} | next: {next}")),
        (Some(task), None) => Some(format!("task: {task}")),
        (None, Some(next)) => Some(format!("next: {next}")),
        (None, None) => None,
    }
}

fn merge_observation_hint_with_suggested_response(
    observation_hint: Option<String>,
    continuation_hint: Option<&str>,
    max_suggested_chars: usize,
) -> Option<String> {
    if max_suggested_chars == 0 {
        return observation_hint;
    }
    let continuation = continuation_hint
        .and_then(|value| normalize_hint_text(value, max_suggested_chars))
        .map(|value| normalize_continuation_hint_prefix(value.as_str()));

    match (observation_hint, continuation) {
        (Some(base), Some(continuation)) => Some(format!("{base} | {continuation}")),
        (None, Some(continuation)) => Some(format!("om: {continuation}")),
        (Some(base), None) => Some(base),
        (None, None) => None,
    }
}

fn normalize_continuation_hint_prefix(value: &str) -> String {
    let normalized = value.trim();
    if normalized
        .strip_prefix("task:")
        .or_else(|| normalized.strip_prefix("next:"))
        .is_some()
    {
        normalized.to_string()
    } else {
        format!("next: {normalized}")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        SearchOptionsInput, build_search_options, infer_buffered_entry_priority,
        normalize_hint_text, parse_optional_target_uri, snapshot_visible_entry_selection,
        snapshot_visible_entry_source_key, validate_search_cutoff_options,
    };
    use crate::error::AxiomError;
    use crate::models::{MetadataFilter, SearchBudget};
    use crate::om::{OmObservationEntryV2, OmObservationOriginKind, OmObservationPriority};
    use crate::uri::AxiomUri;

    #[test]
    fn validate_search_cutoff_options_accepts_supported_values() {
        validate_search_cutoff_options(Some(0.0), Some(2)).expect("valid lower bounds");
        validate_search_cutoff_options(Some(1.0), Some(16)).expect("valid upper bounds");
        validate_search_cutoff_options(None, None).expect("missing options remain valid");
    }

    #[test]
    fn validate_search_cutoff_options_rejects_invalid_threshold() {
        let err = validate_search_cutoff_options(Some(1.1), Some(2))
            .expect_err("threshold above 1.0 must fail");
        assert!(matches!(err, AxiomError::Validation(_)));
    }

    #[test]
    fn validate_search_cutoff_options_rejects_invalid_min_match_tokens() {
        let err = validate_search_cutoff_options(Some(0.5), Some(1))
            .expect_err("min_match_tokens below 2 must fail");
        assert!(matches!(err, AxiomError::Validation(_)));
    }

    #[test]
    fn normalize_hint_text_collapses_whitespace_and_clips_chars() {
        assert_eq!(
            normalize_hint_text("  alpha   beta\tgamma  ", 64).as_deref(),
            Some("alpha beta gamma")
        );
        assert_eq!(
            normalize_hint_text("alpha beta gamma", 5).as_deref(),
            Some("alpha")
        );
        assert_eq!(
            normalize_hint_text("alpha beta", 6).as_deref(),
            Some("alpha")
        );
    }

    #[test]
    fn normalize_hint_text_rejects_empty_or_zero_budget() {
        assert!(normalize_hint_text("   \n\t ", 16).is_none());
        assert!(normalize_hint_text("hint", 0).is_none());
    }

    #[test]
    fn parse_optional_target_uri_returns_none_when_missing() {
        let target = parse_optional_target_uri(None).expect("parse none");
        assert!(target.is_none());
    }

    #[test]
    fn build_search_options_maps_metadata_filter_without_side_effects() {
        let mut fields = HashMap::new();
        fields.insert("tags".to_string(), serde_json::json!(["auth", "oauth"]));
        fields.insert("mime".to_string(), serde_json::json!("text/markdown"));
        let filter = MetadataFilter { fields };

        let options = build_search_options(SearchOptionsInput {
            query: "oauth".to_string(),
            target_uri: Some(AxiomUri::parse("axiom://resources").expect("target parse")),
            session: Some("s-1".to_string()),
            session_hints: vec!["recent hint".to_string()],
            budget: Some(SearchBudget {
                max_ms: Some(100),
                max_nodes: Some(10),
                max_depth: Some(3),
            }),
            requested_limit: 5,
            score_threshold: Some(0.5),
            min_match_tokens: Some(2),
            filter: Some(filter),
            request_type: "search",
        });

        assert_eq!(options.query, "oauth");
        assert_eq!(options.request_type, "search");
        assert_eq!(options.limit, 5);
        assert_eq!(options.session.as_deref(), Some("s-1"));
        assert_eq!(
            options
                .target_uri
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("axiom://resources")
        );
        let resolved_filter = options.filter.expect("mapped filter");
        assert_eq!(
            resolved_filter.tags,
            vec!["auth".to_string(), "oauth".to_string()]
        );
        assert_eq!(resolved_filter.mime.as_deref(), Some("text/markdown"));
    }

    #[test]
    fn infer_buffered_entry_priority_preserves_high_signals() {
        assert_eq!(
            infer_buffered_entry_priority("priority:high patch queue replay"),
            crate::om::OmObservationPriority::High
        );
        assert_eq!(
            infer_buffered_entry_priority("[high] rollback risk"),
            crate::om::OmObservationPriority::High
        );
        assert_eq!(
            infer_buffered_entry_priority("high: fix stale hint"),
            crate::om::OmObservationPriority::High
        );
        assert_eq!(
            infer_buffered_entry_priority("normal buffered note"),
            crate::om::OmObservationPriority::Medium
        );
    }

    #[test]
    fn snapshot_visible_entry_source_key_maps_observation_and_buffered_to_same_chunk() {
        assert_eq!(
            snapshot_visible_entry_source_key("observation:chunk-7"),
            "chunk:chunk-7"
        );
        assert_eq!(
            snapshot_visible_entry_source_key("buffered:session:s1:chunk-7"),
            "chunk:chunk-7"
        );
        assert_eq!(
            snapshot_visible_entry_source_key("reflection:session:s1:42"),
            "entry:reflection:session:s1:42"
        );
    }

    #[test]
    fn snapshot_visible_entry_ids_dedupes_same_chunk_source_keeping_first_entry() {
        let entries = vec![
            OmObservationEntryV2 {
                entry_id: "observation:chunk-1".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::Medium,
                text: "a".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Observation,
                created_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
                superseded_by: None,
            },
            OmObservationEntryV2 {
                entry_id: "buffered:session:s1:chunk-1".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::Medium,
                text: "a-tail".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Chunk,
                created_at_rfc3339: "2026-03-04T00:00:01Z".to_string(),
                superseded_by: None,
            },
            OmObservationEntryV2 {
                entry_id: "reflection:session:s1:99".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::Medium,
                text: "r".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Reflection,
                created_at_rfc3339: "2026-03-04T00:00:02Z".to_string(),
                superseded_by: None,
            },
        ];

        let selection = snapshot_visible_entry_selection(&entries);
        assert_eq!(
            selection.selected_entry_ids,
            vec![
                "observation:chunk-1".to_string(),
                "reflection:session:s1:99".to_string(),
            ]
        );
        assert_eq!(
            selection.activated_visible_entry_ids,
            vec![
                "observation:chunk-1".to_string(),
                "reflection:session:s1:99".to_string(),
            ]
        );
        assert!(selection.buffered_visible_entry_ids.is_empty());
    }

    #[test]
    fn snapshot_visible_entry_selection_prefers_activated_over_buffered_for_same_source() {
        let entries = vec![
            OmObservationEntryV2 {
                entry_id: "buffered:session:s1:chunk-7".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::Medium,
                text: "buffered".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Chunk,
                created_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
                superseded_by: None,
            },
            OmObservationEntryV2 {
                entry_id: "observation:chunk-7".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::Medium,
                text: "active".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Observation,
                created_at_rfc3339: "2026-03-04T00:00:01Z".to_string(),
                superseded_by: None,
            },
            OmObservationEntryV2 {
                entry_id: "reflection:session:s1:12".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::High,
                text: "reflection".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Reflection,
                created_at_rfc3339: "2026-03-04T00:00:02Z".to_string(),
                superseded_by: None,
            },
        ];
        let selection = snapshot_visible_entry_selection(&entries);
        assert_eq!(
            selection.selected_entry_ids,
            vec![
                "observation:chunk-7".to_string(),
                "reflection:session:s1:12".to_string(),
            ]
        );
        assert!(selection.buffered_visible_entry_ids.is_empty());
        assert_eq!(
            selection.activated_visible_entry_ids,
            vec![
                "observation:chunk-7".to_string(),
                "reflection:session:s1:12".to_string()
            ]
        );
    }

    #[test]
    fn snapshot_visible_entry_selection_keeps_buffered_when_no_activated_pair_exists() {
        let entries = vec![
            OmObservationEntryV2 {
                entry_id: "buffered:session:s1:chunk-11".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::Medium,
                text: "buffered only".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Chunk,
                created_at_rfc3339: "2026-03-04T00:00:00Z".to_string(),
                superseded_by: None,
            },
            OmObservationEntryV2 {
                entry_id: "reflection:session:s1:13".to_string(),
                scope_key: "session:s1".to_string(),
                thread_id: "s1".to_string(),
                priority: OmObservationPriority::Medium,
                text: "reflection".to_string(),
                source_message_ids: Vec::new(),
                origin_kind: OmObservationOriginKind::Reflection,
                created_at_rfc3339: "2026-03-04T00:00:01Z".to_string(),
                superseded_by: None,
            },
        ];

        let selection = snapshot_visible_entry_selection(&entries);
        assert_eq!(
            selection.selected_entry_ids,
            vec![
                "buffered:session:s1:chunk-11".to_string(),
                "reflection:session:s1:13".to_string(),
            ]
        );
        assert_eq!(
            selection.buffered_visible_entry_ids,
            vec!["buffered:session:s1:chunk-11".to_string()]
        );
        assert_eq!(
            selection.activated_visible_entry_ids,
            vec!["reflection:session:s1:13".to_string()]
        );
    }
}

#[cfg(test)]
mod backend_tests;
