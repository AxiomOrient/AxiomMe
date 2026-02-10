use std::time::Instant;

use chrono::Utc;
use serde_json::json;

use crate::context_ops::validate_filter;
use crate::error::{AxiomError, Result};
use crate::models::{
    ContextHit, FindResult, IndexRecord, MetadataFilter, QueryPlan, RequestLogEntry,
    RetrievalTrace, SearchBudget, SearchFilter, SearchOptions, TracePoint, TraceStats,
    TypedQueryPlan,
};
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub fn find(
        &self,
        query: &str,
        target_uri: Option<&str>,
        limit: Option<usize>,
        score_threshold: Option<f32>,
        filter: Option<MetadataFilter>,
    ) -> Result<FindResult> {
        self.find_with_budget(query, target_uri, limit, score_threshold, filter, None)
    }

    pub fn find_with_budget(
        &self,
        query: &str,
        target_uri: Option<&str>,
        limit: Option<usize>,
        score_threshold: Option<f32>,
        filter: Option<MetadataFilter>,
        budget: Option<SearchBudget>,
    ) -> Result<FindResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let target_raw = target_uri.map(ToString::to_string);
        let requested_limit = limit.unwrap_or(10);
        let budget = normalize_budget(budget);

        let output = (|| -> Result<FindResult> {
            validate_filter(filter.as_ref())?;
            let target = target_uri.map(AxiomUri::parse).transpose()?;
            let options = SearchOptions {
                query: query.to_string(),
                target_uri: target,
                session: None,
                session_hints: Vec::new(),
                budget: budget.clone(),
                limit: requested_limit,
                score_threshold,
                filter: metadata_filter_to_search_filter(filter),
                request_type: "find".to_string(),
            };

            let mut result = self.run_retrieval_with_backend(options)?;
            self.enrich_find_result_relations(&mut result, 5)?;
            annotate_trace_relation_metrics(&mut result);
            self.persist_trace_result(&mut result)?;
            Ok(result)
        })();

        match output {
            Ok(result) => {
                let trace_id = result.trace.as_ref().map(|x| x.trace_id.clone());
                let details = serde_json::json!({
                    "query": query,
                    "result_count": result.query_results.len(),
                    "limit": requested_limit,
                    "budget": budget_to_json(budget.as_ref()),
                });
                self.try_log_request(RequestLogEntry {
                    request_id,
                    operation: "find".to_string(),
                    status: "ok".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id,
                    target_uri: target_raw,
                    error_code: None,
                    error_message: None,
                    details: Some(details),
                });
                Ok(result)
            }
            Err(err) => {
                self.try_log_request(RequestLogEntry {
                    request_id,
                    operation: "find".to_string(),
                    status: "error".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id: None,
                    target_uri: target_raw,
                    error_code: Some(err.code().to_string()),
                    error_message: Some(err.to_string()),
                    details: Some(serde_json::json!({
                        "query": query,
                        "limit": requested_limit,
                        "budget": budget_to_json(budget.as_ref()),
                    })),
                });
                Err(err)
            }
        }
    }

    pub fn search(
        &self,
        query: &str,
        target_uri: Option<&str>,
        session: Option<&str>,
        limit: Option<usize>,
        score_threshold: Option<f32>,
        filter: Option<MetadataFilter>,
    ) -> Result<FindResult> {
        self.search_with_budget(
            query,
            target_uri,
            session,
            limit,
            score_threshold,
            filter,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn search_with_budget(
        &self,
        query: &str,
        target_uri: Option<&str>,
        session: Option<&str>,
        limit: Option<usize>,
        score_threshold: Option<f32>,
        filter: Option<MetadataFilter>,
        budget: Option<SearchBudget>,
    ) -> Result<FindResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let target_raw = target_uri.map(ToString::to_string);
        let session_raw = session.map(ToString::to_string);
        let requested_limit = limit.unwrap_or(10);
        let budget = normalize_budget(budget);

        let output = (|| -> Result<FindResult> {
            validate_filter(filter.as_ref())?;
            let target = target_uri.map(AxiomUri::parse).transpose()?;

            let session_hints = if let Some(session_id) = session {
                let ctx = self
                    .session(Some(session_id))
                    .get_context_for_search(query, 2, 8)?;
                ctx.recent_messages
                    .iter()
                    .rev()
                    .take(2)
                    .map(|m| m.text.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let options = SearchOptions {
                query: query.to_string(),
                target_uri: target,
                session: session.map(ToString::to_string),
                session_hints,
                budget: budget.clone(),
                limit: requested_limit,
                score_threshold,
                filter: metadata_filter_to_search_filter(filter),
                request_type: "search".to_string(),
            };

            let mut result = self.run_retrieval_with_backend(options)?;
            self.enrich_find_result_relations(&mut result, 5)?;
            annotate_trace_relation_metrics(&mut result);
            self.persist_trace_result(&mut result)?;
            Ok(result)
        })();

        match output {
            Ok(result) => {
                let trace_id = result.trace.as_ref().map(|x| x.trace_id.clone());
                let details = serde_json::json!({
                    "query": query,
                    "result_count": result.query_results.len(),
                    "limit": requested_limit,
                    "session": session_raw,
                    "budget": budget_to_json(budget.as_ref()),
                });
                self.try_log_request(RequestLogEntry {
                    request_id,
                    operation: "search".to_string(),
                    status: "ok".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id,
                    target_uri: target_raw,
                    error_code: None,
                    error_message: None,
                    details: Some(details),
                });
                Ok(result)
            }
            Err(err) => {
                self.try_log_request(RequestLogEntry {
                    request_id,
                    operation: "search".to_string(),
                    status: "error".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id: None,
                    target_uri: target_raw,
                    error_code: Some(err.code().to_string()),
                    error_message: Some(err.to_string()),
                    details: Some(serde_json::json!({
                        "query": query,
                        "limit": requested_limit,
                        "session": session_raw,
                        "budget": budget_to_json(budget.as_ref()),
                    })),
                });
                Err(err)
            }
        }
    }

    fn run_retrieval_with_backend(&self, options: SearchOptions) -> Result<FindResult> {
        let backend =
            resolve_retrieval_backend_mode(std::env::var("AXIOMME_RETRIEVAL_BACKEND").ok());
        self.run_retrieval_with_backend_mode(options, backend)
    }

    fn run_retrieval_with_backend_mode(
        &self,
        options: SearchOptions,
        backend: RetrievalBackendMode,
    ) -> Result<FindResult> {
        let requested_limit = options.limit.max(1);
        let mut result = match backend {
            RetrievalBackendMode::Sqlite => match self.run_sqlite_retrieval(options.clone()) {
                Ok(mut sqlite_result) => {
                    append_query_plan_note(&mut sqlite_result, "backend:sqlite");
                    sqlite_result
                }
                Err(err) => {
                    let mut memory_result = self.run_memory_retrieval(options.clone())?;
                    self.record_sqlite_fallback(&err);
                    append_query_plan_note(
                        &mut memory_result,
                        &format!("backend_fallback:memory({})", err.code()),
                    );
                    append_query_plan_note(&mut memory_result, "backend:memory");
                    memory_result
                }
            },
            RetrievalBackendMode::Memory => {
                let mut memory_result = self.run_memory_retrieval(options.clone())?;
                append_query_plan_note(&mut memory_result, "backend:memory");
                memory_result
            }
            RetrievalBackendMode::Qdrant | RetrievalBackendMode::Hybrid => {
                let mut memory_result = self.run_memory_retrieval(options.clone())?;
                append_query_plan_note(
                    &mut memory_result,
                    &format!("backend:{}", backend.as_str()),
                );

                let qdrant_hits = self.collect_qdrant_hits(&options);
                match qdrant_hits {
                    Ok(hits) => {
                        apply_backend_hits(
                            &mut memory_result,
                            hits,
                            backend == RetrievalBackendMode::Hybrid,
                            requested_limit,
                        );
                        memory_result
                    }
                    Err(err) => {
                        self.record_qdrant_fallback(backend, &options, &err);
                        append_query_plan_note(
                            &mut memory_result,
                            &format!("backend_fallback:memory({})", err.code()),
                        );
                        memory_result
                    }
                }
            }
        };

        let reranker_mode = resolve_reranker_mode(std::env::var("AXIOMME_RERANKER").ok());
        self.apply_reranker_with_mode(&options.query, &mut result, requested_limit, reranker_mode)?;
        Ok(result)
    }

    fn run_memory_retrieval(&self, options: SearchOptions) -> Result<FindResult> {
        let mut memory_result = {
            let index = self
                .index
                .read()
                .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
            self.drr.run(&index, options)
        };
        let embed_profile = crate::embedding::embedding_profile();
        append_query_plan_note(
            &mut memory_result,
            &format!(
                "embedder:{}@{}",
                embed_profile.provider, embed_profile.vector_version
            ),
        );
        Ok(memory_result)
    }

    fn run_sqlite_retrieval(&self, options: SearchOptions) -> Result<FindResult> {
        let started = Instant::now();
        let target_prefix = options.target_uri.as_ref().map(ToString::to_string);
        let effective_query = if options.session_hints.is_empty() {
            options.query.clone()
        } else {
            format!("{} {}", options.query, options.session_hints.join(" "))
        };

        if matches!(options.budget.as_ref().and_then(|x| x.max_ms), Some(0)) {
            return Ok(build_sqlite_result(
                &options,
                Vec::new(),
                "budget_ms".to_string(),
                started.elapsed().as_millis(),
            ));
        }

        let max_depth = options.budget.as_ref().and_then(|x| x.max_depth);
        let mut effective_limit = options.limit.max(1);
        if let Some(max_nodes) = options.budget.as_ref().and_then(|x| x.max_nodes) {
            effective_limit = effective_limit.min(max_nodes.max(1));
        }

        let mut hits = self.state.search_documents_fts(
            &effective_query,
            target_prefix.as_deref(),
            options.filter.as_ref(),
            max_depth,
            effective_limit,
        )?;
        if let Some(threshold) = options.score_threshold {
            hits.retain(|hit| hit.score >= threshold);
        }

        let mapped_hits = hits
            .into_iter()
            .map(|hit| ContextHit {
                uri: hit.uri,
                score: hit.score,
                abstract_text: hit.abstract_text,
                context_type: hit.context_type,
                relations: Vec::new(),
            })
            .collect::<Vec<_>>();

        let stop_reason = if mapped_hits.is_empty() {
            "sqlite_no_match".to_string()
        } else {
            "sqlite_fts".to_string()
        };

        Ok(build_sqlite_result(
            &options,
            mapped_hits,
            stop_reason,
            started.elapsed().as_millis(),
        ))
    }

    fn collect_qdrant_hits(&self, options: &SearchOptions) -> Result<Vec<ContextHit>> {
        let qdrant = self
            .qdrant
            .as_ref()
            .ok_or_else(|| AxiomError::Internal("qdrant backend is not configured".to_string()))?;
        let mut hits = qdrant.search_points(
            &options.query,
            options.limit.max(1),
            options.filter.as_ref(),
        )?;
        let index = self
            .index
            .read()
            .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
        hits.retain(|hit| {
            if let Some(target) = options.target_uri.as_ref()
                && let Ok(uri) = AxiomUri::parse(&hit.uri)
                && !uri.starts_with(target)
            {
                return false;
            }
            if let Some(threshold) = options.score_threshold
                && hit.score < threshold
            {
                return false;
            }
            if let Some(filter) = options.filter.as_ref() {
                let Some(record) = index.get(&hit.uri) else {
                    return false;
                };
                if !index.record_matches_filter(record, Some(filter)) {
                    return false;
                }
            }
            true
        });
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.uri.cmp(&b.uri))
        });
        hits.truncate(options.limit.max(1));

        Ok(hits
            .into_iter()
            .map(|hit| ContextHit {
                uri: hit.uri,
                score: hit.score,
                abstract_text: hit.abstract_text,
                context_type: hit.context_type,
                relations: Vec::new(),
            })
            .collect::<Vec<_>>())
    }

    fn apply_reranker_with_mode(
        &self,
        query: &str,
        result: &mut FindResult,
        limit: usize,
        mode: RerankerMode,
    ) -> Result<()> {
        append_query_plan_note(result, &format!("reranker:{}", mode.as_str()));
        if mode == RerankerMode::Off || result.query_results.len() <= 1 {
            sync_trace_final_topk(result);
            return Ok(());
        }

        let query_tokens = crate::embedding::tokenize_vec(query);
        let intent = classify_query_intent(query, &query_tokens);
        append_query_plan_note(result, &format!("reranker_intent:{}", intent.as_str()));

        let index = self
            .index
            .read()
            .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
        let mut reranked = result
            .query_results
            .iter()
            .map(|hit| {
                let record = index.get(&hit.uri);
                let boost = doc_aware_boost(query, &query_tokens, intent, hit, record);
                let mut out = hit.clone();
                out.score = (out.score * (1.0 + boost)).max(0.0);
                out
            })
            .collect::<Vec<_>>();
        drop(index);

        reranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.uri.cmp(&b.uri))
        });
        reranked.truncate(limit.max(1));
        let (memories, resources, skills) = split_hits(&reranked);
        result.query_results = reranked;
        result.memories = memories;
        result.resources = resources;
        result.skills = skills;
        sync_trace_final_topk(result);

        Ok(())
    }

    fn record_qdrant_fallback(
        &self,
        backend: RetrievalBackendMode,
        options: &SearchOptions,
        err: &AxiomError,
    ) {
        let target_uri = options
            .target_uri
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "axiom://resources".to_string());
        let payload = json!({
            "backend": backend.as_str(),
            "query": options.query,
            "request_type": options.request_type,
            "error_code": err.code(),
            "error": err.to_string(),
        });

        if let Ok(event_id) =
            self.state
                .enqueue("qdrant_search_failed", &target_uri, payload.clone())
        {
            let _ = self.state.mark_outbox_status(event_id, "dead_letter", true);
        }

        self.try_log_request(RequestLogEntry {
            request_id: uuid::Uuid::new_v4().to_string(),
            operation: "retrieval.backend_fallback".to_string(),
            status: "fallback".to_string(),
            latency_ms: 0,
            created_at: Utc::now().to_rfc3339(),
            trace_id: None,
            target_uri: Some(target_uri),
            error_code: Some(err.code().to_string()),
            error_message: Some(err.to_string()),
            details: Some(payload),
        });
    }

    fn record_sqlite_fallback(&self, err: &AxiomError) {
        let target_uri = "axiom://resources".to_string();
        let payload = json!({
            "backend": "sqlite",
            "error_code": err.code(),
            "error": err.to_string(),
        });

        if let Ok(event_id) =
            self.state
                .enqueue("sqlite_search_failed", &target_uri, payload.clone())
        {
            let _ = self.state.mark_outbox_status(event_id, "dead_letter", true);
        }

        self.try_log_request(RequestLogEntry {
            request_id: uuid::Uuid::new_v4().to_string(),
            operation: "retrieval.backend_fallback".to_string(),
            status: "fallback".to_string(),
            latency_ms: 0,
            created_at: Utc::now().to_rfc3339(),
            trace_id: None,
            target_uri: Some(target_uri),
            error_code: Some(err.code().to_string()),
            error_message: Some(err.to_string()),
            details: Some(payload),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetrievalBackendMode {
    Sqlite,
    Memory,
    Qdrant,
    Hybrid,
}

impl RetrievalBackendMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Memory => "memory",
            Self::Qdrant => "qdrant",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RerankerMode {
    Off,
    DocAwareV1,
}

impl RerankerMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::DocAwareV1 => "doc-aware-v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryIntent {
    Lexical,
    Semantic,
    Mixed,
}

impl QueryIntent {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Semantic => "semantic",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentClass {
    Code,
    Config,
    Spec,
    Narrative,
    Memory,
    Skill,
    Session,
    Data,
    General,
}

fn resolve_retrieval_backend_mode(raw: Option<String>) -> RetrievalBackendMode {
    match raw
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("sqlite") | Some("fts") | Some("fts5") | Some("bm25") => RetrievalBackendMode::Sqlite,
        Some("qdrant") => RetrievalBackendMode::Qdrant,
        Some("hybrid") => RetrievalBackendMode::Hybrid,
        Some("memory") => RetrievalBackendMode::Memory,
        _ => RetrievalBackendMode::Sqlite,
    }
}

fn resolve_reranker_mode(raw: Option<String>) -> RerankerMode {
    match raw
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("off") | Some("none") | Some("disabled") => RerankerMode::Off,
        Some("doc-aware") | Some("doc-aware-v1") | None => RerankerMode::DocAwareV1,
        _ => RerankerMode::DocAwareV1,
    }
}

fn classify_query_intent(query: &str, tokens: &[String]) -> QueryIntent {
    let has_symbolic = query.contains("::")
        || query.contains('/')
        || query.contains('.')
        || query.contains('_')
        || query.contains('`');
    let has_digit = query.chars().any(|ch| ch.is_ascii_digit());
    if (has_symbolic || has_digit) && tokens.len() <= 3 {
        return QueryIntent::Lexical;
    }
    if tokens.len() >= 4 && !has_symbolic && !has_digit {
        return QueryIntent::Semantic;
    }
    QueryIntent::Mixed
}

fn query_has_any(tokens: &[String], terms: &[&str]) -> bool {
    tokens
        .iter()
        .any(|token| terms.iter().any(|term| token == term))
}

fn classify_document_class(hit: &ContextHit, record: Option<&IndexRecord>) -> DocumentClass {
    if hit.context_type == "memory" {
        return DocumentClass::Memory;
    }
    if hit.context_type == "skill" {
        return DocumentClass::Skill;
    }
    if hit.context_type == "session" || hit.uri.starts_with("axiom://session/") {
        return DocumentClass::Session;
    }

    let (name, uri_lower) = if let Some(record) = record {
        (
            record.name.to_ascii_lowercase(),
            record.uri.to_ascii_lowercase(),
        )
    } else {
        let name = hit
            .uri
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        (name, hit.uri.to_ascii_lowercase())
    };
    let ext = name.rsplit('.').next().unwrap_or_default();

    if uri_lower.contains("/spec")
        || uri_lower.contains("/contract")
        || uri_lower.contains("/openapi")
        || uri_lower.contains("/schema")
        || name.contains("openapi")
        || name.contains("schema")
        || name.contains("contract")
    {
        return DocumentClass::Spec;
    }
    if matches!(
        ext,
        "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "java" | "go" | "c" | "cpp" | "h"
    ) {
        return DocumentClass::Code;
    }
    if matches!(
        ext,
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "env"
    ) {
        return DocumentClass::Config;
    }
    if matches!(ext, "json" | "jsonl" | "csv" | "tsv" | "parquet") {
        return DocumentClass::Data;
    }
    if matches!(ext, "md" | "markdown" | "txt" | "rst" | "adoc") {
        return DocumentClass::Narrative;
    }

    DocumentClass::General
}

fn doc_aware_boost(
    _query: &str,
    query_tokens: &[String],
    intent: QueryIntent,
    hit: &ContextHit,
    record: Option<&IndexRecord>,
) -> f32 {
    let doc_class = classify_document_class(hit, record);
    let mut boost = match intent {
        QueryIntent::Lexical => match doc_class {
            DocumentClass::Code => 0.12,
            DocumentClass::Config => 0.10,
            DocumentClass::Spec => 0.08,
            _ => 0.0,
        },
        QueryIntent::Semantic => match doc_class {
            DocumentClass::Narrative => 0.12,
            DocumentClass::Spec => 0.09,
            DocumentClass::Memory => 0.09,
            _ => 0.0,
        },
        QueryIntent::Mixed => match doc_class {
            DocumentClass::Spec => 0.10,
            DocumentClass::Narrative => 0.08,
            DocumentClass::Code => 0.08,
            DocumentClass::Config => 0.08,
            _ => 0.0,
        },
    };

    let wants_api = query_has_any(
        query_tokens,
        &[
            "api", "endpoint", "schema", "contract", "spec", "openapi", "grpc",
        ],
    );
    let wants_config = query_has_any(
        query_tokens,
        &[
            "config",
            "configuration",
            "setting",
            "settings",
            "env",
            "yaml",
            "yml",
            "toml",
            "json",
            "ini",
            "docker",
            "compose",
        ],
    );
    let wants_code = query_has_any(
        query_tokens,
        &[
            "code", "impl", "function", "stack", "panic", "compile", "build", "trace",
        ],
    ) || matches!(intent, QueryIntent::Lexical);
    let wants_guide = query_has_any(
        query_tokens,
        &["guide", "overview", "summary", "explain", "how", "why"],
    );
    let wants_memory = query_has_any(
        query_tokens,
        &["memory", "memories", "preference", "remember"],
    );
    let wants_skill = query_has_any(query_tokens, &["skill", "skills", "tool", "tools"]);
    let wants_session = query_has_any(query_tokens, &["session", "recent", "conversation", "chat"]);

    if wants_api && matches!(doc_class, DocumentClass::Spec) {
        boost += 0.22;
    }
    if wants_config && matches!(doc_class, DocumentClass::Config | DocumentClass::Data) {
        boost += 0.20;
    }
    if wants_code && matches!(doc_class, DocumentClass::Code) {
        boost += 0.18;
    }
    if wants_guide && matches!(doc_class, DocumentClass::Narrative | DocumentClass::Spec) {
        boost += 0.16;
    }
    if wants_memory && matches!(doc_class, DocumentClass::Memory) {
        boost += 0.24;
    }
    if wants_skill && matches!(doc_class, DocumentClass::Skill) {
        boost += 0.24;
    }
    if wants_session && matches!(doc_class, DocumentClass::Session) {
        boost += 0.20;
    }

    let (name_lower, uri_lower) = if let Some(record) = record {
        (
            record.name.to_ascii_lowercase(),
            record.uri.to_ascii_lowercase(),
        )
    } else {
        let name = hit
            .uri
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        (name, hit.uri.to_ascii_lowercase())
    };
    if query_tokens
        .iter()
        .any(|token| name_lower.contains(token) || uri_lower.contains(token))
    {
        boost += 0.08;
    }

    if let Some(record) = record {
        let tag_overlap = record
            .tags
            .iter()
            .map(|tag| tag.to_ascii_lowercase())
            .filter(|tag| query_tokens.iter().any(|token| token == tag))
            .count()
            .min(3);
        boost += tag_overlap as f32 * 0.03;
    }

    boost.clamp(0.0, 0.65)
}

fn metadata_filter_to_search_filter(filter: Option<MetadataFilter>) -> Option<SearchFilter> {
    filter.map(|f| SearchFilter {
        tags: f
            .fields
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(ToString::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        mime: f
            .fields
            .get("mime")
            .and_then(|v| v.as_str().map(ToString::to_string)),
    })
}

fn normalize_budget(budget: Option<SearchBudget>) -> Option<SearchBudget> {
    let budget = budget?;
    if budget.max_ms.is_none() && budget.max_nodes.is_none() && budget.max_depth.is_none() {
        return None;
    }
    Some(budget)
}

fn budget_to_json(budget: Option<&SearchBudget>) -> serde_json::Value {
    match budget {
        Some(budget) => json!({
            "max_ms": budget.max_ms,
            "max_nodes": budget.max_nodes,
            "max_depth": budget.max_depth,
        }),
        None => serde_json::Value::Null,
    }
}

fn build_sqlite_result(
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
        notes.push(format!("session_hints:{}", options.session_hints.len()));
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
        .unwrap_or_else(|| {
            vec![
                "resources".to_string(),
                "user".to_string(),
                "agent".to_string(),
                "session".to_string(),
            ]
        });

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

fn apply_backend_hits(
    result: &mut FindResult,
    backend_hits: Vec<ContextHit>,
    merge: bool,
    limit: usize,
) {
    let mut next_hits = if merge {
        rrf_fuse_hits(&result.query_results, &backend_hits, 60)
    } else {
        backend_hits
    };

    next_hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.uri.cmp(&b.uri))
    });
    next_hits.truncate(limit.max(1));

    let (memories, resources, skills) = split_hits(&next_hits);
    result.query_results = next_hits;
    result.memories = memories;
    result.resources = resources;
    result.skills = skills;
    sync_trace_final_topk(result);
}

fn rrf_fuse_hits(primary: &[ContextHit], secondary: &[ContextHit], k: usize) -> Vec<ContextHit> {
    let mut score_map = std::collections::HashMap::<String, f32>::new();
    let mut hit_map = std::collections::HashMap::<String, ContextHit>::new();

    for (rank, hit) in primary.iter().enumerate() {
        let score = 1.0 / (k + rank + 1) as f32;
        *score_map.entry(hit.uri.clone()).or_insert(0.0) += score;
        hit_map
            .entry(hit.uri.clone())
            .or_insert_with(|| hit.clone());
    }
    for (rank, hit) in secondary.iter().enumerate() {
        let score = 1.0 / (k + rank + 1) as f32;
        *score_map.entry(hit.uri.clone()).or_insert(0.0) += score;
        hit_map
            .entry(hit.uri.clone())
            .and_modify(|current| {
                if hit.score > current.score {
                    *current = hit.clone();
                }
            })
            .or_insert_with(|| hit.clone());
    }

    score_map
        .into_iter()
        .filter_map(|(uri, fused_score)| {
            hit_map.remove(&uri).map(|mut hit| {
                hit.score = fused_score;
                hit
            })
        })
        .collect()
}

fn sync_trace_final_topk(result: &mut FindResult) {
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

fn split_hits(hits: &[ContextHit]) -> (Vec<ContextHit>, Vec<ContextHit>, Vec<ContextHit>) {
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

fn append_query_plan_note(result: &mut FindResult, note: &str) {
    let Some(object) = result.query_plan.as_object_mut() else {
        result.query_plan = json!({"notes": [note]});
        return;
    };
    let notes = object.entry("notes").or_insert_with(|| json!([]));
    if let Some(array) = notes.as_array_mut() {
        array.push(json!(note));
    }
}

fn annotate_trace_relation_metrics(result: &mut FindResult) {
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

#[cfg(test)]
mod backend_tests {
    use chrono::Utc;
    use serde_json::json;
    use tempfile::tempdir;

    use crate::models::{ContextHit, FindResult, IndexRecord, SearchBudget, SearchOptions};
    use crate::qdrant::{QdrantConfig, QdrantMirror};

    use super::{
        AxiomMe, RerankerMode, RetrievalBackendMode, apply_backend_hits, resolve_reranker_mode,
        resolve_retrieval_backend_mode,
    };

    #[test]
    fn retrieval_backend_mode_parser_defaults_to_sqlite() {
        assert_eq!(
            resolve_retrieval_backend_mode(None),
            RetrievalBackendMode::Sqlite
        );
        assert_eq!(
            resolve_retrieval_backend_mode(Some("unknown".to_string())),
            RetrievalBackendMode::Sqlite
        );
    }

    #[test]
    fn retrieval_backend_mode_parser_accepts_supported_backends() {
        assert_eq!(
            resolve_retrieval_backend_mode(Some("sqlite".to_string())),
            RetrievalBackendMode::Sqlite
        );
        assert_eq!(
            resolve_retrieval_backend_mode(Some("BM25".to_string())),
            RetrievalBackendMode::Sqlite
        );
        assert_eq!(
            resolve_retrieval_backend_mode(Some("qdrant".to_string())),
            RetrievalBackendMode::Qdrant
        );
        assert_eq!(
            resolve_retrieval_backend_mode(Some("HYBRID".to_string())),
            RetrievalBackendMode::Hybrid
        );
    }

    #[test]
    fn reranker_mode_parser_defaults_to_doc_aware() {
        assert_eq!(resolve_reranker_mode(None), RerankerMode::DocAwareV1);
        assert_eq!(
            resolve_reranker_mode(Some("unknown".to_string())),
            RerankerMode::DocAwareV1
        );
        assert_eq!(
            resolve_reranker_mode(Some("doc-aware".to_string())),
            RerankerMode::DocAwareV1
        );
        assert_eq!(
            resolve_reranker_mode(Some("OFF".to_string())),
            RerankerMode::Off
        );
    }

    #[test]
    fn search_with_budget_propagates_budget_notes() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        let root = IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources".to_string(),
            parent_uri: None,
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "resources".to_string(),
            abstract_text: "resources root".to_string(),
            content: "resources root".to_string(),
            tags: Vec::new(),
            updated_at: Utc::now(),
            depth: 0,
        };
        let leaf = IndexRecord {
            id: "leaf".to_string(),
            uri: "axiom://resources/auth.md".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "auth.md".to_string(),
            abstract_text: "OAuth".to_string(),
            content: "oauth authorization code flow".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 1,
        };

        {
            let mut index = app.index.write().expect("index write");
            index.upsert(root.clone());
            index.upsert(leaf.clone());
        }
        app.state
            .upsert_search_document(&root)
            .expect("upsert root");
        app.state
            .upsert_search_document(&leaf)
            .expect("upsert leaf");

        let result = app
            .search_with_budget(
                "oauth",
                Some("axiom://resources"),
                None,
                Some(5),
                None,
                None,
                Some(SearchBudget {
                    max_ms: None,
                    max_nodes: Some(1),
                    max_depth: Some(3),
                }),
            )
            .expect("search with budget");

        let notes = result
            .query_plan
            .get("notes")
            .and_then(|value| value.as_array())
            .expect("notes");
        assert!(
            notes
                .iter()
                .filter_map(|x| x.as_str())
                .any(|x| x == "budget_nodes:1")
        );
        assert!(
            notes
                .iter()
                .filter_map(|x| x.as_str())
                .any(|x| x == "budget_depth:3")
        );
    }

    #[test]
    fn sqlite_backend_reads_state_without_memory_index() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        let record = IndexRecord {
            id: "sqlite-only".to_string(),
            uri: "axiom://resources/sqlite-only.md".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "sqlite-only.md".to_string(),
            abstract_text: "sqlite only".to_string(),
            content: "bm25 sqlite fts".to_string(),
            tags: vec!["sqlite".to_string()],
            updated_at: Utc::now(),
            depth: 1,
        };
        app.state
            .upsert_search_document(&record)
            .expect("upsert search document");

        let result = app
            .run_retrieval_with_backend_mode(
                SearchOptions {
                    query: "sqlite".to_string(),
                    target_uri: Some(
                        crate::uri::AxiomUri::parse("axiom://resources").expect("target parse"),
                    ),
                    session: None,
                    session_hints: Vec::new(),
                    budget: None,
                    limit: 5,
                    score_threshold: None,
                    filter: None,
                    request_type: "search".to_string(),
                },
                RetrievalBackendMode::Sqlite,
            )
            .expect("sqlite retrieval");

        assert!(
            result
                .query_results
                .iter()
                .any(|x| x.uri == "axiom://resources/sqlite-only.md")
        );
    }

    #[test]
    fn apply_backend_hits_replaces_results_for_qdrant_mode() {
        let mut result = sample_find_result(vec![hit("axiom://resources/a", 0.8)]);
        apply_backend_hits(&mut result, vec![hit("axiom://resources/b", 0.9)], false, 1);

        assert_eq!(result.query_results.len(), 1);
        assert_eq!(result.query_results[0].uri, "axiom://resources/b");
    }

    #[test]
    fn apply_backend_hits_merges_results_for_hybrid_mode() {
        let mut result = sample_find_result(vec![
            hit("axiom://resources/a", 0.8),
            hit("axiom://resources/b", 0.6),
            hit("axiom://resources/d", 0.1),
        ]);
        apply_backend_hits(
            &mut result,
            vec![
                hit("axiom://resources/b", 0.95),
                hit("axiom://resources/c", 0.7),
            ],
            true,
            3,
        );

        assert_eq!(result.query_results[0].uri, "axiom://resources/b");
        assert!(
            result
                .query_results
                .iter()
                .any(|x| x.uri == "axiom://resources/c")
        );
        assert!(
            !result
                .query_results
                .iter()
                .any(|x| x.uri == "axiom://resources/d")
        );
    }

    #[test]
    fn apply_backend_hits_respects_requested_limit_not_memory_count() {
        let mut result = sample_find_result(vec![hit("axiom://resources/a", 0.1)]);
        apply_backend_hits(
            &mut result,
            vec![
                hit("axiom://resources/b", 0.9),
                hit("axiom://resources/c", 0.8),
                hit("axiom://resources/d", 0.7),
            ],
            false,
            3,
        );

        assert_eq!(result.query_results.len(), 3);
        assert_eq!(result.query_results[0].uri, "axiom://resources/b");
    }

    #[test]
    fn qdrant_mode_fallback_records_dead_letter_event() {
        let temp = tempdir().expect("tempdir");
        let mut app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        {
            let mut index = app.index.write().expect("index write");
            index.upsert(IndexRecord {
                id: "test-1".to_string(),
                uri: "axiom://resources/backend-fallback/auth.md".to_string(),
                parent_uri: Some("axiom://resources/backend-fallback".to_string()),
                is_leaf: true,
                context_type: "resource".to_string(),
                name: "auth.md".to_string(),
                abstract_text: "OAuth flow".to_string(),
                content: "oauth authorization code".to_string(),
                tags: vec!["auth".to_string()],
                updated_at: Utc::now(),
                depth: 3,
            });
        }

        app.qdrant = Some(
            QdrantMirror::new(QdrantConfig {
                base_url: "http://127.0.0.1:9".to_string(),
                api_key: None,
                collection: "axiomme_l0".to_string(),
                timeout_ms: 30,
            })
            .expect("qdrant mirror"),
        );

        let result = app
            .run_retrieval_with_backend_mode(
                SearchOptions {
                    query: "oauth".to_string(),
                    target_uri: None,
                    session: None,
                    session_hints: Vec::new(),
                    budget: None,
                    limit: 5,
                    score_threshold: None,
                    filter: None,
                    request_type: "search".to_string(),
                },
                RetrievalBackendMode::Qdrant,
            )
            .expect("fallback result");

        assert!(!result.query_results.is_empty());
        let notes = result.query_plan["notes"].as_array().expect("notes");
        assert!(
            notes
                .iter()
                .filter_map(|x| x.as_str())
                .any(|x| x.starts_with("backend_fallback:memory("))
        );

        let dead = app.state.fetch_outbox("dead_letter", 50).expect("outbox");
        assert!(
            dead.iter()
                .any(|event| event.event_type == "qdrant_search_failed")
        );
    }

    #[test]
    fn doc_aware_reranker_prioritizes_config_documents() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        let config = IndexRecord {
            id: "cfg-1".to_string(),
            uri: "axiom://resources/app/settings.toml".to_string(),
            parent_uri: Some("axiom://resources/app".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "settings.toml".to_string(),
            abstract_text: "runtime settings".to_string(),
            content: "database_url and retry limits".to_string(),
            tags: vec!["config".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };
        let guide = IndexRecord {
            id: "guide-1".to_string(),
            uri: "axiom://resources/app/guide.md".to_string(),
            parent_uri: Some("axiom://resources/app".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "guide.md".to_string(),
            abstract_text: "developer guide".to_string(),
            content: "overview and onboarding".to_string(),
            tags: vec!["markdown".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };
        {
            let mut index = app.index.write().expect("index write");
            index.upsert(config);
            index.upsert(guide);
        }

        let mut result = sample_find_result(vec![
            hit("axiom://resources/app/guide.md", 0.92),
            hit("axiom://resources/app/settings.toml", 0.86),
        ]);
        app.apply_reranker_with_mode(
            "config env settings",
            &mut result,
            2,
            RerankerMode::DocAwareV1,
        )
        .expect("rerank");

        assert_eq!(
            result.query_results[0].uri,
            "axiom://resources/app/settings.toml"
        );
        let notes = result.query_plan["notes"].as_array().expect("notes");
        assert!(
            notes
                .iter()
                .filter_map(|x| x.as_str())
                .any(|x| x == "reranker:doc-aware-v1")
        );
    }

    fn hit(uri: &str, score: f32) -> ContextHit {
        ContextHit {
            uri: uri.to_string(),
            score,
            abstract_text: String::new(),
            context_type: "resource".to_string(),
            relations: Vec::new(),
        }
    }

    fn sample_find_result(hits: Vec<ContextHit>) -> FindResult {
        FindResult {
            memories: Vec::new(),
            resources: hits.clone(),
            skills: Vec::new(),
            query_plan: json!({"notes": []}),
            query_results: hits,
            trace: None,
            trace_uri: None,
        }
    }
}
