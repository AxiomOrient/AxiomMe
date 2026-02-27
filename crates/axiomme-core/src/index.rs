use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::embedding::{embed_text, tokenize_set};
use crate::mime::infer_mime;
use crate::models::{IndexRecord, SearchFilter};
use crate::uri::{AxiomUri, Scope};

const W_EXACT: f32 = 0.42;
const W_EXACT_HIGH_CONF_BOOST: f32 = 0.20;
const W_DENSE: f32 = 0.33;
const W_SPARSE: f32 = 0.20;
const W_RECENCY: f32 = 0.03;
const W_PATH: f32 = 0.02;
const EXACT_BONUS_HIGH: f32 = 0.35;
const EXACT_BONUS_MEDIUM: f32 = 0.22;
const EXACT_BONUS_LOW: f32 = 0.10;
const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;
const MAX_EXACT_HEADING_KEYS: usize = 24;
const MAX_EXACT_CONTENT_LINE_KEYS: usize = 64;

#[derive(Debug, Clone)]
pub struct ScoredRecord {
    pub uri: Arc<str>,
    pub is_leaf: bool,
    pub depth: usize,
    pub exact: f32,
    pub dense: f32,
    pub sparse: f32,
    pub recency: f32,
    pub path: f32,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct IndexChildRecord {
    pub uri: Arc<str>,
    pub is_leaf: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy)]
struct ChildIndexEntry {
    is_leaf: bool,
    depth: usize,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryIndex {
    records: HashMap<Arc<str>, IndexRecord>,
    vectors: HashMap<Arc<str>, Vec<f32>>,
    token_sets: HashMap<Arc<str>, HashSet<String>>,
    term_freqs: HashMap<Arc<str>, HashMap<String, u32>>,
    doc_lengths: HashMap<Arc<str>, usize>,
    doc_freqs: HashMap<String, usize>,
    raw_text_lower: HashMap<Arc<str>, String>,
    exact_keys: HashMap<Arc<str>, ExactRecordKeys>,
    children_by_parent: HashMap<Arc<str>, BTreeMap<Arc<str>, ChildIndexEntry>>,
    total_doc_length: usize,
}

impl InMemoryIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, record: IndexRecord) {
        let key: Arc<str> = Arc::from(record.uri.as_str());
        let has_existing = self.records.contains_key(key.as_ref());
        let previous_parent_uri = if has_existing {
            self.records
                .get(key.as_ref())
                .and_then(|existing| existing.parent_uri.clone())
        } else {
            None
        };
        if has_existing {
            self.remove_lexical_stats(key.as_ref());
            self.remove_child_index_entry(previous_parent_uri.as_deref(), key.as_ref());
        }
        let child_entry = ChildIndexEntry {
            is_leaf: record.is_leaf,
            depth: record.depth,
        };
        let parent_uri = record.parent_uri.clone();
        let exact_keys = ExactRecordKeys::from_record(&record);
        let text = build_upsert_text(&record);
        let text_lower = text.to_lowercase();
        let tokens = crate::embedding::tokenize_vec(&text);
        let mut term_freq = HashMap::with_capacity(tokens.len());
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
        self.exact_keys.insert(key.clone(), exact_keys);
        self.vectors.insert(key.clone(), embed_text(&text));
        self.records.insert(key.clone(), record);
        self.upsert_child_index_entry(parent_uri.as_deref(), key, child_entry);
    }

    pub fn remove(&mut self, uri: &str) {
        if let Some(existing) = self.records.remove(uri) {
            self.remove_child_index_entry(existing.parent_uri.as_deref(), uri);
        }
        self.vectors.remove(uri);
        self.remove_lexical_stats(uri);
        self.exact_keys.remove(uri);
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.vectors.clear();
        self.token_sets.clear();
        self.term_freqs.clear();
        self.doc_lengths.clear();
        self.doc_freqs.clear();
        self.raw_text_lower.clear();
        self.exact_keys.clear();
        self.children_by_parent.clear();
        self.total_doc_length = 0;
    }

    #[must_use]
    pub fn get(&self, uri: &str) -> Option<&IndexRecord> {
        self.records.get(uri)
    }

    #[must_use]
    pub fn all_records(&self) -> Vec<IndexRecord> {
        let mut out: Vec<_> = self.records.values().cloned().collect();
        out.sort_by(|a, b| a.uri.cmp(&b.uri));
        out
    }

    #[must_use]
    pub fn uris_with_prefix(&self, prefix: &AxiomUri) -> Vec<String> {
        let mut out = self
            .records
            .keys()
            .filter(|uri| {
                AxiomUri::parse(uri.as_ref())
                    .map(|parsed| parsed.starts_with(prefix))
                    .unwrap_or(false)
            })
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        out.sort();
        out
    }

    #[must_use]
    pub fn children_of(&self, parent_uri: &str) -> Vec<IndexChildRecord> {
        // O(k) traversal over explicit parent->children index; no global record scan.
        let Some(children) = self.children_by_parent.get(parent_uri) else {
            return Vec::new();
        };
        let mut out = Vec::<IndexChildRecord>::with_capacity(children.len());
        for (uri, entry) in children {
            out.push(IndexChildRecord {
                uri: uri.clone(),
                is_leaf: entry.is_leaf,
                depth: entry.depth,
            });
        }
        out
    }

    #[must_use]
    pub fn token_overlap_count(&self, uri: &str, query_tokens: &HashSet<String>) -> usize {
        if query_tokens.is_empty() {
            return 0;
        }
        self.token_sets.get(uri).map_or(0, |doc_tokens| {
            query_tokens.intersection(doc_tokens).count()
        })
    }

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
            (usize_to_f32(self.total_doc_length) / usize_to_f32(self.records.len())).max(1.0)
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
            let exact_component =
                W_EXACT.mul_add(exact, W_EXACT_HIGH_CONF_BOOST * exact * exact * exact);
            let exact_bonus = exact_confidence_bonus(exact);

            let score = exact_bonus
                + W_PATH.mul_add(
                    path,
                    W_RECENCY.mul_add(
                        recency,
                        W_SPARSE.mul_add(sparse, W_DENSE.mul_add(dense, exact_component)),
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
            .filter(|s| !s.is_leaf)
            .collect::<Vec<_>>();
        out.sort_by(score_ordering);
        out.truncate(limit);
        out
    }

    #[must_use]
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

    #[must_use]
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

    fn upsert_child_index_entry(
        &mut self,
        parent_uri: Option<&str>,
        child_uri: Arc<str>,
        entry: ChildIndexEntry,
    ) {
        let Some(parent_uri) = parent_uri else {
            return;
        };
        self.children_by_parent
            .entry(Arc::from(parent_uri))
            .or_default()
            .insert(child_uri, entry);
    }

    fn remove_child_index_entry(&mut self, parent_uri: Option<&str>, child_uri: &str) {
        let Some(parent_uri) = parent_uri else {
            return;
        };
        let mut remove_parent = false;
        if let Some(children) = self.children_by_parent.get_mut(parent_uri) {
            children.remove(child_uri);
            remove_parent = children.is_empty();
        }
        if remove_parent {
            self.children_by_parent.remove(parent_uri);
        }
    }

    fn has_matching_leaf_descendant(&self, ancestor_uri: &str, filter: &NormalizedFilter) -> bool {
        // Parent->children graph is the source of truth for ancestry checks.
        let mut pending = vec![Arc::<str>::from(ancestor_uri)];
        let mut visited = HashSet::<Arc<str>>::new();

        while let Some(parent_uri) = pending.pop() {
            if !visited.insert(parent_uri.clone()) {
                continue;
            }
            let Some(children) = self.children_by_parent.get(parent_uri.as_ref()) else {
                continue;
            };
            for (child_uri, child_entry) in children {
                if child_entry.is_leaf {
                    if let Some(record) = self.records.get(child_uri.as_ref())
                        && leaf_matches_filter(record, filter)
                    {
                        return true;
                    }
                    continue;
                }
                pending.push(child_uri.clone());
            }
        }

        false
    }

    pub(crate) fn filter_projection_uris(
        &self,
        filter: Option<&SearchFilter>,
    ) -> Option<HashSet<Arc<str>>> {
        let filter = normalize_filter(filter)?;
        // Keep filter projection on shared URI keys to avoid per-search String allocations.
        let mut allowed_uris = HashSet::new();

        for (leaf_key, record) in self.records.iter().filter(|(_, record)| record.is_leaf) {
            if !leaf_matches_filter(record, &filter) {
                continue;
            }
            allowed_uris.insert(leaf_key.clone());

            let mut parent_uri = record.parent_uri.as_deref();
            let mut remaining_hops = self.records.len();
            while let Some(uri) = parent_uri {
                if remaining_hops == 0 {
                    break;
                }
                remaining_hops = remaining_hops.saturating_sub(1);
                if let Some((parent_key, parent_record)) = self.records.get_key_value(uri) {
                    allowed_uris.insert(parent_key.clone());
                    parent_uri = parent_record.parent_uri.as_deref();
                } else {
                    allowed_uris.insert(Arc::from(uri));
                    break;
                }
            }
        }

        Some(allowed_uris)
    }
}

fn build_upsert_text(record: &IndexRecord) -> String {
    let tags_len = record.tags.iter().map(String::len).sum::<usize>();
    let tag_gap_len = record.tags.len().saturating_sub(1);
    let mut text = String::with_capacity(
        record.name.len()
            + record.abstract_text.len()
            + record.content.len()
            + tags_len
            + tag_gap_len
            + 3,
    );
    text.push_str(&record.name);
    text.push(' ');
    text.push_str(&record.abstract_text);
    text.push(' ');
    text.push_str(&record.content);
    text.push(' ');
    for (index, tag) in record.tags.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        text.push_str(tag);
    }
    text
}

#[derive(Debug)]
struct NormalizedFilter {
    tags: Vec<String>,
    mime: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ExactRecordKeys {
    literal: ExactLiteralKeys,
    compact: ExactCompactKeys,
    tokens: ExactTokenSignatures,
    sections: ExactSectionKeys,
}

#[derive(Debug, Clone, Default)]
struct ExactLiteralKeys {
    uri_lower: String,
    name_lower: String,
    abstract_lower: String,
    basename_lower: String,
    stem_lower: String,
}

#[derive(Debug, Clone, Default)]
struct ExactCompactKeys {
    name: ExactFuzzyCompactKey,
    abstract_key: String,
    basename: ExactFuzzyCompactKey,
    stem: ExactFuzzyCompactKey,
}

#[derive(Debug, Clone, Default)]
struct ExactFuzzyCompactKey {
    key: String,
    len: usize,
    bigrams: Vec<u64>,
}

impl ExactFuzzyCompactKey {
    fn from_key(key: String) -> Self {
        Self {
            len: key.chars().count(),
            bigrams: compact_char_bigrams(&key),
            key,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ExactTokenSignatures {
    name: String,
    abstract_text: String,
    basename: String,
    stem: String,
}

#[derive(Debug, Clone, Default)]
struct ExactSectionKeys {
    heading_lower_hashes: Vec<u64>,
    heading_compact_keys: Vec<String>,
    heading_token_signatures: Vec<String>,
    content_line_lower_hashes: Vec<u64>,
    content_line_token_signatures: Vec<String>,
}

impl ExactRecordKeys {
    fn from_record(record: &IndexRecord) -> Self {
        let basename = uri_basename(&record.uri);
        let stem = basename_stem(&basename);
        let content_line_lowers =
            normalized_content_line_lowers(&record.content, MAX_EXACT_CONTENT_LINE_KEYS);
        let content_line_lower_hashes = normalize_string_hashes(&content_line_lowers);
        let content_line_token_signatures = normalize_string_keys(
            content_line_lowers
                .iter()
                .map(|line| token_signature_from_text(line))
                .collect(),
        );
        let heading_lowers = markdown_heading_lowers(&record.content, MAX_EXACT_HEADING_KEYS);
        let heading_lower_hashes = normalize_string_hashes(&heading_lowers);
        let heading_compact_keys = normalize_string_keys(
            heading_lowers
                .iter()
                .map(|heading| compact_alnum_key(heading))
                .collect(),
        );
        let heading_token_signatures = normalize_string_keys(
            heading_lowers
                .iter()
                .map(|heading| token_signature_from_text(heading))
                .collect(),
        );
        Self {
            literal: ExactLiteralKeys {
                uri_lower: record.uri.to_lowercase(),
                name_lower: record.name.to_lowercase(),
                abstract_lower: record.abstract_text.to_lowercase(),
                basename_lower: basename.to_lowercase(),
                stem_lower: stem.to_lowercase(),
            },
            compact: ExactCompactKeys {
                name: ExactFuzzyCompactKey::from_key(compact_alnum_key(&record.name)),
                abstract_key: compact_alnum_key(&record.abstract_text),
                basename: ExactFuzzyCompactKey::from_key(compact_alnum_key(&basename)),
                stem: ExactFuzzyCompactKey::from_key(compact_alnum_key(&stem)),
            },
            tokens: ExactTokenSignatures {
                name: token_signature_from_text(&record.name),
                abstract_text: token_signature_from_text(&record.abstract_text),
                basename: token_signature_from_text(&basename),
                stem: token_signature_from_text(&stem),
            },
            sections: ExactSectionKeys {
                heading_lower_hashes,
                heading_compact_keys,
                heading_token_signatures,
                content_line_lower_hashes,
                content_line_token_signatures,
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ExactQueryKeys {
    raw_lower: String,
    raw_lower_hash: u64,
    compact_key: String,
    compact_len: usize,
    compact_bigrams: Vec<u64>,
    token_signature: String,
}

impl ExactQueryKeys {
    fn from_query(query: &str) -> Self {
        let raw_lower = query.trim().to_lowercase();
        let raw_lower_hash = if raw_lower.is_empty() {
            0
        } else {
            stable_fingerprint64(&raw_lower)
        };
        let compact_key = compact_alnum_key(query);
        Self {
            raw_lower,
            raw_lower_hash,
            compact_len: compact_key.chars().count(),
            compact_bigrams: compact_char_bigrams(&compact_key),
            compact_key,
            token_signature: token_signature_from_text(query),
        }
    }

    fn is_empty(&self) -> bool {
        self.raw_lower.is_empty() && self.compact_key.is_empty() && self.token_signature.is_empty()
    }
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

fn score_ordering(a: &ScoredRecord, b: &ScoredRecord) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| b.exact.partial_cmp(&a.exact).unwrap_or(Ordering::Equal))
        .then_with(|| a.uri.cmp(&b.uri))
}

fn exact_match_score(
    query: &ExactQueryKeys,
    record_keys: Option<&ExactRecordKeys>,
    fallback_record: &IndexRecord,
) -> f32 {
    if query.is_empty() {
        return 0.0;
    }
    let owned_fallback;
    let keys = if let Some(keys) = record_keys {
        keys
    } else {
        owned_fallback = ExactRecordKeys::from_record(fallback_record);
        &owned_fallback
    };

    if !query.raw_lower.is_empty() {
        if query.raw_lower == keys.literal.uri_lower {
            return 1.0;
        }
        if contains_sorted_hash(&keys.sections.heading_lower_hashes, query.raw_lower_hash) {
            return 0.985;
        }
        if contains_sorted_hash(
            &keys.sections.content_line_lower_hashes,
            query.raw_lower_hash,
        ) {
            return 0.975;
        }
        if query.raw_lower == keys.literal.abstract_lower {
            return 0.99;
        }
        if query.raw_lower == keys.literal.basename_lower {
            return 0.98;
        }
        if query.raw_lower == keys.literal.stem_lower {
            return 0.96;
        }
        if query.raw_lower == keys.literal.name_lower {
            return 0.94;
        }
    }

    if !query.token_signature.is_empty() {
        if query.token_signature == keys.tokens.abstract_text {
            return 0.95;
        }
        if contains_sorted_key(
            &keys.sections.heading_token_signatures,
            &query.token_signature,
        ) {
            return 0.935;
        }
        if contains_sorted_key(
            &keys.sections.content_line_token_signatures,
            &query.token_signature,
        ) {
            return 0.93;
        }
        if query.token_signature == keys.tokens.stem {
            return 0.92;
        }
        if query.token_signature == keys.tokens.basename {
            return 0.90;
        }
        if query.token_signature == keys.tokens.name {
            return 0.88;
        }
    }

    if !query.compact_key.is_empty() {
        if query.compact_key == keys.compact.stem.key {
            return 0.93;
        }
        if contains_sorted_key(&keys.sections.heading_compact_keys, &query.compact_key) {
            return 0.925;
        }
        if query.compact_key == keys.compact.basename.key {
            return 0.91;
        }
        if query.compact_key == keys.compact.name.key {
            return 0.89;
        }
        if query.compact_key == keys.compact.abstract_key {
            return 0.87;
        }

        if query.compact_len >= 5 {
            if keys
                .sections
                .heading_compact_keys
                .iter()
                .any(|heading| within_edit_distance_one(&query.compact_key, heading))
            {
                return 0.88;
            }
            if within_edit_distance_one(&query.compact_key, &keys.compact.stem.key) {
                return 0.86;
            }
            if within_edit_distance_one(&query.compact_key, &keys.compact.basename.key) {
                return 0.84;
            }
            if within_edit_distance_one(&query.compact_key, &keys.compact.name.key) {
                return 0.82;
            }
        }

        let fuzzy = fuzzy_compact_bigram_score(query, keys);
        if fuzzy > 0.0 {
            return fuzzy;
        }
    }

    0.0
}

fn exact_confidence_bonus(exact: f32) -> f32 {
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
    let inter = usize_to_f32(query_tokens.intersection(doc_tokens).count());
    let union = usize_to_f32(query_tokens.union(doc_tokens).count());
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
struct LexicalDocView<'a> {
    term_freq: Option<&'a HashMap<String, u32>>,
    token_set: Option<&'a HashSet<String>>,
    text_lower: Option<&'a str>,
    doc_len: usize,
}

#[derive(Debug, Clone, Copy)]
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

fn uri_basename(uri: &str) -> String {
    uri.rsplit('/').next().unwrap_or_default().to_string()
}

fn basename_stem(basename: &str) -> String {
    basename
        .rsplit_once('.')
        .map_or_else(|| basename.to_string(), |(stem, _)| stem.to_string())
}

fn token_signature_from_text(text: &str) -> String {
    crate::embedding::tokenize_vec(text).join(" ")
}

fn collect_head_tail_unique_keys<I>(keys: I, limit: usize) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    if limit == 0 {
        return Vec::new();
    }

    let head_limit = limit.div_ceil(2);
    let tail_limit = limit.saturating_sub(head_limit);
    let mut head = Vec::with_capacity(head_limit);
    let mut tail = VecDeque::<String>::with_capacity(tail_limit);
    let mut seen = HashSet::<String>::new();

    for key in keys {
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        if head.len() < head_limit {
            head.push(key);
            continue;
        }
        if tail_limit == 0 {
            continue;
        }
        tail.push_back(key);
        if tail.len() > tail_limit {
            tail.pop_front();
        }
    }

    head.extend(tail);
    head
}

fn markdown_heading_lowers(content: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let mut heading_keys = Vec::<String>::new();
    let mut in_fence_block = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence_block = !in_fence_block;
            continue;
        }
        if in_fence_block {
            continue;
        }
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        if level == 0 || level > 6 {
            continue;
        }
        let Some(raw_heading) = trimmed.get(level..) else {
            continue;
        };
        let heading = raw_heading.trim().trim_end_matches('#').trim();
        if heading.is_empty() {
            continue;
        }
        heading_keys.push(heading.to_lowercase());
    }
    let mut headings = collect_head_tail_unique_keys(heading_keys, limit);
    headings.sort_unstable();
    headings
}

fn normalized_content_line_lowers(content: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let mut line_keys = Vec::<String>::new();
    for line in content.lines() {
        let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let lowered = normalized.trim().to_lowercase();
        if lowered.len() < 3 {
            continue;
        }
        line_keys.push(lowered);
    }
    let mut lines = collect_head_tail_unique_keys(line_keys, limit);
    lines.sort_unstable();
    lines
}

fn normalize_string_keys(mut keys: Vec<String>) -> Vec<String> {
    keys.retain(|key| !key.is_empty());
    keys.sort_unstable();
    keys.dedup();
    keys
}

fn normalize_string_hashes(keys: &[String]) -> Vec<u64> {
    let mut hashes = keys
        .iter()
        .filter(|key| !key.is_empty())
        .map(|key| stable_fingerprint64(key))
        .collect::<Vec<_>>();
    hashes.sort_unstable();
    hashes.dedup();
    hashes
}

fn contains_sorted_key(keys: &[String], target: &str) -> bool {
    keys.binary_search_by(|candidate| candidate.as_str().cmp(target))
        .is_ok()
}

fn contains_sorted_hash(keys: &[u64], target: u64) -> bool {
    keys.binary_search(&target).is_ok()
}

fn compact_alnum_key(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn stable_fingerprint64(text: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn compact_char_bigrams(text: &str) -> Vec<u64> {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() < 2 {
        return Vec::new();
    }
    let mut bigrams = chars
        .windows(2)
        .map(|pair| ((pair[0] as u64) << 32) | (pair[1] as u64))
        .collect::<Vec<_>>();
    // Pre-sort once at key construction time so pairwise fuzzy scoring is
    // merge-only and allocation-free in the hot path.
    bigrams.sort_unstable();
    bigrams
}

fn fuzzy_compact_bigram_score(query: &ExactQueryKeys, keys: &ExactRecordKeys) -> f32 {
    if query.compact_len < 6 || query.compact_bigrams.len() < 2 {
        return 0.0;
    }

    let mut best = 0.0_f32;
    best = best.max(fuzzy_bigram_field_score(
        query,
        &keys.compact.stem.key,
        keys.compact.stem.len,
        &keys.compact.stem.bigrams,
        0.86,
    ));
    best = best.max(fuzzy_bigram_field_score(
        query,
        &keys.compact.basename.key,
        keys.compact.basename.len,
        &keys.compact.basename.bigrams,
        0.84,
    ));
    best = best.max(fuzzy_bigram_field_score(
        query,
        &keys.compact.name.key,
        keys.compact.name.len,
        &keys.compact.name.bigrams,
        0.82,
    ));
    best
}

fn fuzzy_bigram_field_score(
    query: &ExactQueryKeys,
    candidate_key: &str,
    candidate_len: usize,
    candidate_bigrams: &[u64],
    field_weight: f32,
) -> f32 {
    if candidate_len < 6 || candidate_bigrams.len() < 2 {
        return 0.0;
    }
    if query.compact_len.abs_diff(candidate_len) > 4 {
        return 0.0;
    }

    let Some(query_prefix) = query.compact_key.chars().next() else {
        return 0.0;
    };
    let Some(candidate_prefix) = candidate_key.chars().next() else {
        return 0.0;
    };
    if query_prefix != candidate_prefix {
        return 0.0;
    }

    let dice = sorensen_dice_multiset(&query.compact_bigrams, candidate_bigrams);
    if dice < 0.70 {
        return 0.0;
    }
    (field_weight * (0.52 + 0.43 * dice)).clamp(0.0, 1.0)
}

fn sorensen_dice_multiset(lhs: &[u64], rhs: &[u64]) -> f32 {
    if lhs.is_empty() || rhs.is_empty() {
        return 0.0;
    }
    debug_assert!(lhs.windows(2).all(|pair| pair[0] <= pair[1]));
    debug_assert!(rhs.windows(2).all(|pair| pair[0] <= pair[1]));

    let mut i = 0usize;
    let mut j = 0usize;
    let mut intersection = 0usize;
    while i < lhs.len() && j < rhs.len() {
        if lhs[i] == rhs[j] {
            intersection += 1;
            i += 1;
            j += 1;
            continue;
        }
        if lhs[i] < rhs[j] {
            i += 1;
        } else {
            j += 1;
        }
    }

    if intersection == 0 {
        return 0.0;
    }
    let numerator = usize_to_f32(intersection.saturating_mul(2));
    let denominator = usize_to_f32(lhs.len().saturating_add(rhs.len()));
    if denominator == 0.0 {
        return 0.0;
    }
    (numerator / denominator).clamp(0.0, 1.0)
}

fn within_edit_distance_one(lhs: &str, rhs: &str) -> bool {
    if lhs == rhs {
        return true;
    }

    let lhs_chars: Vec<char> = lhs.chars().collect();
    let rhs_chars: Vec<char> = rhs.chars().collect();
    let lhs_len = lhs_chars.len();
    let rhs_len = rhs_chars.len();
    if lhs_len.abs_diff(rhs_len) > 1 {
        return false;
    }

    if lhs_len == rhs_len {
        let mismatches = lhs_chars
            .iter()
            .zip(rhs_chars.iter())
            .enumerate()
            .filter_map(|(idx, (left, right))| if left != right { Some(idx) } else { None })
            .collect::<Vec<_>>();
        if mismatches.len() <= 1 {
            return true;
        }
        if mismatches.len() == 2 {
            let first = mismatches[0];
            let second = mismatches[1];
            if second == first + 1
                && lhs_chars[first] == rhs_chars[second]
                && lhs_chars[second] == rhs_chars[first]
            {
                return true;
            }
        }
        return false;
    }

    let (shorter, longer) = if lhs_len < rhs_len {
        (lhs_chars, rhs_chars)
    } else {
        (rhs_chars, lhs_chars)
    };
    let mut short_idx = 0usize;
    let mut long_idx = 0usize;
    let mut edits = 0usize;
    while short_idx < shorter.len() && long_idx < longer.len() {
        if shorter[short_idx] == longer[long_idx] {
            short_idx += 1;
            long_idx += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
        long_idx += 1;
    }

    true
}

fn recency_score(now: DateTime<Utc>, updated_at: DateTime<Utc>) -> f32 {
    let age_days = i64_to_f32((now - updated_at).num_days().max(0));
    (1.0 / (1.0 + age_days / 30.0)).clamp(0.0, 1.0)
}

fn path_score(uri: &str, target_uri: Option<&str>, target_scope_root: Option<&str>) -> f32 {
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

fn uri_path_prefix_match(uri: &str, prefix_uri: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::models::SearchFilter;

    #[test]
    fn build_upsert_text_matches_legacy_join_shape_with_tags() {
        let record = IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/docs/a.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "name".to_string(),
            abstract_text: "abstract".to_string(),
            content: "content".to_string(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        };
        let legacy = [
            record.name.as_str(),
            record.abstract_text.as_str(),
            record.content.as_str(),
            &record.tags.join(" "),
        ]
        .join(" ");
        assert_eq!(build_upsert_text(&record), legacy);
    }

    #[test]
    fn build_upsert_text_matches_legacy_join_shape_without_tags() {
        let record = IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/docs/a.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "name".to_string(),
            abstract_text: "abstract".to_string(),
            content: "content".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 3,
        };
        let legacy = [
            record.name.as_str(),
            record.abstract_text.as_str(),
            record.content.as_str(),
            &record.tags.join(" "),
        ]
        .join(" ");
        assert_eq!(build_upsert_text(&record), legacy);
    }

    #[test]
    fn search_prioritizes_matching_doc() {
        let mut index = InMemoryIndex::new();
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
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/docs/auth")
        );
    }

    #[test]
    fn token_overlap_count_uses_indexed_token_sets() {
        let mut index = InMemoryIndex::new();
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

        let query_tokens = crate::embedding::tokenize_set("oauth code missing");
        assert_eq!(
            index.token_overlap_count("axiom://resources/docs/auth", &query_tokens),
            2
        );
        assert_eq!(
            index.token_overlap_count("axiom://resources/docs/unknown", &query_tokens),
            0
        );
    }

    #[test]
    fn children_of_returns_sorted_child_records() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "p".to_string(),
            uri: "axiom://resources/docs".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "docs".to_string(),
            abstract_text: "docs".to_string(),
            content: "docs".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 2,
        });
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/docs/b.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "b.md".to_string(),
            abstract_text: "b".to_string(),
            content: "b".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/docs/a.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "a.md".to_string(),
            abstract_text: "a".to_string(),
            content: "a".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "3".to_string(),
            uri: "axiom://resources/other.md".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "other.md".to_string(),
            abstract_text: "other".to_string(),
            content: "other".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 2,
        });

        let children = index.children_of("axiom://resources/docs");
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].uri, "axiom://resources/docs/a.md".into());
        assert_eq!(children[1].uri, "axiom://resources/docs/b.md".into());
        assert!(children.iter().all(|child| child.is_leaf));
        assert!(children.iter().all(|child| child.depth == 3));
    }

    #[test]
    fn children_of_tracks_reparent_and_remove_consistently() {
        let mut index = InMemoryIndex::new();
        let uri = "axiom://resources/docs/item.md";
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: uri.to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "item.md".to_string(),
            abstract_text: "item".to_string(),
            content: "item".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 3,
        });
        assert_eq!(index.children_of("axiom://resources/docs").len(), 1);

        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: uri.to_string(),
            parent_uri: Some("axiom://resources/relocated".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "item.md".to_string(),
            abstract_text: "item".to_string(),
            content: "item v2".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 3,
        });
        assert!(index.children_of("axiom://resources/docs").is_empty());
        let relocated = index.children_of("axiom://resources/relocated");
        assert_eq!(relocated.len(), 1);
        assert_eq!(relocated[0].uri, uri.into());

        index.remove(uri);
        assert!(index.children_of("axiom://resources/relocated").is_empty());
    }

    #[test]
    fn compact_char_bigrams_are_sorted_for_merge_scoring() {
        let bigrams = compact_char_bigrams("abca");
        assert!(bigrams.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn sorensen_dice_multiset_counts_duplicates() {
        let lhs = vec![1_u64, 1_u64, 2_u64];
        let rhs = vec![1_u64, 2_u64, 2_u64];
        let score = sorensen_dice_multiset(&lhs, &rhs);
        assert!((score - (4.0 / 6.0)).abs() < 1e-6);
    }

    #[test]
    fn lexical_exact_match_boost_prioritizes_literal_query() {
        let mut index = InMemoryIndex::new();
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
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/logs/exact")
        );
        assert!(result.first().expect("no result").sparse >= result[1].sparse);
    }

    #[test]
    fn exact_filename_match_prioritizes_target_file() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/manual/FILE_STRUCTURE.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "FILE_STRUCTURE.md".to_string(),
            abstract_text: "Workspace file structure".to_string(),
            content: "AxiomMe file structure guide".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/manual/ARCHITECTURE.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "ARCHITECTURE.md".to_string(),
            abstract_text: "Architecture notes".to_string(),
            content: "Discusses file structure and decomposition.".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let with_ext = index.search("FILE_STRUCTURE.md", None, 10, None, None);
        assert_eq!(
            with_ext.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/manual/FILE_STRUCTURE.md")
        );
        assert!(with_ext.first().expect("no result").exact > 0.0);

        let stem_only = index.search("FILE_STRUCTURE", None, 10, None, None);
        assert_eq!(
            stem_only.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/manual/FILE_STRUCTURE.md")
        );
        assert!(stem_only.first().expect("no result").exact > 0.0);
    }

    #[test]
    fn exact_title_match_prioritizes_name_match() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/notes/title-guide.md".to_string(),
            parent_uri: Some("axiom://resources/notes".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "QA Guide".to_string(),
            abstract_text: "Manual QA process".to_string(),
            content: "# QA Guide\nChecklist and steps".to_string(),
            tags: vec!["qa".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/notes/checklist.md".to_string(),
            parent_uri: Some("axiom://resources/notes".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "QA Checklist".to_string(),
            abstract_text: "QA checklist".to_string(),
            content: "# QA Checklist\nGuide for release QA".to_string(),
            tags: vec!["qa".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("QA Guide", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/notes/title-guide.md")
        );
        assert!(result.first().expect("no result").exact > result[1].exact);
    }

    #[test]
    fn exact_abstract_title_match_prioritizes_heading_title() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/context/FILE_STRUCTURE.md".to_string(),
            parent_uri: Some("axiom://resources/context".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "FILE_STRUCTURE.md".to_string(),
            abstract_text: "File Structure (Lean Architecture)".to_string(),
            content: "Document layout and boundaries.".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/context/clean-architecture.md".to_string(),
            parent_uri: Some("axiom://resources/context".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "clean-architecture.md".to_string(),
            abstract_text: "Clean Architecture".to_string(),
            content: "lean architecture and file structure guidance".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("File Structure (Lean Architecture)", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/context/FILE_STRUCTURE.md")
        );
        assert!(result.first().expect("no result").exact >= 0.95);
    }

    #[test]
    fn exact_korean_abstract_title_match_prioritizes_heading_title() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/expertise/system-architect.md".to_string(),
            parent_uri: Some("axiom://resources/expertise".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "system-architect.md".to_string(),
            abstract_text: "아키텍트 페르소나".to_string(),
            content: "시스템 설계 관점의 역할과 책임".to_string(),
            tags: vec!["persona".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/expertise/web-platform.md".to_string(),
            parent_uri: Some("axiom://resources/expertise".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "web-platform.md".to_string(),
            abstract_text: "웹 플랫폼 생태계 가이드".to_string(),
            content: "아키텍처 플랫폼 안내".to_string(),
            tags: vec!["guide".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("아키텍트 페르소나", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/expertise/system-architect.md")
        );
        assert!(result.first().expect("no result").exact >= 0.95);
    }

    #[test]
    fn markdown_heading_signal_prioritizes_heading_owner_doc() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/rules/macos-platform.md".to_string(),
            parent_uri: Some("axiom://resources/rules".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "macos-platform.md".to_string(),
            abstract_text: "macOS rules".to_string(),
            content: "### RULE_2_2: 절대 리젝 방지 규칙\n정책 본문".to_string(),
            tags: vec!["rules".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/rules/ios-platform.md".to_string(),
            parent_uri: Some("axiom://resources/rules".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "ios-platform.md".to_string(),
            abstract_text: "iOS rules".to_string(),
            content: "이 문서는 RULE_2_2: 절대 리젝 방지 규칙을 참조한다.".to_string(),
            tags: vec!["rules".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("RULE_2_2: 절대 리젝 방지 규칙", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/rules/macos-platform.md")
        );
        assert!(result.first().expect("no result").exact >= 0.97);
    }

    #[test]
    fn markdown_heading_lowers_extracts_and_normalizes_atx_titles() {
        let headings = markdown_heading_lowers(
            "text\n# Title One\n##  Title Two  ##\n####\n### title one\n",
            8,
        );
        assert_eq!(
            headings,
            vec!["title one".to_string(), "title two".to_string()]
        );
    }

    #[test]
    fn markdown_heading_lowers_keeps_tail_window_under_limit() {
        let headings = markdown_heading_lowers("# h1\n# h2\n# h3\n# h4\n# h5\n# h6\n", 3);
        assert_eq!(
            headings,
            vec!["h1".to_string(), "h2".to_string(), "h6".to_string()]
        );
    }

    #[test]
    fn markdown_heading_lowers_ignores_fenced_code_comments() {
        let headings = markdown_heading_lowers(
            "# Intro\n```sh\n# 개발 서버 시작\n# TypeScript 체크\n```\n## Real Section\n",
            8,
        );
        assert_eq!(
            headings,
            vec!["intro".to_string(), "real section".to_string()]
        );
    }

    #[test]
    fn deep_markdown_heading_signal_uses_tail_window_for_exact_match() {
        let mut index = InMemoryIndex::new();
        let mut deep_outline = String::new();
        for section in 0..40 {
            deep_outline.push_str(&format!("# SECTION {section}\n"));
        }
        deep_outline.push_str("\nTail body");
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/guide/deep-outline.md".to_string(),
            parent_uri: Some("axiom://resources/guide".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "deep-outline.md".to_string(),
            abstract_text: "Deep outline".to_string(),
            content: deep_outline,
            tags: vec!["guide".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/guide/other.md".to_string(),
            parent_uri: Some("axiom://resources/guide".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "other.md".to_string(),
            abstract_text: "Other outline".to_string(),
            content: "SECTION 39 설명 문서".to_string(),
            tags: vec!["guide".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("SECTION 39", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/guide/deep-outline.md")
        );
        assert!(
            result.first().expect("no result").exact >= 0.97,
            "tail heading should contribute exact-match signal"
        );
    }

    #[test]
    fn normalized_content_line_lowers_keeps_head_and_tail_under_limit() {
        let lines = normalized_content_line_lowers(
            "line-alpha\nline-beta\nline-charlie\nline-delta\nline-echo\nline-foxtrot\n",
            4,
        );
        assert_eq!(
            lines,
            vec![
                "line-alpha".to_string(),
                "line-beta".to_string(),
                "line-echo".to_string(),
                "line-foxtrot".to_string()
            ]
        );
    }

    #[test]
    fn content_line_exact_signal_prioritizes_line_owner_doc() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/rules/backend-platform.md".to_string(),
            parent_uri: Some("axiom://resources/rules".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "backend-platform.md".to_string(),
            abstract_text: "backend rules".to_string(),
            content: "개요\n의존성 캐싱 최적화\n추가 설명".to_string(),
            tags: vec!["rules".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/rules/other.md".to_string(),
            parent_uri: Some("axiom://resources/rules".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "other.md".to_string(),
            abstract_text: "other rules".to_string(),
            content: "이 문서는 의존성 캐싱 최적화 관련 권장사항을 포함합니다.".to_string(),
            tags: vec!["rules".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("의존성 캐싱 최적화", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/rules/backend-platform.md")
        );
        assert!(result.first().expect("no result").exact >= 0.97);
    }

    #[test]
    fn compact_key_exact_match_handles_punctuationless_query() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/manual/QA_GUIDE.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "QA Guide".to_string(),
            abstract_text: "iOS QA Guide".to_string(),
            content: "qa checklist".to_string(),
            tags: vec!["qa".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/manual/quick-start.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "Quick Start".to_string(),
            abstract_text: "Start guide".to_string(),
            content: "quick setup".to_string(),
            tags: vec!["guide".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("qaguide", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/manual/QA_GUIDE.md")
        );
        assert!(result.first().expect("no result").exact >= 0.89);
    }

    #[test]
    fn compact_key_edit_distance_one_prioritizes_filename_typo() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/manual/guide.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "guide.md".to_string(),
            abstract_text: "Guide".to_string(),
            content: "core guide".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/manual/guild.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "guild.md".to_string(),
            abstract_text: "Guild".to_string(),
            content: "team guild handbook".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("guidd", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/manual/guide.md")
        );
        assert!(result.first().expect("no result").exact >= 0.84);
    }

    #[test]
    fn compact_key_adjacent_swap_typo_prioritizes_filename() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/manual/guide.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "guide.md".to_string(),
            abstract_text: "Guide".to_string(),
            content: "guide".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/manual/guild.md".to_string(),
            parent_uri: Some("axiom://resources/manual".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "guild.md".to_string(),
            abstract_text: "Guild".to_string(),
            content: "guild".to_string(),
            tags: vec!["docs".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("gudie", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/manual/guide.md")
        );
        assert!(result.first().expect("no result").exact >= 0.84);
    }

    #[test]
    fn compact_key_korean_substitution_typo_prefers_original_title() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "1".to_string(),
            uri: "axiom://resources/notes/korean-title.md".to_string(),
            parent_uri: Some("axiom://resources/notes".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "웹 플랫폼 생태계 가이드".to_string(),
            abstract_text: "웹 플랫폼 생태계 가이드".to_string(),
            content: "플랫폼 생태계 운영 가이드".to_string(),
            tags: vec!["guide".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "2".to_string(),
            uri: "axiom://resources/notes/korean-other.md".to_string(),
            parent_uri: Some("axiom://resources/notes".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "웹 플랫폼 아키텍처 가이드".to_string(),
            abstract_text: "웹 플랫폼 아키텍처 가이드".to_string(),
            content: "플랫폼 아키텍처 설계 문서".to_string(),
            tags: vec!["guide".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let result = index.search("웹플랫폼생태계가이x", None, 10, None, None);
        assert_eq!(
            result.first().expect("no result").uri,
            std::sync::Arc::from("axiom://resources/notes/korean-title.md")
        );
        assert!(result.first().expect("no result").exact >= 0.70);
    }

    #[test]
    fn tag_filter_limits_leaf_results() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources/docs".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "docs".to_string(),
            abstract_text: "docs".to_string(),
            content: String::new(),
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
                .any(|x| x.uri == "axiom://resources/docs".into())
        );
        assert!(
            result
                .iter()
                .any(|x| x.uri == "axiom://resources/docs/auth.md".into())
        );
        assert!(
            !result
                .iter()
                .any(|x| x.uri == "axiom://resources/docs/storage.md".into())
        );
    }

    #[test]
    fn filter_keeps_matching_leaf_ancestor_chain() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "root".to_string(),
            uri: "axiom://resources/docs".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "docs".to_string(),
            abstract_text: "docs".to_string(),
            content: String::new(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 1,
        });
        index.upsert(IndexRecord {
            id: "nested".to_string(),
            uri: "axiom://resources/docs/guides".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "guides".to_string(),
            abstract_text: "guides".to_string(),
            content: String::new(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 2,
        });
        index.upsert(IndexRecord {
            id: "match".to_string(),
            uri: "axiom://resources/docs/guides/auth.md".to_string(),
            parent_uri: Some("axiom://resources/docs/guides".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "auth.md".to_string(),
            abstract_text: "auth".to_string(),
            content: "oauth flow".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "non-match".to_string(),
            uri: "axiom://resources/docs/guides/storage.md".to_string(),
            parent_uri: Some("axiom://resources/docs/guides".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "storage.md".to_string(),
            abstract_text: "storage".to_string(),
            content: "disk iops".to_string(),
            tags: vec!["storage".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });

        let filter = SearchFilter {
            tags: vec!["auth".to_string()],
            mime: None,
        };
        let result = index.search("docs", None, 20, None, Some(&filter));
        assert!(
            result
                .iter()
                .any(|x| x.uri == "axiom://resources/docs".into())
        );
        assert!(
            result
                .iter()
                .any(|x| x.uri == "axiom://resources/docs/guides".into())
        );
        assert!(
            result
                .iter()
                .any(|x| x.uri == "axiom://resources/docs/guides/auth.md".into())
        );
        assert!(
            !result
                .iter()
                .any(|x| x.uri == "axiom://resources/docs/guides/storage.md".into())
        );
    }

    #[test]
    fn record_matches_filter_uses_parent_chain_not_uri_prefix() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "docs".to_string(),
            uri: "axiom://resources/docs".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "docs".to_string(),
            abstract_text: "docs".to_string(),
            content: String::new(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 1,
        });
        index.upsert(IndexRecord {
            id: "other".to_string(),
            uri: "axiom://resources/other".to_string(),
            parent_uri: Some("axiom://resources".to_string()),
            is_leaf: false,
            context_type: "resource".to_string(),
            name: "other".to_string(),
            abstract_text: "other".to_string(),
            content: String::new(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 1,
        });
        index.upsert(IndexRecord {
            id: "leaf".to_string(),
            uri: "axiom://resources/docs/ghost.md".to_string(),
            parent_uri: Some("axiom://resources/other".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "ghost.md".to_string(),
            abstract_text: "ghost".to_string(),
            content: "auth".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 2,
        });

        let filter = SearchFilter {
            tags: vec!["auth".to_string()],
            mime: None,
        };
        let docs = index.get("axiom://resources/docs").expect("docs record");
        let other = index.get("axiom://resources/other").expect("other record");
        assert!(!index.record_matches_filter(docs, Some(&filter)));
        assert!(index.record_matches_filter(other, Some(&filter)));
    }

    #[test]
    fn mime_filter_matches_extension_derived_mime() {
        let mut index = InMemoryIndex::new();
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
        assert!(result.iter().any(|x| x.uri.ends_with("guide.md")));
        assert!(!result.iter().any(|x| x.uri.ends_with("schema.json")));
    }

    #[test]
    fn uris_with_prefix_returns_sorted_matches_without_record_clone_requirements() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "a".to_string(),
            uri: "axiom://resources/docs/a.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "a.md".to_string(),
            abstract_text: "a".to_string(),
            content: "a".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 2,
        });
        index.upsert(IndexRecord {
            id: "b".to_string(),
            uri: "axiom://resources/docs/sub/b.md".to_string(),
            parent_uri: Some("axiom://resources/docs/sub".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "b.md".to_string(),
            abstract_text: "b".to_string(),
            content: "b".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "c".to_string(),
            uri: "axiom://resources/other/c.md".to_string(),
            parent_uri: Some("axiom://resources/other".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "c.md".to_string(),
            abstract_text: "c".to_string(),
            content: "c".to_string(),
            tags: vec![],
            updated_at: Utc::now(),
            depth: 2,
        });

        let prefix = AxiomUri::parse("axiom://resources/docs").expect("prefix");
        let uris = index.uris_with_prefix(&prefix);
        assert_eq!(
            uris,
            vec![
                "axiom://resources/docs/a.md".to_string(),
                "axiom://resources/docs/sub/b.md".to_string()
            ]
        );
    }

    #[test]
    fn uri_path_prefix_match_respects_segment_boundaries() {
        assert!(uri_path_prefix_match(
            "axiom://resources/docs/auth",
            "axiom://resources/docs/auth"
        ));
        assert!(uri_path_prefix_match(
            "axiom://resources/docs/auth/guide.md",
            "axiom://resources/docs/auth"
        ));
        assert!(!uri_path_prefix_match(
            "axiom://resources/docs/authz.md",
            "axiom://resources/docs/auth"
        ));
    }

    #[test]
    fn search_target_filter_respects_uri_boundaries_without_parse() {
        let mut index = InMemoryIndex::new();
        index.upsert(IndexRecord {
            id: "auth-child".to_string(),
            uri: "axiom://resources/docs/auth/guide.md".to_string(),
            parent_uri: Some("axiom://resources/docs/auth".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "guide.md".to_string(),
            abstract_text: "auth guide".to_string(),
            content: "guide".to_string(),
            tags: vec!["auth".to_string()],
            updated_at: Utc::now(),
            depth: 3,
        });
        index.upsert(IndexRecord {
            id: "authz-sibling".to_string(),
            uri: "axiom://resources/docs/authz.md".to_string(),
            parent_uri: Some("axiom://resources/docs".to_string()),
            is_leaf: true,
            context_type: "resource".to_string(),
            name: "authz.md".to_string(),
            abstract_text: "authz guide".to_string(),
            content: "guide".to_string(),
            tags: vec!["authz".to_string()],
            updated_at: Utc::now(),
            depth: 2,
        });

        let target = AxiomUri::parse("axiom://resources/docs/auth").expect("target uri");
        let hits = index.search("guide", Some(&target), 20, None, None);
        assert!(
            hits.iter()
                .any(|hit| hit.uri == "axiom://resources/docs/auth/guide.md".into())
        );
        assert!(
            !hits
                .iter()
                .any(|hit| hit.uri == "axiom://resources/docs/authz.md".into())
        );
    }
}
