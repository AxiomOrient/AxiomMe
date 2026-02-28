use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use super::ChildIndexEntry;
use super::filter::{NormalizedFilter, leaf_matches_filter, normalize_filter};
use crate::models::{IndexRecord, SearchFilter};

pub(super) fn has_matching_leaf_descendant(
    ancestor_uri: &str,
    children_by_parent: &HashMap<Arc<str>, BTreeMap<Arc<str>, ChildIndexEntry>>,
    records: &HashMap<Arc<str>, IndexRecord>,
    filter: &NormalizedFilter,
) -> bool {
    // Parent->children graph is the source of truth for ancestry checks.
    let mut pending = vec![Arc::<str>::from(ancestor_uri)];
    let mut visited = HashSet::<Arc<str>>::new();

    while let Some(parent_uri) = pending.pop() {
        if !visited.insert(parent_uri.clone()) {
            continue;
        }
        let Some(children) = children_by_parent.get(parent_uri.as_ref()) else {
            continue;
        };
        for (child_uri, child_entry) in children {
            if child_entry.is_leaf {
                if let Some(record) = records.get(child_uri.as_ref())
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

pub(super) fn filter_projection_uris(
    records: &HashMap<Arc<str>, IndexRecord>,
    filter: Option<&SearchFilter>,
) -> Option<HashSet<Arc<str>>> {
    let filter = normalize_filter(filter)?;
    // Keep filter projection on shared URI keys to avoid per-search String allocations.
    let mut allowed_uris = HashSet::new();

    for (leaf_key, record) in records.iter().filter(|(_, record)| record.is_leaf) {
        if !leaf_matches_filter(record, &filter) {
            continue;
        }
        allowed_uris.insert(leaf_key.clone());

        let mut parent_uri = record.parent_uri.as_deref();
        let mut remaining_hops = records.len();
        while let Some(uri) = parent_uri {
            if remaining_hops == 0 {
                break;
            }
            remaining_hops = remaining_hops.saturating_sub(1);
            if let Some((parent_key, parent_record)) = records.get_key_value(uri) {
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
