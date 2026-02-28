use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::embedding::embed_text;
use crate::models::{IndexRecord, SearchFilter};
use crate::uri::{AxiomUri, Scope};
use ancestry::{
    filter_projection_uris as ancestry_filter_projection_uris,
    has_matching_leaf_descendant as ancestry_has_matching_leaf_descendant,
};
use exact::ExactRecordKeys;
#[cfg(test)]
use exact::{
    compact_char_bigrams, markdown_heading_lowers, normalized_content_line_lowers,
    sorensen_dice_multiset,
};
use filter::NormalizedFilter;
#[cfg(test)]
use rank::uri_path_prefix_match;
use text_assembly::build_upsert_text;

mod ancestry;
mod exact;
mod filter;
mod lifecycle;
mod rank;
mod search_flow;
mod text_assembly;

const W_EXACT: f32 = 0.42;
const W_EXACT_HIGH_CONF_BOOST: f32 = 0.20;
const W_DENSE: f32 = 0.33;
const W_SPARSE: f32 = 0.20;
const W_RECENCY: f32 = 0.03;
const W_PATH: f32 = 0.02;

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
pub(super) struct ChildIndexEntry {
    pub(super) is_leaf: bool,
    pub(super) depth: usize,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryIndex {
    records: HashMap<Arc<str>, IndexRecord>,
    om_records: HashMap<String, crate::om::OmRecord>,
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

    pub fn upsert_om_record(&mut self, om: crate::om::OmRecord) {
        let uri = format!("axiom://agent/om/{}", om.scope_key);
        let record = IndexRecord {
            id: om.id.clone(),
            uri: uri.clone(),
            parent_uri: Some("axiom://agent/om".to_string()),
            is_leaf: true,
            context_type: "om_record".to_string(),
            name: format!("OM: {}", om.scope_key),
            abstract_text: om
                .active_observations
                .lines()
                .next()
                .unwrap_or_default()
                .to_string(),
            content: om.active_observations.clone(),
            tags: vec!["om".to_string(), om.scope.as_str().to_string()],
            updated_at: om.updated_at,
            depth: 3,
        };
        self.om_records.insert(om.scope_key.clone(), om);
        self.upsert(record);
    }

    pub fn get_om_record(&self, scope_key: &str) -> Option<&crate::om::OmRecord> {
        self.om_records.get(scope_key)
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
        if let Some(scope_key) = uri.strip_prefix("axiom://agent/om/") {
            self.om_records.remove(scope_key);
        }
        if let Some(existing) = self.records.remove(uri) {
            self.remove_child_index_entry(existing.parent_uri.as_deref(), uri);
        }
        self.vectors.remove(uri);
        self.remove_lexical_stats(uri);
        self.exact_keys.remove(uri);
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.om_records.clear();
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

    #[must_use]
    pub fn record_matches_filter(
        &self,
        record: &IndexRecord,
        filter: Option<&SearchFilter>,
    ) -> bool {
        filter::record_matches_filter(record, filter, |normalized_filter| {
            self.has_matching_leaf_descendant(&record.uri, normalized_filter)
        })
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

    fn has_matching_leaf_descendant(&self, ancestor_uri: &str, filter: &NormalizedFilter) -> bool {
        ancestry_has_matching_leaf_descendant(
            ancestor_uri,
            &self.children_by_parent,
            &self.records,
            filter,
        )
    }

    pub(crate) fn filter_projection_uris(
        &self,
        filter: Option<&SearchFilter>,
    ) -> Option<HashSet<Arc<str>>> {
        ancestry_filter_projection_uris(&self.records, filter)
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "ranking weights are intentionally lossy floating-point values"
)]
const fn usize_to_f32(value: usize) -> f32 {
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

    #[test]
    fn clear_removes_om_record_cache() {
        let mut index = InMemoryIndex::new();
        let now = Utc::now();
        let scope_key = "session:s-om-clear";
        index.upsert_om_record(crate::om::OmRecord {
            id: "om-clear".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-clear".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "observation".to_string(),
            observation_token_count: 10,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: false,
            is_buffering_observation: false,
            is_buffering_reflection: false,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        });
        assert!(index.get_om_record(scope_key).is_some());

        index.clear();

        assert!(index.get_om_record(scope_key).is_none());
    }

    #[test]
    fn remove_om_uri_removes_om_record_cache() {
        let mut index = InMemoryIndex::new();
        let now = Utc::now();
        let scope_key = "session:s-om-remove";
        let om_uri = format!("axiom://agent/om/{scope_key}");
        index.upsert_om_record(crate::om::OmRecord {
            id: "om-remove".to_string(),
            scope: crate::om::OmScope::Session,
            scope_key: scope_key.to_string(),
            session_id: Some("s-om-remove".to_string()),
            thread_id: None,
            resource_id: None,
            generation_count: 0,
            last_applied_outbox_event_id: None,
            origin_type: crate::om::OmOriginType::Initial,
            active_observations: "observation".to_string(),
            observation_token_count: 10,
            pending_message_tokens: 0,
            last_observed_at: Some(now),
            current_task: None,
            suggested_response: None,
            last_activated_message_ids: Vec::new(),
            observer_trigger_count_total: 0,
            reflector_trigger_count_total: 0,
            is_observing: false,
            is_reflecting: false,
            is_buffering_observation: false,
            is_buffering_reflection: false,
            last_buffered_at_tokens: 0,
            last_buffered_at_time: None,
            buffered_reflection: None,
            buffered_reflection_tokens: None,
            buffered_reflection_input_tokens: None,
            reflected_observation_line_count: None,
            created_at: now,
            updated_at: now,
        });
        assert!(index.get_om_record(scope_key).is_some());

        index.remove(&om_uri);

        assert!(index.get_om_record(scope_key).is_none());
        assert!(index.get(&om_uri).is_none());
    }
}
