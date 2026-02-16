use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::uri::AxiomUri;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationSummary {
    pub uri: String,
    pub reason: String,
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
    pub memories: Vec<ContextHit>,
    pub resources: Vec<ContextHit>,
    pub skills: Vec<ContextHit>,
    pub query_plan: serde_json::Value,
    pub query_results: Vec<ContextHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<RetrievalTrace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_uri: Option<String>,
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
    pub scopes: Vec<String>,
    pub keywords: Vec<String>,
    #[serde(default)]
    pub typed_queries: Vec<TypedQueryPlan>,
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
    pub embedding: EmbeddingBackendStatus,
}
