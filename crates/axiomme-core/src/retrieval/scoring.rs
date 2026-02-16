use std::cmp::Ordering;
use std::collections::HashMap;

use crate::models::{ContextHit, TracePoint};

use super::planner::PlannedQuery;

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

pub(super) fn make_hit(record: &crate::models::IndexRecord, score: f32) -> ContextHit {
    ContextHit {
        uri: record.uri.clone(),
        score,
        abstract_text: record.abstract_text.clone(),
        context_type: record.context_type.clone(),
        relations: Vec::new(),
    }
}

pub(super) fn tokenize_keywords(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| !x.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(super) fn merge_hits(acc: &mut HashMap<String, ContextHit>, hits: Vec<ContextHit>) {
    for hit in hits {
        acc.entry(hit.uri.clone())
            .and_modify(|existing| {
                if hit.score > existing.score {
                    *existing = hit.clone();
                }
            })
            .or_insert(hit);
    }
}

pub(super) fn merge_trace_points(acc: &mut HashMap<String, f32>, points: &[TracePoint]) {
    for point in points {
        acc.entry(point.uri.clone())
            .and_modify(|score| {
                if point.score > *score {
                    *score = point.score;
                }
            })
            .or_insert(point.score);
    }
}

pub(super) fn sorted_trace_points(points: HashMap<String, f32>) -> Vec<TracePoint> {
    let mut out = points
        .into_iter()
        .map(|(uri, score)| TracePoint { uri, score })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.uri.cmp(&b.uri))
    });
    out
}

pub(super) fn typed_query_plans(
    planned_queries: &[PlannedQuery],
) -> Vec<crate::models::TypedQueryPlan> {
    planned_queries
        .iter()
        .map(|x| crate::models::TypedQueryPlan {
            kind: x.kind.clone(),
            query: x.query.clone(),
            scopes: x.scopes.iter().map(|s| s.as_str().to_string()).collect(),
            priority: x.priority,
        })
        .collect()
}
