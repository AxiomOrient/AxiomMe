use std::collections::{HashSet, VecDeque};

use crate::models::IndexRecord;

const MAX_EXACT_HEADING_KEYS: usize = 24;
const MAX_EXACT_CONTENT_LINE_KEYS: usize = 64;

#[derive(Debug, Clone, Default)]
pub(super) struct ExactRecordKeys {
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
    pub(super) fn from_record(record: &IndexRecord) -> Self {
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
pub(super) struct ExactQueryKeys {
    raw_lower: String,
    raw_lower_hash: u64,
    compact_key: String,
    compact_len: usize,
    compact_bigrams: Vec<u64>,
    token_signature: String,
}

impl ExactQueryKeys {
    pub(super) fn from_query(query: &str) -> Self {
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

pub(super) fn exact_match_score(
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

pub(super) fn markdown_heading_lowers(content: &str, limit: usize) -> Vec<String> {
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

pub(super) fn normalized_content_line_lowers(content: &str, limit: usize) -> Vec<String> {
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

pub(super) fn compact_char_bigrams(text: &str) -> Vec<u64> {
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

pub(super) fn sorensen_dice_multiset(lhs: &[u64], rhs: &[u64]) -> f32 {
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

#[allow(
    clippy::cast_precision_loss,
    reason = "ranking helpers intentionally use lossy floating-point arithmetic"
)]
const fn usize_to_f32(value: usize) -> f32 {
    value as f32
}
