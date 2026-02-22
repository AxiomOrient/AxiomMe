use serde_json::json;

use crate::models::{
    ContextHit, FindResult, MetadataFilter, SearchBudget, SearchFilter, TracePoint,
};

pub(super) fn metadata_filter_to_search_filter(
    filter: Option<MetadataFilter>,
) -> Option<SearchFilter> {
    let filter = filter?;
    Some(SearchFilter {
        tags: filter
            .fields
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(ToString::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        mime: filter
            .fields
            .get("mime")
            .and_then(|v| v.as_str().map(ToString::to_string)),
    })
}

pub(super) fn normalize_budget(budget: Option<SearchBudget>) -> Option<SearchBudget> {
    let budget = budget?;
    if budget.max_ms.is_none() && budget.max_nodes.is_none() && budget.max_depth.is_none() {
        return None;
    }
    Some(budget)
}

pub(super) fn budget_to_json(budget: Option<&SearchBudget>) -> serde_json::Value {
    budget.map_or(serde_json::Value::Null, |budget| {
        json!({
            "max_ms": budget.max_ms,
            "max_nodes": budget.max_nodes,
            "max_depth": budget.max_depth,
        })
    })
}

pub(super) fn sync_trace_final_topk(result: &mut FindResult) {
    let Some(trace) = result.trace.as_mut() else {
        return;
    };
    trace.final_topk = result
        .query_results
        .iter()
        .map(|hit| TracePoint {
            uri: hit.uri.clone(),
            score: hit.score,
        })
        .collect();
}

pub(super) fn split_hits(
    hits: &[ContextHit],
) -> (Vec<ContextHit>, Vec<ContextHit>, Vec<ContextHit>) {
    let mut memories = Vec::new();
    let mut resources = Vec::new();
    let mut skills = Vec::new();

    for hit in hits {
        if hit.uri.starts_with("axiom://user/memories")
            || hit.uri.starts_with("axiom://agent/memories")
        {
            memories.push(hit.clone());
        } else if hit.uri.starts_with("axiom://agent/skills") {
            skills.push(hit.clone());
        } else {
            resources.push(hit.clone());
        }
    }

    (memories, resources, skills)
}

pub(super) fn append_query_plan_note(result: &mut FindResult, note: &str) {
    result.query_plan.notes.push(note.to_string());
}

pub(super) fn annotate_trace_relation_metrics(result: &mut FindResult) {
    let Some(trace) = result.trace.as_mut() else {
        return;
    };
    let relation_enriched_hits = result
        .query_results
        .iter()
        .filter(|hit| !hit.relations.is_empty())
        .count();
    let relation_enriched_links = result
        .query_results
        .iter()
        .map(|hit| hit.relations.len())
        .sum();
    trace.metrics.relation_enriched_hits = relation_enriched_hits;
    trace.metrics.relation_enriched_links = relation_enriched_links;
}
