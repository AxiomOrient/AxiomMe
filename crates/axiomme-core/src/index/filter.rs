use crate::mime::infer_mime;
use crate::models::{IndexRecord, SearchFilter};

#[derive(Debug)]
pub(super) struct NormalizedFilter {
    pub(super) tags: Vec<String>,
    pub(super) mime: Option<String>,
}

pub(super) fn record_matches_filter(
    record: &IndexRecord,
    filter: Option<&SearchFilter>,
    has_matching_leaf_descendant: impl FnOnce(&NormalizedFilter) -> bool,
) -> bool {
    let Some(normalized_filter) = normalize_filter(filter) else {
        return true;
    };

    if record.is_leaf {
        return leaf_matches_filter(record, &normalized_filter);
    }

    has_matching_leaf_descendant(&normalized_filter)
}

pub(super) fn normalize_filter(filter: Option<&SearchFilter>) -> Option<NormalizedFilter> {
    let filter = filter?;
    let tags = filter
        .tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let mime = filter
        .mime
        .as_ref()
        .map(|mime| mime.trim().to_lowercase())
        .filter(|mime| !mime.is_empty());
    if tags.is_empty() && mime.is_none() {
        return None;
    }
    Some(NormalizedFilter { tags, mime })
}

pub(super) fn leaf_matches_filter(record: &IndexRecord, filter: &NormalizedFilter) -> bool {
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
