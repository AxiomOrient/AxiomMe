use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::models::{ContextHit, TracePoint};

use super::planner::PlannedQuery;

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
    let mut out = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    for token in query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| !x.is_empty())
    {
        let token = token.to_string();
        if seen.insert(token.clone()) {
            out.push(token);
        }
    }
    out
}

pub(super) fn merge_hits(acc: &mut HashMap<String, ContextHit>, hits: Vec<ContextHit>) {
    for hit in hits {
        if let Some(existing) = acc.get_mut(&hit.uri) {
            if hit.score > existing.score {
                *existing = hit;
            }
            continue;
        }
        acc.insert(hit.uri.clone(), hit);
    }
}

fn compare_hit_score_desc_then_uri_asc(a: &ContextHit, b: &ContextHit) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.uri.cmp(&b.uri))
}

pub(super) fn sort_hits_by_score_desc_uri_asc(hits: &mut [ContextHit]) {
    hits.sort_by(compare_hit_score_desc_then_uri_asc);
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
        if let Some(score) = acc.get_mut(&point.uri) {
            if point.score > *score {
                *score = point.score;
            }
            continue;
        }
        acc.insert(point.uri.clone(), point.score);
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
    let mut out = Vec::with_capacity(planned_queries.len());
    for x in planned_queries {
        out.push(crate::models::TypedQueryPlan {
            kind: x.kind.clone(),
            query: x.query.clone(),
            scopes: x.scopes.iter().map(|s| s.as_str().to_string()).collect(),
            priority: x.priority,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        fanout_priority_weight, merge_hits, scale_hit_scores, scale_trace_point_scores,
        sort_hits_by_score_desc_uri_asc, tokenize_keywords,
    };
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

    #[test]
    fn hit_sort_is_deterministic_for_equal_scores_via_uri_tiebreak() {
        let mut hits = vec![
            hit("axiom://resources/z.md", 0.70),
            hit("axiom://resources/a.md", 0.70),
            hit("axiom://resources/m.md", 0.90),
        ];
        sort_hits_by_score_desc_uri_asc(&mut hits);
        let uris = hits.iter().map(|x| x.uri.as_str()).collect::<Vec<_>>();
        assert_eq!(
            uris,
            vec![
                "axiom://resources/m.md",
                "axiom://resources/a.md",
                "axiom://resources/z.md"
            ]
        );
    }

    #[test]
    fn tokenize_keywords_normalizes_and_deduplicates_in_order() {
        let tokens = tokenize_keywords("OAuth oauth, token TOKEN refresh");
        assert_eq!(tokens, vec!["oauth", "token", "refresh"]);
    }
}
