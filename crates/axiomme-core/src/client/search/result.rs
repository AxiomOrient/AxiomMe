use serde_json::json;

use crate::models::{
    ContextHit, FindResult, MetadataFilter, QueryPlan, RetrievalTrace, SearchBudget, SearchFilter,
    SearchOptions, TracePoint, TraceStats, TypedQueryPlan,
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

pub(super) fn build_sqlite_result(
    options: &SearchOptions,
    hits: Vec<ContextHit>,
    stop_reason: String,
    latency_ms: u128,
) -> FindResult {
    let mut notes = vec!["sqlite_fts".to_string()];
    if options.filter.is_some() {
        notes.push("filter".to_string());
    }
    if !options.session_hints.is_empty() {
        let session_hint_count = options.session_hints.len();
        notes.push(format!("session_hints:{session_hint_count}"));
        let om_hint_count = options
            .session_hints
            .iter()
            .filter(|hint| hint.trim_start().to_ascii_lowercase().starts_with("om:"))
            .count();
        notes.push(format!("session_om_hints:{om_hint_count}"));
    }
    if let Some(budget) = options.budget.as_ref() {
        if let Some(max_ms) = budget.max_ms {
            notes.push(format!("budget_ms:{max_ms}"));
        }
        if let Some(max_nodes) = budget.max_nodes {
            notes.push(format!("budget_nodes:{max_nodes}"));
        }
        if let Some(max_depth) = budget.max_depth {
            notes.push(format!("budget_depth:{max_depth}"));
        }
    }

    let scopes = options
        .target_uri
        .as_ref()
        .map(|target| vec![target.scope().as_str().to_string()])
        .map_or_else(
            || {
                vec![
                    "resources".to_string(),
                    "user".to_string(),
                    "agent".to_string(),
                    "session".to_string(),
                ]
            },
            std::convert::identity,
        );

    let typed_queries = vec![TypedQueryPlan {
        kind: "sqlite_fts".to_string(),
        query: options.query.clone(),
        scopes: scopes.clone(),
        priority: 1,
    }];
    let typed_queries = if options.session_hints.is_empty() {
        typed_queries
    } else {
        let mut out = typed_queries;
        out.push(TypedQueryPlan {
            kind: "session_recent".to_string(),
            query: options.session_hints.join(" "),
            scopes: vec!["session".to_string()],
            priority: 2,
        });
        out
    };

    let final_topk = hits
        .iter()
        .map(|hit| TracePoint {
            uri: hit.uri.clone(),
            score: hit.score,
        })
        .collect::<Vec<_>>();

    let trace = RetrievalTrace {
        trace_id: uuid::Uuid::new_v4().to_string(),
        request_type: options.request_type.clone(),
        query: options.query.clone(),
        target_uri: options.target_uri.as_ref().map(ToString::to_string),
        start_points: Vec::new(),
        steps: Vec::new(),
        final_topk,
        stop_reason,
        metrics: TraceStats {
            latency_ms,
            explored_nodes: 0,
            convergence_rounds: 0,
            typed_query_count: typed_queries.len(),
            relation_enriched_hits: 0,
            relation_enriched_links: 0,
        },
    };

    let (memories, resources, skills) = split_hits(&hits);
    FindResult {
        memories,
        resources,
        skills,
        query_plan: serde_json::to_value(QueryPlan {
            scopes,
            keywords: crate::embedding::tokenize_vec(&options.query),
            typed_queries,
            notes,
        })
        .unwrap_or_else(|_| json!({"notes": ["sqlite_fts"]})),
        query_results: hits,
        trace: Some(trace),
        trace_uri: None,
    }
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
    let Some(object) = result.query_plan.as_object_mut() else {
        result.query_plan = json!({"notes": [note]});
        return;
    };
    let notes = object.entry("notes").or_insert_with(|| json!([]));
    if let Some(array) = notes.as_array_mut() {
        array.push(json!(note));
    }
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
