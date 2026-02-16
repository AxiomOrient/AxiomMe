use std::collections::HashSet;
use std::time::Instant;

use chrono::Utc;

use crate::config::{OmHintBounds, OmHintPolicy, RetrievalBackend};
use crate::context_ops::validate_filter;
use crate::error::{AxiomError, Result};
use crate::llm_io::estimate_text_tokens;
use crate::models::{
    FindResult, Message, MetadataFilter, RequestLogEntry, SearchBudget, SearchOptions,
    SearchRequest,
};
use crate::om::{OmScope, build_bounded_observation_hint};
use crate::om_bridge::OmHintReadStateV1;
use crate::session::resolve_om_scope_binding_for_session_with_config;
use crate::uri::AxiomUri;

use super::AxiomMe;

mod backend;
mod reranker;
mod result;

use result::{
    annotate_trace_relation_metrics, append_query_plan_note, budget_to_json,
    metadata_filter_to_search_filter, normalize_budget,
};

const DEFAULT_OM_SCOPE_LOOKUP_FALLBACK_LIMIT: usize = 4;

#[derive(Debug, Clone, Copy, Default)]
struct OmSearchMetrics {
    context_tokens_before_om: u32,
    context_tokens_after_om: u32,
    observation_tokens_active: u32,
    observer_trigger_count: u32,
    reflector_trigger_count: u32,
    om_hint_applied: bool,
    session_recent_hint_count: u32,
    session_hint_count_final: u32,
    om_filtered_message_count: u32,
}

impl AxiomMe {
    #[must_use]
    pub fn search_requires_runtime_prepare(&self) -> bool {
        matches!(
            self.config.search.retrieval_backend,
            RetrievalBackend::Memory
        )
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

        let output = (|| -> Result<FindResult> {
            validate_filter(filter.as_ref())?;
            validate_search_cutoff_options(score_threshold, None)?;
            let target = target_uri.map(AxiomUri::parse).transpose()?;
            let options = SearchOptions {
                query: query.to_string(),
                target_uri: target,
                session: None,
                session_hints: Vec::new(),
                budget: budget.clone(),
                limit: requested_limit,
                score_threshold,
                min_match_tokens: None,
                filter: metadata_filter_to_search_filter(filter),
                request_type: "find".to_string(),
            };

            let mut result = self.run_retrieval_with_backend(&options)?;
            self.enrich_find_result_relations(&mut result, 5)?;
            annotate_trace_relation_metrics(&mut result);
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
        })
    }

    fn build_search_session_hints(
        &self,
        session_id: &str,
        query: &str,
        hint_policy: OmHintPolicy,
    ) -> Result<(Vec<String>, OmSearchMetrics)> {
        let mut metrics = OmSearchMetrics::default();
        let ctx = self.session(Some(session_id)).get_context_for_search(
            query,
            hint_policy.context_max_archives,
            hint_policy.context_max_messages,
        )?;
        let om_state = self.fetch_session_om_state(session_id)?;
        let pre_om_recent_hints =
            collect_recent_hints(&ctx.recent_messages, hint_policy.recent_hint_limit);
        let pre_om_hints = merge_recent_and_om_hints(&pre_om_recent_hints, None, hint_policy);
        let filtered_recent_messages =
            if let Some(state) = om_state.as_ref().filter(|state| state.hint.is_some()) {
                filter_recent_messages_by_ids(&ctx.recent_messages, &state.activated_message_ids)
            } else {
                ctx.recent_messages.clone()
            };
        let recent_hints =
            collect_recent_hints(&filtered_recent_messages, hint_policy.recent_hint_limit);
        let merged_hints = merge_recent_and_om_hints(
            &recent_hints,
            om_state.as_ref().and_then(|state| state.hint.as_deref()),
            hint_policy,
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
        metrics.om_filtered_message_count = saturating_usize_to_u32(
            ctx.recent_messages
                .len()
                .saturating_sub(filtered_recent_messages.len()),
        );
        metrics.context_tokens_after_om = estimate_hint_tokens(&merged_hints);

        Ok((merged_hints, metrics))
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
        } = request;
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let target_raw = target_uri.clone();
        let session_raw = session.clone();
        let requested_limit = limit.unwrap_or(10);
        let budget = normalize_budget(budget);
        let hint_policy = self.config.search.om_hint_policy;
        let mut om_metrics = OmSearchMetrics::default();

        let output = (|| -> Result<FindResult> {
            validate_filter(filter.as_ref())?;
            validate_search_cutoff_options(score_threshold, min_match_tokens)?;
            let target = target_uri.as_deref().map(AxiomUri::parse).transpose()?;

            let session_hints = if let Some(session_id) = session.as_deref() {
                let (hints, metrics) =
                    self.build_search_session_hints(session_id, &query, hint_policy)?;
                om_metrics = metrics;
                hints
            } else {
                Vec::new()
            };

            let options = SearchOptions {
                query: query.clone(),
                target_uri: target,
                session: session.clone(),
                session_hints,
                budget: budget.clone(),
                limit: requested_limit,
                score_threshold,
                min_match_tokens,
                filter: metadata_filter_to_search_filter(filter),
                request_type: "search".to_string(),
            };

            let mut result = self.run_retrieval_with_backend(&options)?;
            self.enrich_find_result_relations(&mut result, 5)?;
            annotate_trace_relation_metrics(&mut result);
            annotate_om_query_plan_visibility(&mut result, &om_metrics, hint_policy);
            self.persist_trace_result(&mut result)?;
            Ok(result)
        })();

        match output {
            Ok(result) => {
                let trace_id = result.trace.as_ref().map(|x| x.trace_id.clone());
                let details = search_request_details(
                    &query,
                    requested_limit,
                    session_raw.as_deref(),
                    budget.as_ref(),
                    score_threshold,
                    min_match_tokens,
                    om_metrics,
                    hint_policy,
                    Some(result.query_results.len()),
                );
                self.try_log_request(&RequestLogEntry {
                    request_id,
                    operation: "search".to_string(),
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
                let details = search_request_details(
                    &query,
                    requested_limit,
                    session_raw.as_deref(),
                    budget.as_ref(),
                    score_threshold,
                    min_match_tokens,
                    om_metrics,
                    hint_policy,
                    None,
                );
                self.try_log_request(&RequestLogEntry {
                    request_id,
                    operation: "search".to_string(),
                    status: "error".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id: None,
                    target_uri: target_raw,
                    error_code: Some(err.code().to_string()),
                    error_message: Some(err.to_string()),
                    details: Some(details),
                });
                Err(err)
            }
        }
    }

    fn run_retrieval_with_backend(&self, options: &SearchOptions) -> Result<FindResult> {
        let backend = self.config.search.retrieval_backend;
        self.run_retrieval_with_backend_mode(options, backend)
    }

    pub(crate) fn fetch_session_om_state(
        &self,
        session_id: &str,
    ) -> Result<Option<OmHintReadStateV1>> {
        self.fetch_session_om_state_with_enabled(session_id, self.config.om.enabled)
    }

    pub(crate) fn fetch_om_state_by_scope_key(
        &self,
        scope_key: &str,
        preferred_thread_id: Option<&str>,
    ) -> Result<Option<OmHintReadStateV1>> {
        self.fetch_om_state_by_scope_key_with_enabled(
            scope_key,
            preferred_thread_id,
            self.config.om.enabled,
        )
    }

    pub(crate) fn fetch_om_state_by_scope_key_with_enabled(
        &self,
        scope_key: &str,
        preferred_thread_id: Option<&str>,
        om_enabled: bool,
    ) -> Result<Option<OmHintReadStateV1>> {
        if !om_enabled {
            return Ok(None);
        }
        let Some(record) = self.state.get_om_record_by_scope_key(scope_key)? else {
            return Ok(None);
        };
        Ok(Some(self.build_om_hint_state_from_record(
            &record,
            preferred_thread_id,
        )?))
    }

    pub(crate) fn fetch_session_om_state_with_enabled(
        &self,
        session_id: &str,
        om_enabled: bool,
    ) -> Result<Option<OmHintReadStateV1>> {
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
            OmScope::Thread => Some(
                scope_binding
                    .thread_id
                    .as_deref()
                    .or(record.thread_id.as_deref())
                    .unwrap_or(session_id),
            ),
            OmScope::Resource => Some(scope_binding.thread_id.as_deref().unwrap_or(session_id)),
        };
        Ok(Some(self.build_om_hint_state_from_record(
            &record,
            preferred_thread_id,
        )?))
    }

    fn build_om_hint_state_from_record(
        &self,
        record: &crate::om::OmRecord,
        preferred_thread_id: Option<&str>,
    ) -> Result<OmHintReadStateV1> {
        let preferred_thread_id = preferred_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let thread_state_suggested_response = if record.scope == OmScope::Session {
            None
        } else {
            let thread_states = self.state.list_om_thread_states(&record.scope_key)?;
            preferred_thread_id
                .and_then(|thread_id| {
                    thread_states
                        .iter()
                        .find(|state| state.thread_id == thread_id)
                        .and_then(|state| state.suggested_response.as_deref())
                })
                .or_else(|| {
                    thread_states
                        .iter()
                        .find_map(|state| state.suggested_response.as_deref())
                })
                .map(ToString::to_string)
        };

        Ok(OmHintReadStateV1 {
            scope_key: record.scope_key.clone(),
            hint: bounded_om_hint_from_record(
                &record.active_observations,
                thread_state_suggested_response
                    .as_deref()
                    .or(record.suggested_response.as_deref()),
                self.config.search.om_hint_bounds,
            ),
            activated_message_ids: record.last_activated_message_ids.clone(),
            observation_tokens_active: record.observation_token_count,
            observer_trigger_count_total: record.observer_trigger_count_total,
            reflector_trigger_count_total: record.reflector_trigger_count_total,
        })
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

fn annotate_om_query_plan_visibility(
    result: &mut FindResult,
    metrics: &OmSearchMetrics,
    policy: OmHintPolicy,
) {
    if metrics.observer_trigger_count == 0
        && metrics.reflector_trigger_count == 0
        && metrics.session_recent_hint_count == 0
        && metrics.session_hint_count_final == 0
        && metrics.om_filtered_message_count == 0
    {
        return;
    }
    append_query_plan_note(
        result,
        &format!("om_hint_applied:{}", u8::from(metrics.om_hint_applied)),
    );
    append_query_plan_note(
        result,
        &format!("session_hints_final:{}", metrics.session_hint_count_final),
    );
    append_query_plan_note(
        result,
        &format!("observer_triggers:{}", metrics.observer_trigger_count),
    );
    append_query_plan_note(
        result,
        &format!("reflector_triggers:{}", metrics.reflector_trigger_count),
    );
    append_query_plan_note(
        result,
        &format!("om_filtered_messages:{}", metrics.om_filtered_message_count),
    );
    append_query_plan_note(
        result,
        &format!(
            "om_hint_policy:{}/{}/{}/{}",
            policy.recent_hint_limit,
            policy.total_hint_limit,
            policy.keep_recent_with_om,
            policy.context_max_messages
        ),
    );
}

fn hint_policy_to_json(policy: OmHintPolicy) -> serde_json::Value {
    serde_json::json!({
        "context_max_archives": policy.context_max_archives,
        "context_max_messages": policy.context_max_messages,
        "recent_hint_limit": policy.recent_hint_limit,
        "total_hint_limit": policy.total_hint_limit,
        "keep_recent_with_om": policy.keep_recent_with_om,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "request log payload builder keeps explicit fields to avoid hidden coupling"
)]
fn search_request_details(
    query: &str,
    requested_limit: usize,
    session: Option<&str>,
    budget: Option<&SearchBudget>,
    score_threshold: Option<f32>,
    min_match_tokens: Option<usize>,
    metrics: OmSearchMetrics,
    hint_policy: OmHintPolicy,
    result_count: Option<usize>,
) -> serde_json::Value {
    let mut details = serde_json::json!({
        "query": query,
        "limit": requested_limit,
        "session": session,
        "budget": budget_to_json(budget),
        "score_threshold": score_threshold,
        "min_match_tokens": min_match_tokens,
        "context_tokens_before_om": metrics.context_tokens_before_om,
        "context_tokens_after_om": metrics.context_tokens_after_om,
        "observation_tokens_active": metrics.observation_tokens_active,
        "observer_trigger_count": metrics.observer_trigger_count,
        "reflector_trigger_count": metrics.reflector_trigger_count,
        "om_hint_applied": metrics.om_hint_applied,
        "session_recent_hint_count": metrics.session_recent_hint_count,
        "session_hint_count_final": metrics.session_hint_count_final,
        "om_filtered_message_count": metrics.om_filtered_message_count,
        "om_hint_policy": hint_policy_to_json(hint_policy),
    });
    if let Some(result_count) = result_count {
        details["result_count"] = serde_json::json!(result_count);
    }
    details
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

fn bounded_om_hint_from_record(
    active_observations: &str,
    suggested_response: Option<&str>,
    bounds: OmHintBounds,
) -> Option<String> {
    merge_observation_hint_with_suggested_response(
        build_bounded_observation_hint(active_observations, bounds.max_lines, bounds.max_chars),
        suggested_response,
        bounds.max_suggested_chars,
    )
}

fn merge_observation_hint_with_suggested_response(
    observation_hint: Option<String>,
    suggested_response: Option<&str>,
    max_suggested_chars: usize,
) -> Option<String> {
    if max_suggested_chars == 0 {
        return observation_hint;
    }
    let suggested = suggested_response
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(max_suggested_chars).collect::<String>())
        .filter(|value| !value.is_empty());

    match (observation_hint, suggested) {
        (Some(base), Some(next)) => Some(format!("{base} | next: {next}")),
        (None, Some(next)) => Some(format!("om: next: {next}")),
        (Some(base), None) => Some(base),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::validate_search_cutoff_options;
    use crate::error::AxiomError;

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
}

#[cfg(test)]
mod backend_tests;
