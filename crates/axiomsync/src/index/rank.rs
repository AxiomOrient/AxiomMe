use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use super::ScoredRecord;

const EXACT_BONUS_HIGH: f32 = 0.35;
const EXACT_BONUS_MEDIUM: f32 = 0.22;
const EXACT_BONUS_LOW: f32 = 0.10;
const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;

pub(super) fn score_ordering(a: &ScoredRecord, b: &ScoredRecord) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| b.exact.partial_cmp(&a.exact).unwrap_or(Ordering::Equal))
        .then_with(|| a.uri.cmp(&b.uri))
}

pub(super) fn exact_confidence_bonus(exact: f32) -> f32 {
    if exact >= 0.90 {
        return EXACT_BONUS_HIGH;
    }
    if exact >= 0.82 {
        return EXACT_BONUS_MEDIUM;
    }
    if exact >= 0.70 {
        return EXACT_BONUS_LOW;
    }
    0.0
}

pub(super) fn cosine(a: &[f32], b: &[f32]) -> f32 {
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
    let inter = usize_to_f32(query_tokens.intersection(doc_tokens).count());
    let union = usize_to_f32(query_tokens.union(doc_tokens).count());
    if union == 0.0 { 0.0 } else { inter / union }
}

pub(super) fn lexical_score(
    query_token_list: &[String],
    query_tokens: &HashSet<String>,
    query_lower: &str,
    doc: LexicalDocView<'_>,
    corpus: LexicalCorpusView<'_>,
) -> f32 {
    let overlap = doc
        .token_set
        .map_or(0.0, |tokens| lexical_overlap(query_tokens, tokens));
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
        .unwrap_or_default();
    let bm25_norm = bm25_raw / (bm25_raw + 2.0);
    let literal = literal_match_score(query_lower, doc.text_lower.unwrap_or_default());
    0.10f32
        .mul_add(literal, 0.25f32.mul_add(overlap, 0.65f32 * bm25_norm))
        .clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LexicalDocView<'a> {
    pub(super) term_freq: Option<&'a HashMap<String, u32>>,
    pub(super) token_set: Option<&'a HashSet<String>>,
    pub(super) text_lower: Option<&'a str>,
    pub(super) doc_len: usize,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LexicalCorpusView<'a> {
    pub(super) doc_freqs: &'a HashMap<String, usize>,
    pub(super) total_docs: usize,
    pub(super) avg_doc_len: f32,
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

    let mut score = 0.0;
    let mut seen = HashSet::new();
    for token in query_tokens {
        if !seen.insert(token) {
            continue;
        }
        let Some(tf) = doc_term_freq.get(token) else {
            continue;
        };
        let df = usize_to_f32(*doc_freqs.get(token).unwrap_or(&0));
        let n = usize_to_f32(total_docs);
        let idf_ratio = (n - df + 0.5) / (df + 0.5);
        let idf = idf_ratio.ln_1p().max(0.0);
        let tf = u32_to_f32(*tf);
        let length_norm =
            BM25_B.mul_add(usize_to_f32(doc_len) / avg_doc_len.max(1.0), 1.0 - BM25_B);
        let denom = BM25_K1.mul_add(length_norm, tf);
        if denom > 0.0 {
            score += idf * (tf * (BM25_K1 + 1.0) / denom);
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

pub(super) fn recency_score(now: DateTime<Utc>, updated_at: DateTime<Utc>) -> f32 {
    let age_days = i64_to_f32((now - updated_at).num_days().max(0));
    (1.0 / (1.0 + age_days / 30.0)).clamp(0.0, 1.0)
}

pub(super) fn path_score(
    uri: &str,
    target_uri: Option<&str>,
    target_scope_root: Option<&str>,
) -> f32 {
    let Some(target_uri) = target_uri else {
        return 0.0;
    };
    if uri == target_uri {
        return 1.0;
    }

    if uri_path_prefix_match(uri, target_uri) {
        return 0.8;
    }

    if uri_path_prefix_match(target_uri, uri) {
        return 0.6;
    }

    if let Some(scope_root) = target_scope_root
        && uri_path_prefix_match(uri, scope_root)
    {
        return 0.2;
    }

    0.0
}

pub(super) fn uri_path_prefix_match(uri: &str, prefix_uri: &str) -> bool {
    uri == prefix_uri
        || (uri.starts_with(prefix_uri)
            && uri
                .as_bytes()
                .get(prefix_uri.len())
                .is_some_and(|boundary| *boundary == b'/'))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "ranking weights are intentionally lossy floating-point values"
)]
const fn usize_to_f32(value: usize) -> f32 {
    value as f32
}

#[allow(
    clippy::cast_precision_loss,
    reason = "ranking weights are intentionally lossy floating-point values"
)]
const fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

#[allow(
    clippy::cast_precision_loss,
    reason = "ranking decay operates in f32 and accepts intentional precision loss"
)]
const fn i64_to_f32(value: i64) -> f32 {
    value as f32
}
