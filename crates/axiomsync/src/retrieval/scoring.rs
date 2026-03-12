use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::index::ScoredRecord;
use crate::models::{ContextHit, ScoreComponents, TracePoint};

use super::planner::PlannedQuery;

const MAX_SNIPPET_CHARS: usize = 240;

pub(super) fn make_hit(
    record: &crate::models::IndexRecord,
    score: f32,
    query: &str,
    components: Option<&ScoredRecord>,
) -> ContextHit {
    let query_tokens = tokenize_keywords(query);
    let snippet = build_snippet(record, &query_tokens);
    let matched_heading = find_matched_heading(record, &query_tokens);
    ContextHit {
        uri: record.uri.clone(),
        score,
        abstract_text: record.abstract_text.clone(),
        context_type: record.context_type.clone(),
        relations: Vec::new(),
        snippet,
        matched_heading,
        score_components: score_components_from_scored(components),
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
        hit.score_components.exact *= weight;
        hit.score_components.dense *= weight;
        hit.score_components.sparse *= weight;
        hit.score_components.path *= weight;
        hit.score_components.recency *= weight;
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

fn score_components_from_scored(scored: Option<&ScoredRecord>) -> ScoreComponents {
    let Some(scored) = scored else {
        return ScoreComponents::default();
    };
    ScoreComponents {
        exact: scored.exact,
        dense: scored.dense,
        sparse: scored.sparse,
        path: scored.path,
        recency: scored.recency,
    }
}

fn build_snippet(record: &crate::models::IndexRecord, query_tokens: &[String]) -> Option<String> {
    let content_line = record
        .content
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && if query_tokens.is_empty() {
                    true
                } else {
                    line_contains_any_token(line, query_tokens)
                }
        })
        .map(clip_preview);
    if content_line.is_some() {
        return content_line;
    }

    let abstract_line = clip_preview(record.abstract_text.trim());
    if !abstract_line.is_empty() {
        return Some(abstract_line);
    }
    None
}

fn find_matched_heading(
    record: &crate::models::IndexRecord,
    query_tokens: &[String],
) -> Option<String> {
    let mut first_heading = None::<String>;
    for line in record.content.lines().map(str::trim) {
        if !line.starts_with('#') {
            continue;
        }
        let heading = line.trim_start_matches('#').trim();
        if heading.is_empty() {
            continue;
        }
        if first_heading.is_none() {
            first_heading = Some(clip_preview(heading));
        }
        if !query_tokens.is_empty() && line_contains_any_token(heading, query_tokens) {
            return Some(clip_preview(heading));
        }
    }
    first_heading
}

fn line_contains_any_token(line: &str, query_tokens: &[String]) -> bool {
    let lowered = line.to_ascii_lowercase();
    query_tokens.iter().any(|token| lowered.contains(token))
}

fn clip_preview(raw: &str) -> String {
    raw.chars().take(MAX_SNIPPET_CHARS).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::{
        fanout_priority_weight, make_hit, merge_hits, scale_hit_scores, scale_trace_point_scores,
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
            snippet: None,
            matched_heading: None,
            score_components: crate::models::ScoreComponents::default(),
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
    fn make_hit_contains_snippet_and_score_components() {
        let record = crate::models::IndexRecord {
            id: "id-1".to_string(),
            uri: "axiom://resources/docs/api.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "api.md".to_string(),
            abstract_text: "api guide".to_string(),
            content: "# OAuth Flow\nUse token exchange endpoint.".to_string(),
            tags: vec!["markdown".to_string()],
            updated_at: chrono::Utc::now(),
            depth: 3,
        };
        let scored = crate::index::ScoredRecord {
            uri: std::sync::Arc::from(record.uri.as_str()),
            is_leaf: true,
            depth: 3,
            exact: 0.91,
            dense: 0.52,
            sparse: 0.73,
            recency: 0.40,
            path: 0.17,
            score: 0.88,
        };
        let hit = make_hit(&record, 0.88, "oauth token", Some(&scored));
        assert_eq!(hit.matched_heading.as_deref(), Some("OAuth Flow"));
        assert!(hit.snippet.is_some());
        assert!(hit.score_components.exact > 0.0);
        assert!(hit.score_components.sparse > 0.0);
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
