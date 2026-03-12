use std::sync::Arc;

use super::{ChildIndexEntry, InMemoryIndex};

impl InMemoryIndex {
    pub(super) fn remove_lexical_stats(&mut self, uri: &str) {
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

    pub(super) fn upsert_child_index_entry(
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

    pub(super) fn remove_child_index_entry(&mut self, parent_uri: Option<&str>, child_uri: &str) {
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
}
