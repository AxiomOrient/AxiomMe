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

pub(super) fn annotate_typed_edge_query_plan_visibility(result: &mut FindResult, enabled: bool) {
    if !enabled {
        return;
    }
    append_query_plan_note(result, "typed_edge_enrichment:1");
    let typed_edges = result
        .query_results
        .iter()
        .chain(result.memories.iter())
        .chain(result.resources.iter())
        .chain(result.skills.iter())
        .flat_map(|hit| hit.relations.iter())
        .filter(|relation| relation.relation_type.is_some())
        .count();
    append_query_plan_note(result, &format!("typed_edge_links:{typed_edges}"));
}

#[cfg(test)]
mod tests {
    use super::annotate_typed_edge_query_plan_visibility;
    use crate::models::{ContextHit, FindResult, QueryPlan, RelationSummary};

    fn hit_with_relation(relation_type: Option<&str>) -> ContextHit {
        ContextHit {
            uri: "axiom://resources/demo/a.md".to_string(),
            score: 0.9,
            abstract_text: "demo".to_string(),
            context_type: "resource".to_string(),
            relations: vec![RelationSummary {
                uri: "axiom://resources/demo/b.md".to_string(),
                reason: "depends".to_string(),
                relation_type: relation_type.map(ToString::to_string),
                source_object_type: None,
                target_object_type: None,
            }],
        }
    }

    #[test]
    fn typed_edge_query_plan_visibility_is_disabled_by_flag() {
        let mut result = FindResult {
            memories: Vec::new(),
            resources: Vec::new(),
            skills: Vec::new(),
            query_plan: QueryPlan::default(),
            query_results: vec![hit_with_relation(Some("depends_on"))],
            trace: None,
            trace_uri: None,
        };
        annotate_typed_edge_query_plan_visibility(&mut result, false);
        assert!(result.query_plan.notes.is_empty());
    }

    #[test]
    fn typed_edge_query_plan_visibility_reports_typed_link_count() {
        let mut result = FindResult {
            memories: vec![hit_with_relation(None)],
            resources: Vec::new(),
            skills: Vec::new(),
            query_plan: QueryPlan::default(),
            query_results: vec![hit_with_relation(Some("depends_on"))],
            trace: None,
            trace_uri: None,
        };
        annotate_typed_edge_query_plan_visibility(&mut result, true);
        assert!(
            result
                .query_plan
                .notes
                .iter()
                .any(|value| value == "typed_edge_enrichment:1")
        );
        assert!(
            result
                .query_plan
                .notes
                .iter()
                .any(|value| value == "typed_edge_links:1")
        );
    }
}
