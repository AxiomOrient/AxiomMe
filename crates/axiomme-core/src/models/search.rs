use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::uri::AxiomUri;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationSummary {
    pub uri: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_object_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_object_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RelationLink {
    pub id: String,
    pub uris: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHit {
    pub uri: String,
    pub score: f32,
    #[serde(rename = "abstract")]
    pub abstract_text: String,
    pub context_type: String,
    pub relations: Vec<RelationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindResult {
    pub query_plan: QueryPlan,
    pub query_results: Vec<ContextHit>,
    #[serde(default, skip_serializing_if = "HitBuckets::is_empty")]
    pub hit_buckets: HitBuckets,
    #[serde(default)]
    pub memories: Vec<ContextHit>,
    #[serde(default)]
    pub resources: Vec<ContextHit>,
    #[serde(default)]
    pub skills: Vec<ContextHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<RetrievalTrace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct HitBuckets {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memories: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<usize>,
}

impl HitBuckets {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.memories.is_empty() && self.resources.is_empty() && self.skills.is_empty()
    }
}

impl FindResult {
    pub fn rebuild_hit_buckets(&mut self) {
        self.hit_buckets = classify_hit_buckets(&self.query_results);
        self.rebuild_legacy_views();
    }

    pub fn rebuild_legacy_views(&mut self) {
        let (memories, resources, skills) =
            collect_legacy_hit_views(&self.query_results, &self.hit_buckets);
        self.memories = memories;
        self.resources = resources;
        self.skills = skills;
    }

    pub fn memories(&self) -> impl Iterator<Item = &ContextHit> {
        self.hit_buckets
            .memories
            .iter()
            .filter_map(|&index| self.query_results.get(index))
    }

    pub fn resources(&self) -> impl Iterator<Item = &ContextHit> {
        self.hit_buckets
            .resources
            .iter()
            .filter_map(|&index| self.query_results.get(index))
    }

    pub fn skills(&self) -> impl Iterator<Item = &ContextHit> {
        self.hit_buckets
            .skills
            .iter()
            .filter_map(|&index| self.query_results.get(index))
    }
}

pub fn classify_hit_buckets(hits: &[ContextHit]) -> HitBuckets {
    let mut buckets = HitBuckets::default();
    for (index, hit) in hits.iter().enumerate() {
        if hit.uri.starts_with("axiom://user/memories")
            || hit.uri.starts_with("axiom://agent/memories")
        {
            buckets.memories.push(index);
        } else if hit.uri.starts_with("axiom://agent/skills") {
            buckets.skills.push(index);
        } else {
            buckets.resources.push(index);
        }
    }
    buckets
}

fn collect_legacy_hit_views(
    hits: &[ContextHit],
    buckets: &HitBuckets,
) -> (Vec<ContextHit>, Vec<ContextHit>, Vec<ContextHit>) {
    let memories = buckets
        .memories
        .iter()
        .filter_map(|&index| hits.get(index).cloned())
        .collect::<Vec<_>>();
    let resources = buckets
        .resources
        .iter()
        .filter_map(|&index| hits.get(index).cloned())
        .collect::<Vec<_>>();
    let skills = buckets
        .skills
        .iter()
        .filter_map(|&index| hits.get(index).cloned())
        .collect::<Vec<_>>();
    (memories, resources, skills)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilter {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalStep {
    pub round: u32,
    pub current_uri: String,
    pub children_examined: usize,
    pub children_selected: usize,
    pub queue_size_after: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStats {
    pub latency_ms: u128,
    pub explored_nodes: usize,
    pub convergence_rounds: u32,
    #[serde(default)]
    pub typed_query_count: usize,
    #[serde(default)]
    pub relation_enriched_hits: usize,
    #[serde(default)]
    pub relation_enriched_links: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTrace {
    pub trace_id: String,
    pub request_type: String,
    pub query: String,
    pub target_uri: Option<String>,
    pub start_points: Vec<TracePoint>,
    pub steps: Vec<RetrievalStep>,
    pub final_topk: Vec<TracePoint>,
    pub stop_reason: String,
    pub metrics: TraceStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracePoint {
    pub uri: String,
    pub score: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecord {
    pub id: String,
    pub uri: String,
    pub parent_uri: Option<String>,
    pub is_leaf: bool,
    pub context_type: String,
    pub name: String,
    pub abstract_text: String,
    pub content: String,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub query: String,
    pub target_uri: Option<AxiomUri>,
    pub session: Option<String>,
    #[serde(default)]
    pub session_hints: Vec<String>,
    #[serde(default)]
    pub budget: Option<SearchBudget>,
    pub limit: usize,
    pub score_threshold: Option<f32>,
    pub min_match_tokens: Option<usize>,
    pub filter: Option<SearchFilter>,
    pub request_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchBudget {
    pub max_ms: Option<u64>,
    pub max_nodes: Option<usize>,
    pub max_depth: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryPlan {
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub typed_queries: Vec<TypedQueryPlan>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedQueryPlan {
    pub kind: String,
    pub query: String,
    pub scopes: Vec<String>,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub fields: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_match_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<MetadataFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<SearchBudget>,
    #[serde(default)]
    pub runtime_hints: Vec<RuntimeHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHintKind {
    Observation,
    CurrentTask,
    SuggestedResponse,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeHint {
    pub kind: RuntimeHintKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingBackendStatus {
    pub provider: String,
    pub vector_version: String,
    pub dim: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub local_records: usize,
    pub retrieval_backend: String,
    pub retrieval_backend_policy: String,
    pub embedding: EmbeddingBackendStatus,
}

#[cfg(test)]
mod tests {
    use super::{
        ContextHit, FindResult, QueryPlan, SearchRequest, TypedQueryPlan, classify_hit_buckets,
    };

    #[test]
    fn query_plan_serialization_snapshot_is_stable() {
        let plan = QueryPlan {
            scopes: vec!["resources".to_string()],
            keywords: vec!["oauth".to_string()],
            typed_queries: vec![
                TypedQueryPlan {
                    kind: "primary".to_string(),
                    query: "oauth".to_string(),
                    scopes: vec!["resources".to_string()],
                    priority: 1,
                },
                TypedQueryPlan {
                    kind: "session_recent".to_string(),
                    query: "oauth hint".to_string(),
                    scopes: vec!["session".to_string()],
                    priority: 2,
                },
            ],
            notes: vec!["backend:memory".to_string(), "budget_nodes:10".to_string()],
        };

        let encoded = serde_json::to_value(&plan).expect("serialize query plan");
        assert_eq!(
            encoded,
            serde_json::json!({
                "scopes": ["resources"],
                "keywords": ["oauth"],
                "typed_queries": [
                    {
                        "kind": "primary",
                        "query": "oauth",
                        "scopes": ["resources"],
                        "priority": 1
                    },
                    {
                        "kind": "session_recent",
                        "query": "oauth hint",
                        "scopes": ["session"],
                        "priority": 2
                    }
                ],
                "notes": ["backend:memory", "budget_nodes:10"]
            })
        );
    }

    #[test]
    fn runtime_hint_serde_backward_compat() {
        let payload = serde_json::json!({
            "query": "oauth",
            "target_uri": "axiom://resources",
            "session": "s-1",
            "limit": 5
        });
        let decoded: SearchRequest =
            serde_json::from_value(payload).expect("deserialize search request");
        assert!(decoded.runtime_hints.is_empty());
    }

    fn hit(uri: &str) -> ContextHit {
        ContextHit {
            uri: uri.to_string(),
            score: 0.5,
            abstract_text: String::new(),
            context_type: "resource".to_string(),
            relations: Vec::new(),
        }
    }

    #[test]
    fn classify_hit_buckets_assigns_stable_indices() {
        let hits = vec![
            hit("axiom://resources/docs/a.md"),
            hit("axiom://user/memories/preferences/pref.md"),
            hit("axiom://agent/skills/rust.md"),
            hit("axiom://agent/memories/patterns/pat.md"),
        ];
        let buckets = classify_hit_buckets(&hits);
        assert_eq!(buckets.resources, vec![0]);
        assert_eq!(buckets.memories, vec![1, 3]);
        assert_eq!(buckets.skills, vec![2]);
    }

    #[test]
    fn find_result_bucket_accessors_read_from_query_results() {
        let query_results = vec![
            hit("axiom://resources/docs/a.md"),
            hit("axiom://user/memories/preferences/pref.md"),
            hit("axiom://agent/skills/rust.md"),
        ];
        let hit_buckets = classify_hit_buckets(&query_results);
        let mut result = FindResult {
            query_plan: QueryPlan::default(),
            hit_buckets,
            query_results,
            memories: Vec::new(),
            resources: Vec::new(),
            skills: Vec::new(),
            trace: None,
            trace_uri: None,
        };
        result.rebuild_legacy_views();
        let memories = result
            .memories()
            .map(|item| item.uri.clone())
            .collect::<Vec<_>>();
        let resources = result
            .resources()
            .map(|item| item.uri.clone())
            .collect::<Vec<_>>();
        let skills = result
            .skills()
            .map(|item| item.uri.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            memories,
            vec!["axiom://user/memories/preferences/pref.md".to_string()]
        );
        assert_eq!(resources, vec!["axiom://resources/docs/a.md".to_string()]);
        assert_eq!(skills, vec!["axiom://agent/skills/rust.md".to_string()]);
    }

    #[test]
    fn find_result_legacy_views_are_derived_from_hit_buckets() {
        let query_results = vec![
            hit("axiom://resources/docs/a.md"),
            hit("axiom://user/memories/preferences/pref.md"),
            hit("axiom://agent/skills/rust.md"),
        ];
        let hit_buckets = classify_hit_buckets(&query_results);
        let mut result = FindResult {
            query_plan: QueryPlan::default(),
            query_results,
            hit_buckets,
            memories: Vec::new(),
            resources: Vec::new(),
            skills: Vec::new(),
            trace: None,
            trace_uri: None,
        };

        result.rebuild_legacy_views();
        assert_eq!(result.memories.len(), 1);
        assert_eq!(result.resources.len(), 1);
        assert_eq!(result.skills.len(), 1);
    }
}
