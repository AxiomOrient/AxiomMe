use crate::embedding::embed_text;
use crate::error::Result;
use crate::uri::AxiomUri;

use super::Session;
use super::helpers::normalize_memory_text;
use super::promotion::memory_category_path;
use super::types::{ExistingMemoryFact, ExistingPromotionFact};
use super::write_path::parse_memory_entries;

use super::write_path::has_markdown_extension;

pub(super) fn list_existing_promotion_facts(
    session: &Session,
) -> Result<Vec<ExistingPromotionFact>> {
    let categories = [
        "profile",
        "preferences",
        "entities",
        "events",
        "cases",
        "patterns",
    ];
    let mut out = Vec::<ExistingPromotionFact>::new();
    for category in categories {
        let uris = list_memory_document_uris(session, category)?;
        for uri in uris {
            let content = session.fs.read(&uri)?;
            let entries = parse_memory_entries(&content);
            for entry in entries {
                let text = normalize_memory_text(&entry.text);
                if text.is_empty() {
                    continue;
                }
                out.push(ExistingPromotionFact {
                    category: category.to_string(),
                    text,
                });
            }
        }
    }
    out.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.text.cmp(&right.text))
    });
    Ok(out)
}

pub(super) fn list_existing_memory_facts(
    session: &Session,
    category: &str,
) -> Result<Vec<ExistingMemoryFact>> {
    let uris = list_memory_document_uris(session, category)?;
    let mut out = Vec::<ExistingMemoryFact>::new();

    for uri in uris {
        let content = session.fs.read(&uri)?;
        let entries = parse_memory_entries(&content);
        for entry in entries {
            let text = normalize_memory_text(&entry.text);
            if text.is_empty() {
                continue;
            }
            out.push(ExistingMemoryFact {
                uri: uri.clone(),
                vector: embed_text(&text),
                text,
            });
        }
    }

    Ok(out)
}

fn list_memory_document_uris(session: &Session, category: &str) -> Result<Vec<AxiomUri>> {
    let (scope, base_path, single_file) = memory_category_path(category)?;
    let base_uri = AxiomUri::root(scope).join(base_path)?;
    if !session.fs.exists(&base_uri) {
        return Ok(Vec::new());
    }
    if single_file {
        return Ok(vec![base_uri]);
    }

    let entries = session.fs.list(&base_uri, true)?;
    let mut out = Vec::<AxiomUri>::new();
    for entry in entries {
        if entry.is_dir || !has_markdown_extension(&entry.uri) {
            continue;
        }
        if entry.uri.ends_with(".abstract.md") || entry.uri.ends_with(".overview.md") {
            continue;
        }
        if let Ok(uri) = AxiomUri::parse(&entry.uri) {
            out.push(uri);
        }
    }
    out.sort_by_key(ToString::to_string);
    Ok(out)
}
