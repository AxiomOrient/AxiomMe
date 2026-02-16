use std::time::Instant;

use chrono::Utc;
use serde_json::json;

use crate::config::RetrievalBackend;
use crate::error::{AxiomError, Result};
use crate::models::{ContextHit, RequestLogEntry, SearchOptions};

use super::AxiomMe;
use super::reranker::resolve_reranker_mode;
use super::result::{append_query_plan_note, build_sqlite_result};

impl AxiomMe {
    pub(super) fn run_retrieval_with_backend_mode(
        &self,
        options: &SearchOptions,
        backend: RetrievalBackend,
    ) -> Result<crate::models::FindResult> {
        let requested_limit = options.limit.max(1);
        let mut result = match backend {
            RetrievalBackend::Sqlite => match self.run_sqlite_retrieval(options) {
                Ok(mut sqlite_result) => {
                    append_query_plan_note(&mut sqlite_result, "backend:sqlite");
                    if let Some(threshold) = options.score_threshold {
                        append_query_plan_note(
                            &mut sqlite_result,
                            &format!("score_threshold:{threshold:.3}"),
                        );
                    }
                    if let Some(min_match_tokens) =
                        options.min_match_tokens.filter(|value| *value > 1)
                    {
                        append_query_plan_note(
                            &mut sqlite_result,
                            &format!("min_match_tokens:{min_match_tokens}"),
                        );
                    }
                    sqlite_result
                }
                Err(err) => {
                    let mut memory_result = self.run_memory_retrieval(options)?;
                    self.record_sqlite_fallback(&err);
                    append_query_plan_note(
                        &mut memory_result,
                        &format!("backend_fallback:memory({})", err.code()),
                    );
                    append_query_plan_note(&mut memory_result, "backend:memory");
                    memory_result
                }
            },
            RetrievalBackend::Memory => {
                let mut memory_result = self.run_memory_retrieval(options)?;
                append_query_plan_note(&mut memory_result, "backend:memory");
                memory_result
            }
        };

        let reranker_mode = resolve_reranker_mode(self.config.search.reranker.as_deref());
        self.apply_reranker_with_mode(&options.query, &mut result, requested_limit, reranker_mode)?;
        Ok(result)
    }

    fn run_memory_retrieval(&self, options: &SearchOptions) -> Result<crate::models::FindResult> {
        let mut memory_result = {
            let index = self
                .index
                .read()
                .map_err(|_| AxiomError::lock_poisoned("index"))?;
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

    fn run_sqlite_retrieval(&self, options: &SearchOptions) -> Result<crate::models::FindResult> {
        let started = Instant::now();
        let target_prefix = options.target_uri.as_ref().map(ToString::to_string);
        let effective_query = if options.session_hints.is_empty() {
            options.query.clone()
        } else {
            format!("{} {}", options.query, options.session_hints.join(" "))
        };

        if matches!(options.budget.as_ref().and_then(|x| x.max_ms), Some(0)) {
            return Ok(build_sqlite_result(
                options,
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
            self.config.search.query_normalizer_enabled,
            effective_limit,
            options.min_match_tokens,
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
            options,
            mapped_hits,
            stop_reason,
            started.elapsed().as_millis(),
        ))
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

        self.try_log_request(&RequestLogEntry {
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
