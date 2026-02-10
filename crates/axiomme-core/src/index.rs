use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use crate::embedding::{embed_text, tokenize_set};
use crate::models::{IndexRecord, SearchFilter};
use crate::uri::{AxiomUri, Scope};

const W_DENSE: f32 = 0.55;
const W_SPARSE: f32 = 0.30;
const W_RECENCY: f32 = 0.10;
const W_PATH: f32 = 0.05;

#[derive(Debug, Clone)]
pub struct ScoredRecord {
    pub record: IndexRecord,
    pub dense: f32,
    pub sparse: f32,
    pub recency: f32,
    pub path: f32,
    pub score: f32,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryHybridIndex {
    records: HashMap<String, IndexRecord>,
    vectors: HashMap<String, Vec<f32>>,
    token_sets: HashMap<String, HashSet<String>>,
    term_freqs: HashMap<String, HashMap<String, u32>>,
    doc_lengths: HashMap<String, usize>,
    doc_freqs: HashMap<String, usize>,
    raw_text_lower: HashMap<String, String>,
    total_doc_length: usize,
}

impl InMemoryHybridIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, record: IndexRecord) {
        let key = record.uri.clone();
        if self.records.contains_key(&key) {
            self.remove_lexical_stats(&key);
        }
        let text = [
            record.name.as_str(),
            record.abstract_text.as_str(),
            record.content.as_str(),
            &record.tags.join(" "),
        ]
        .join(" ");
        let text_lower = text.to_lowercase();
        let tokens = crate::embedding::tokenize_vec(&text);
        let mut term_freq = HashMap::new();
        for token in tokens {
            *term_freq.entry(token).or_insert(0) += 1;
        }
        for token in term_freq.keys() {
            *self.doc_freqs.entry(token.clone()).or_insert(0) += 1;
        }
        let doc_len = term_freq.values().map(|x| *x as usize).sum::<usize>();
        self.total_doc_length += doc_len;
        self.doc_lengths.insert(key.clone(), doc_len);
        self.token_sets
            .insert(key.clone(), term_freq.keys().cloned().collect());
        self.term_freqs.insert(key.clone(), term_freq);
        self.raw_text_lower.insert(key.clone(), text_lower);
        self.vectors.insert(key.clone(), embed_text(&text));
        self.records.insert(key, record);
    }

    pub fn remove(&mut self, uri: &str) {
        self.records.remove(uri);
        self.vectors.remove(uri);
        self.remove_lexical_stats(uri);
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.vectors.clear();
        self.token_sets.clear();
        self.term_freqs.clear();
        self.doc_lengths.clear();
        self.doc_freqs.clear();
        self.raw_text_lower.clear();
        self.total_doc_length = 0;
    }

    pub fn get(&self, uri: &str) -> Option<&IndexRecord> {
        self.records.get(uri)
    }

    pub fn all_records(&self) -> Vec<IndexRecord> {
        let mut out: Vec<_> = self.records.values().cloned().collect();
        out.sort_by(|a, b| a.uri.cmp(&b.uri));
        out
    }

    pub fn children_of(&self, parent_uri: &str) -> Vec<IndexRecord> {
        let mut out: Vec<_> = self
            .records
            .values()
            .filter(|r| r.parent_uri.as_deref() == Some(parent_uri))
            .cloned()
            .collect();
        out.sort_by(|a, b| a.uri.cmp(&b.uri));
        out
    }

    pub fn search(
        &self,
        query: &str,
        target_uri: Option<&AxiomUri>,
        limit: usize,
        score_threshold: Option<f32>,
        filter: Option<&SearchFilter>,
    ) -> Vec<ScoredRecord> {
        let q_embed = embed_text(query);
        let q_tokens = tokenize_set(query);
        let q_token_list = crate::embedding::tokenize_vec(query);
        let query_lower = query.to_lowercase();
        let avg_doc_length = if self.records.is_empty() {
            1.0
        } else {
            (self.total_doc_length as f32 / self.records.len() as f32).max(1.0)
        };

        let mut scored = Vec::new();
        for record in self.records.values() {
            if let Some(target) = target_uri
                && let Ok(record_uri) = AxiomUri::parse(&record.uri)
                && !record_uri.starts_with(target)
            {
                continue;
            }
            if !self.record_matches_filter(record, filter) {
                continue;
            }

            let uri = &record.uri;
            let dense = cosine(
                &q_embed,
                self.vectors.get(uri).map(Vec::as_slice).unwrap_or(&[]),
            );
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
            let recency = recency_score(record.updated_at);
            let path = path_score(uri, target_uri);

            let score = W_DENSE * dense + W_SPARSE * sparse + W_RECENCY * recency + W_PATH * path;
            if let Some(threshold) = score_threshold
                && score < threshold
            {
                continue;
            }

            scored.push(ScoredRecord {
                record: record.clone(),
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
            .filter(|s| !s.record.is_leaf)
            .collect::<Vec<_>>();
        out.sort_by(score_ordering);
        out.truncate(limit);
        out
    }

    pub fn record_matches_filter(
        &self,
        record: &IndexRecord,
        filter: Option<&SearchFilter>,
    ) -> bool {
        let Some(filter) = normalize_filter(filter) else {
            return true;
        };

        if record.is_leaf {
            return leaf_matches_filter(record, &filter);
        }

        self.has_matching_leaf_descendant(&record.uri, &filter)
    }

    pub fn scope_roots(&self, scopes: &[Scope]) -> Vec<IndexRecord> {
        let mut roots = Vec::new();
        for scope in scopes {
            let uri = format!("axiom://{}", scope.as_str());
            if let Some(rec) = self.get(&uri) {
                roots.push(rec.clone());
            }
        }
        roots
    }

    fn remove_lexical_stats(&mut self, uri: &str) {
        if let Some(term_freq) = self.term_freqs.remove(uri) {
            for token in term_freq.keys() {
                if let Some(df) = self.doc_freqs.get_mut(token) {
                    *df = df.saturating_sub(1);
                    if *df == 0 {
                        self.doc_freqs.remove(token);
                    }
                }
            }
        }
        if let Some(doc_len) = self.doc_lengths.remove(uri) {
            self.total_doc_length = self.total_doc_length.saturating_sub(doc_len);
        }
        self.token_sets.remove(uri);
        self.raw_text_lower.remove(uri);
    }

    fn has_matching_leaf_descendant(&self, ancestor_uri: &str, filter: &NormalizedFilter) -> bool {
        let prefix = format!("{}/", ancestor_uri);
        self.records.values().any(|record| {
            record.is_leaf && record.uri.starts_with(&prefix) && leaf_matches_filter(record, filter)
        })
    }
}

#[derive(Debug)]
struct NormalizedFilter {
    tags: Vec<String>,
    mime: Option<String>,
}

fn normalize_filter(filter: Option<&SearchFilter>) -> Option<NormalizedFilter> {
    let filter = filter?;
    let tags = filter
        .tags
        .iter()
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>();
    let mime = filter
        .mime
        .as_ref()
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty());
    if tags.is_empty() && mime.is_none() {
        return None;
    }
    Some(NormalizedFilter { tags, mime })
}

fn leaf_matches_filter(record: &IndexRecord, filter: &NormalizedFilter) -> bool {
    if !filter.tags.is_empty()
        && !filter.tags.iter().all(|wanted| {
            record
                .tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(wanted))
        })
    {
        return false;
    }

    if let Some(required_mime) = &filter.mime {
        let Some(record_mime) = infer_mime(record) else {
            return false;
        };
        if !record_mime.eq_ignore_ascii_case(required_mime) {
            return false;
        }
    }

    true
}

fn infer_mime(record: &IndexRecord) -> Option<&'static str> {
    let ext = record.name.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some("text/markdown"),
        "txt" | "log" => Some("text/plain"),
        "json" => Some("application/json"),
        "rs" => Some("text/rust"),
        _ => None,
    }
}

fn score_ordering(a: &ScoredRecord, b: &ScoredRecord) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.record.uri.cmp(&b.record.uri))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let len = a.len().min(b.len());
    let mut sum = 0.0;
    for i in 0..len {
        sum += a[i] * b[i];
    }
    sum
}

fn lexical_overlap(query_tokens: &HashSet<String>, doc_tokens: &HashSet<String>) -> f32 {
    if query_tokens.is_empty() || doc_tokens.is_empty() {
        return 0.0;
    }
    let inter = query_tokens.intersection(doc_tokens).count() as f32;
    let union = query_tokens.union(doc_tokens).count() as f32;
    if union == 0.0 { 0.0 } else { inter / union }
}

fn lexical_score(
    query_token_list: &[String],
    query_tokens: &HashSet<String>,
    query_lower: &str,
    doc: LexicalDocView<'_>,
    corpus: LexicalCorpusView<'_>,
) -> f32 {
    let overlap = doc
        .token_set
        .map(|tokens| lexical_overlap(query_tokens, tokens))
        .unwrap_or(0.0);
    let bm25_raw = doc
        .term_freq
        .map(|tf| {
            bm25_score(
                query_token_list,
                tf,
                doc.doc_len,
                corpus.doc_freqs,
                corpus.total_docs,
                corpus.avg_doc_len,
            )
        })
        .unwrap_or(0.0);
    let bm25_norm = bm25_raw / (bm25_raw + 2.0);
    let literal = literal_match_score(query_lower, doc.text_lower.unwrap_or_default());
    (0.65 * bm25_norm + 0.25 * overlap + 0.10 * literal).clamp(0.0, 1.0)
}

struct LexicalDocView<'a> {
    term_freq: Option<&'a HashMap<String, u32>>,
    token_set: Option<&'a HashSet<String>>,
    text_lower: Option<&'a str>,
    doc_len: usize,
}

struct LexicalCorpusView<'a> {
    doc_freqs: &'a HashMap<String, usize>,
    total_docs: usize,
    avg_doc_len: f32,
}

fn bm25_score(
    query_tokens: &[String],
    doc_term_freq: &HashMap<String, u32>,
    doc_len: usize,
    doc_freqs: &HashMap<String, usize>,
    total_docs: usize,
    avg_doc_len: f32,
) -> f32 {
    if query_tokens.is_empty() || doc_term_freq.is_empty() || doc_len == 0 || total_docs == 0 {
        return 0.0;
    }

    const K1: f32 = 1.2;
    const B: f32 = 0.75;

    let mut score = 0.0;
    let mut seen = HashSet::new();
    for token in query_tokens {
        if !seen.insert(token) {
            continue;
        }
        let Some(tf) = doc_term_freq.get(token) else {
            continue;
        };
        let df = *doc_freqs.get(token).unwrap_or(&0) as f32;
        let n = total_docs as f32;
        let idf = (((n - df + 0.5) / (df + 0.5)) + 1.0).ln().max(0.0);
        let tf = *tf as f32;
        let denom = tf + K1 * (1.0 - B + B * (doc_len as f32 / avg_doc_len.max(1.0)));
        if denom > 0.0 {
            score += idf * (tf * (K1 + 1.0) / denom);
        }
    }
    score
}

fn literal_match_score(query_lower: &str, doc_text_lower: &str) -> f32 {
    let q = query_lower.trim();
    if q.len() < 3 {
        return 0.0;
    }
    if doc_text_lower.contains(q) { 1.0 } else { 0.0 }
}

fn recency_score(updated_at: DateTime<Utc>) -> f32 {
    let age_days = (Utc::now() - updated_at).num_days().max(0) as f32;
    (1.0 / (1.0 + age_days / 30.0)).clamp(0.0, 1.0)
}

fn path_score(uri: &str, target: Option<&AxiomUri>) -> f32 {
    let Some(target) = target else {
        return 0.0;
    };
    let Ok(candidate) = AxiomUri::parse(uri) else {
        return 0.0;
    };

    if candidate == *target {
        return 1.0;
    }

    if candidate.starts_with(target) {
        return 0.8;
    }

    if target.starts_with(&candidate) {
        return 0.6;
    }

    if candidate.scope() == target.scope() {
        return 0.2;
    }

    0.0
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::models::SearchFilter;

    #[test]
    fn search_prioritizes_matching_doc() {
        let mut index = InMemoryHybridIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/docs/auth".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "auth".to_string(),
            abstract_text: "OAuth flow documentation".to_string(),
            content: "oauth authorization code flow".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/docs/storage".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "storage".to_string(),
            abstract_text: "Storage docs".to_string(),
            content: "disk and iops".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("oauth flow", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").record.uri,
            "axiom://resources/docs/auth"
        );
    }

    #[test]
    fn lexical_exact_match_boost_prioritizes_literal_query() {
        let mut index = InMemoryHybridIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/logs/exact".to_string(),
            parent_uri: Some("axiom://resources/logs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "exact.log".to_string(),
            abstract_text: "Exact compiler error trace".to_string(),
            content: "error[E0425]: cannot find value `foo` in this scope".to_string(),
            tags: vec!["error".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/logs/near".to_string(),
            parent_uri: Some("axiom://resources/logs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "near.log".to_string(),
            abstract_text: "Similar error guidance".to_string(),
            content: "cannot find value in this scope; example shows E0425 and foo notes"
                .to_string(),
            tags: vec!["error".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let query = "error[E0425]: cannot find value `foo` in this scope";
        let result = index.search(query, None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").record.uri,
            "axiom://resources/logs/exact"
        );
        assert!(result.first().expect("no result").sparse >= result[1].sparse);
    }

    #[test]
    fn tag_filter_limits_leaf_results() {
        let mut index = InMemoryHybridIndex::new();
        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources/docs".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "docs".to_string(),
            abstract_text: "docs".to_string(),
            content: "".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 1,
        });
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/docs/auth.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "auth.md".to_string(),
            abstract_text: "auth".to_string(),
            content: "oauth flow".to_string(),
            tags: vec!["auth".to_string(), "markdown".to_string()],
            updated_at: Utc::now(),
            depth: 2,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/docs/storage.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "storage.md".to_string(),
            abstract_text: "storage".to_string(),
            content: "disk IOPS".to_string(),
            tags: vec!["storage".to_string(), "markdown".to_string()],
            updated_at: Utc::now(),
            depth: 2,
        });

        let filter = SearchFilter {
            tags: vec!["auth".to_string()],
            mime: None,
        };
        let result = index.search("docs", None, 20, None, Some(&filter));
        assert!(
            result
                .iter()
                .any(|x| x.record.uri == "axiom://resources/docs/auth.md")
        );
        assert!(
            !result
                .iter()
                .any(|x| x.record.uri == "axiom://resources/docs/storage.md")
        );
    }

    #[test]
    fn mime_filter_matches_extension_derived_mime() {
        let mut index = InMemoryHybridIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/docs/guide.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "guide.md".to_string(),
            abstract_text: "guide".to_string(),
            content: "getting started".to_string(),
            tags: vec!["markdown".to_string()],
            updated_at: Utc::now(),
            depth: 2,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/docs/schema.json".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "schema.json".to_string(),
            abstract_text: "schema".to_string(),
            content: "{\"a\":1}".to_string(),
            tags: vec!["json".to_string()],
            updated_at: Utc::now(),
            depth: 2,
        });

        let filter = SearchFilter {
            tags: vec![],
            mime: Some("text/markdown".to_string()),
        };
        let result = index.search("schema guide", None, 20, None, Some(&filter));
        assert!(result.iter().any(|x| x.record.uri.ends_with("guide.md")));
        assert!(!result.iter().any(|x| x.record.uri.ends_with("schema.json")));
    }
}
