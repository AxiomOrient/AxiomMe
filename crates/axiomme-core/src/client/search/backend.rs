use crate::config::QUERY_PLAN_BACKEND_POLICY_MEMORY_ONLY;
use crate::error::{AxiomError, Result};
use crate::models::SearchOptions;

use super::AxiomMe;
use super::reranker::resolve_reranker_mode;
use super::result::append_query_plan_note;

impl AxiomMe {
    pub(super) fn run_retrieval_memory_only(
        &self,
        options: &SearchOptions,
    ) -> Result<crate::models::FindResult> {
        let requested_limit = options.limit.max(1);
        let mut result = self.run_memory_retrieval(options)?;
        append_query_plan_note(&mut result, "backend:memory");
        append_query_plan_note(&mut result, QUERY_PLAN_BACKEND_POLICY_MEMORY_ONLY);
        if let Some(threshold) = options.score_threshold {
            append_query_plan_note(&mut result, &format!("score_threshold:{threshold:.3}"));
        }
        if let Some(min_match_tokens) = options.min_match_tokens.filter(|value| *value > 1) {
            append_query_plan_note(&mut result, &format!("min_match_tokens:{min_match_tokens}"));
        }

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
}
