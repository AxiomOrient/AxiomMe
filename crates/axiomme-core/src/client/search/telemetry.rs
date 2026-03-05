use std::time::Instant;

use crate::config::{OmHintPolicy, RETRIEVAL_BACKEND_MEMORY, RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY};
use crate::error::AxiomError;
use crate::models::{FindResult, SearchBudget};

use super::OmSearchMetrics;
use super::result::{append_query_plan_note, budget_to_json};

#[derive(Debug, Clone)]
pub(super) struct SearchRequestLogInput<'a> {
    pub(super) query: &'a str,
    pub(super) requested_limit: usize,
    pub(super) session: Option<&'a str>,
    pub(super) budget: Option<&'a SearchBudget>,
    pub(super) score_threshold: Option<f32>,
    pub(super) min_match_tokens: Option<usize>,
    pub(super) metrics: OmSearchMetrics,
    pub(super) hint_policy: OmHintPolicy,
    pub(super) typed_edge_enrichment: bool,
    pub(super) result_count: Option<usize>,
}

#[derive(Debug)]
pub(super) struct SearchRequestLogEvent<'a> {
    pub(super) request_id: &'a str,
    pub(super) started: Instant,
    pub(super) target_uri: Option<&'a str>,
    pub(super) trace_id: Option<&'a str>,
    pub(super) status: &'static str,
    pub(super) error: Option<&'a AxiomError>,
    pub(super) details: serde_json::Value,
}

pub(super) fn annotate_om_query_plan_visibility(
    result: &mut FindResult,
    metrics: &OmSearchMetrics,
    policy: OmHintPolicy,
) {
    if metrics.observer_trigger_count == 0
        && metrics.reflector_trigger_count == 0
        && metrics.session_recent_hint_count == 0
        && metrics.session_hint_count_final == 0
        && metrics.om_filtered_message_count == 0
        && !metrics.om_hint_reader_snapshot_v2
    {
        return;
    }
    if metrics.om_hint_reader_snapshot_v2 {
        append_query_plan_note(result, "om_hint_reader:snapshot_v2");
        append_query_plan_note(
            result,
            &format!(
                "om_snapshot_buffered_chunks:{}",
                metrics.om_snapshot_buffered_chunk_count
            ),
        );
        if metrics.om_hint_compaction_priority_v2 {
            append_query_plan_note(result, "om_hint_compaction:priority_v2");
            append_query_plan_note(
                result,
                &format!(
                    "om_hint_high_priority_selected:{}",
                    metrics.om_hint_high_priority_selected_count
                ),
            );
            if !metrics.om_snapshot_reserved_high_entry_ids.is_empty() {
                append_query_plan_note(
                    result,
                    &format!(
                        "om_snapshot_reserved_high_entries:{}",
                        metrics.om_snapshot_reserved_high_entry_ids.join(",")
                    ),
                );
            }
            append_query_plan_note(
                result,
                &format!(
                    "om_hint_current_task_reserved:{}",
                    u8::from(metrics.om_hint_current_task_reserved)
                ),
            );
            if !metrics.om_snapshot_visible_entry_ids.is_empty() {
                append_query_plan_note(
                    result,
                    &format!(
                        "om_snapshot_visible_entries:{}",
                        metrics.om_snapshot_visible_entry_ids.join(",")
                    ),
                );
            }
            if !metrics.om_snapshot_visible_activated_entry_ids.is_empty() {
                append_query_plan_note(
                    result,
                    &format!(
                        "om_snapshot_visible_activated_entries:{}",
                        metrics.om_snapshot_visible_activated_entry_ids.join(",")
                    ),
                );
            }
            if !metrics.om_snapshot_visible_buffered_entry_ids.is_empty() {
                append_query_plan_note(
                    result,
                    &format!(
                        "om_snapshot_visible_buffered_entries:{}",
                        metrics.om_snapshot_visible_buffered_entry_ids.join(",")
                    ),
                );
            }
        }
        if !metrics.om_snapshot_buffered_chunk_ids.is_empty() {
            append_query_plan_note(
                result,
                &format!(
                    "om_snapshot_buffered_chunk_ids:{}",
                    metrics.om_snapshot_buffered_chunk_ids.join(",")
                ),
            );
        }
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

pub(super) fn search_request_details(input: SearchRequestLogInput<'_>) -> serde_json::Value {
    let SearchRequestLogInput {
        query,
        requested_limit,
        session,
        budget,
        score_threshold,
        min_match_tokens,
        metrics,
        hint_policy,
        typed_edge_enrichment,
        result_count,
    } = input;
    let mut details = serde_json::json!({
        "query": query,
        "limit": requested_limit,
        "session": session,
        "budget": budget_to_json(budget),
        "score_threshold": score_threshold,
        "min_match_tokens": min_match_tokens,
        "retrieval_backend": RETRIEVAL_BACKEND_MEMORY,
        "retrieval_backend_policy": RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY,
        "context_tokens_before_om": metrics.context_tokens_before_om,
        "context_tokens_after_om": metrics.context_tokens_after_om,
        "observation_tokens_active": metrics.observation_tokens_active,
        "observer_trigger_count": metrics.observer_trigger_count,
        "reflector_trigger_count": metrics.reflector_trigger_count,
        "om_hint_applied": metrics.om_hint_applied,
        "om_hint_reader": if metrics.om_hint_reader_snapshot_v2 { "snapshot_v2" } else { "none" },
        "om_snapshot_buffered_chunk_count": metrics.om_snapshot_buffered_chunk_count,
        "om_snapshot_buffered_chunk_ids": metrics.om_snapshot_buffered_chunk_ids,
        "om_hint_compaction": if metrics.om_hint_compaction_priority_v2 { "priority_v2" } else { "none" },
        "om_hint_high_priority_selected_count": metrics.om_hint_high_priority_selected_count,
        "om_snapshot_reserved_high_entry_ids": metrics.om_snapshot_reserved_high_entry_ids,
        "om_hint_reserved_high_entry_ids": metrics.om_snapshot_reserved_high_entry_ids,
        "om_snapshot_visible_entry_ids": metrics.om_snapshot_visible_entry_ids,
        "om_hint_selected_entry_ids": metrics.om_snapshot_visible_entry_ids,
        "om_snapshot_visible_activated_entry_ids": metrics.om_snapshot_visible_activated_entry_ids,
        "om_snapshot_visible_buffered_entry_ids": metrics.om_snapshot_visible_buffered_entry_ids,
        "om_hint_current_task_reserved": metrics.om_hint_current_task_reserved,
        "session_recent_hint_count": metrics.session_recent_hint_count,
        "session_hint_count_final": metrics.session_hint_count_final,
        "om_filtered_message_count": metrics.om_filtered_message_count,
        "om_hint_policy": hint_policy_to_json(hint_policy),
        "typed_edge_enrichment": typed_edge_enrichment,
    });
    if let Some(result_count) = result_count {
        details["result_count"] = serde_json::json!(result_count);
    }
    details
}
