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

pub(super) const fn fanout_priority_weight(priority: u8) -> f32 {
    match priority {
        0 | 1 => 1.0,
        2 => 0.82,
        3 => 0.64,
        _ => 0.46,
    }
}

pub(super) fn scale_hit_scores(hits: &mut [ContextHit], weight: f32) {
    if weight >= 1.0 {
        return;
    }
    for hit in hits {
        hit.score *= weight;
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

pub(super) fn scale_trace_point_scores(points: &mut [TracePoint], weight: f32) {
    if weight >= 1.0 {
        return;
    }
    for point in points {
        point.score *= weight;
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

#[cfg(test)]
mod tests {
    use super::{fanout_priority_weight, merge_hits, scale_hit_scores, scale_trace_point_scores};
    use crate::models::{ContextHit, TracePoint};
    use std::collections::HashMap;

    fn hit(uri: &str, score: f32) -> ContextHit {
        ContextHit {
            uri: uri.to_string(),
            score,
            abstract_text: String::new(),
            context_type: "resource".to_string(),
            relations: Vec::new(),
        }
    }

    #[test]
    fn fanout_priority_weight_profile_is_explicit_and_deterministic() {
        assert_eq!(fanout_priority_weight(1), 1.0);
        assert_eq!(fanout_priority_weight(2), 0.82);
        assert_eq!(fanout_priority_weight(3), 0.64);
        assert_eq!(fanout_priority_weight(4), 0.46);
        assert_eq!(fanout_priority_weight(9), 0.46);
    }

    #[test]
    fn weighted_merge_keeps_primary_when_secondary_query_is_noisy() {
        let mut merged = HashMap::new();
        merge_hits(&mut merged, vec![hit("axiom://resources/exact.md", 0.72)]);

        let mut noisy_hits = vec![
            hit("axiom://resources/exact.md", 0.79),
            hit("axiom://resources/noise.md", 0.84),
        ];
        scale_hit_scores(&mut noisy_hits, fanout_priority_weight(2));
        merge_hits(&mut merged, noisy_hits);

        let exact = merged
            .get("axiom://resources/exact.md")
            .expect("exact hit")
            .score;
        let noise = merged
            .get("axiom://resources/noise.md")
            .expect("noise hit")
            .score;

        assert!((exact - 0.72).abs() < 0.0001);
        assert!(noise < exact);
    }

    #[test]
    fn scale_trace_point_scores_applies_same_weight_rule() {
        let mut points = vec![TracePoint {
            uri: "axiom://resources/root".to_string(),
            score: 0.50,
        }];
        scale_trace_point_scores(&mut points, fanout_priority_weight(3));
        assert!((points[0].score - 0.32).abs() < 0.0001);
    }
}
