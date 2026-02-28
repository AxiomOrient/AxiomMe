use chrono::Utc;

use crate::embedding::{embed_text, tokenize_set};
use crate::models::SearchFilter;
use crate::uri::AxiomUri;

use super::exact::{ExactQueryKeys, exact_match_score};
use super::rank::{
    LexicalCorpusView, LexicalDocView, cosine, exact_confidence_bonus, lexical_score, path_score,
    recency_score, score_ordering, uri_path_prefix_match,
};
use super::{InMemoryIndex, ScoredRecord};

impl InMemoryIndex {
    pub fn search(
        &self,
        query: &str,
        target_uri: Option<&AxiomUri>,
        limit: usize,
        score_threshold: Option<f32>,
        filter: Option<&SearchFilter>,
    ) -> Vec<ScoredRecord> {
        let exact_query = ExactQueryKeys::from_query(query);
        let q_embed = embed_text(query);
        let q_tokens = tokenize_set(query);
        let q_token_list = crate::embedding::tokenize_vec(query);
        let query_lower = query.to_lowercase();
        let target_uri_text = target_uri.map(AxiomUri::to_string_uri);
        let target_scope_root =
            target_uri.map(|target| format!("axiom://{}", target.scope().as_str()));
        let avg_doc_length = if self.records.is_empty() {
            1.0
        } else {
            (super::usize_to_f32(self.total_doc_length) / super::usize_to_f32(self.records.len()))
                .max(1.0)
        };
        let filter_projection = self.filter_projection_uris(filter);
        let now = Utc::now();

        let mut scored = Vec::new();
        for (arc_uri, record) in self.records.iter() {
            if let Some(target) = target_uri_text.as_deref()
                && !uri_path_prefix_match(&record.uri, target)
            {
                continue;
            }
            if let Some(allowed_uris) = filter_projection.as_ref()
                && !allowed_uris.contains(record.uri.as_str())
            {
                continue;
            }

            let uri = record.uri.as_str();
            let dense = cosine(&q_embed, self.vectors.get(uri).map_or(&[], Vec::as_slice));
            let sparse = lexical_score(
                &q_token_list,
                &q_tokens,
                &query_lower,
                LexicalDocView {
                    term_freq: self.term_freqs.get(uri),
                    token_set: self.token_sets.get(uri),
                    text_lower: self.raw_text_lower.get(uri).map(String::as_str),
                    doc_len: self.doc_lengths.get(uri).copied().unwrap_or(0),
                },
                LexicalCorpusView {
                    doc_freqs: &self.doc_freqs,
                    total_docs: self.records.len(),
                    avg_doc_len: avg_doc_length,
                },
            );
            let recency = recency_score(now, record.updated_at);
            let path = path_score(
                uri,
                target_uri_text.as_deref(),
                target_scope_root.as_deref(),
            );
            let exact = exact_match_score(&exact_query, self.exact_keys.get(uri), record);
            let exact_component = super::W_EXACT.mul_add(
                exact,
                super::W_EXACT_HIGH_CONF_BOOST * exact * exact * exact,
            );
            let exact_bonus = exact_confidence_bonus(exact);

            let score = exact_bonus
                + super::W_PATH.mul_add(
                    path,
                    super::W_RECENCY.mul_add(
                        recency,
                        super::W_SPARSE
                            .mul_add(sparse, super::W_DENSE.mul_add(dense, exact_component)),
                    ),
                );
            if let Some(threshold) = score_threshold
                && score < threshold
            {
                continue;
            }

            scored.push(ScoredRecord {
                uri: arc_uri.clone(),
                is_leaf: record.is_leaf,
                depth: record.depth,
                exact,
                dense,
                sparse,
                recency,
                path,
                score,
            });
        }

        scored.sort_by(score_ordering);
        scored.truncate(limit);
        scored
    }

    pub fn search_directories(
        &self,
        query: &str,
        target_uri: Option<&AxiomUri>,
        limit: usize,
        filter: Option<&SearchFilter>,
    ) -> Vec<ScoredRecord> {
        let mut out = self
            .search(
                query,
                target_uri,
                limit.saturating_mul(4).max(20),
                None,
                filter,
            )
            .into_iter()
            .filter(|score| !score.is_leaf)
            .collect::<Vec<_>>();
        out.sort_by(score_ordering);
        out.truncate(limit);
        out
    }
}
