use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::error::{AxiomError, Result};
use crate::models::{IndexRecord, MemoryCandidate};
use crate::uri::AxiomUri;

use super::super::indexing::ensure_directory_record;
use super::super::memory::{MemorySource, merge_memory_markdown};
use super::Session;
use super::promotion::memory_uri_for_category_key;
use super::resolve_path::dedup_source_ids;
use super::types::ResolvedMemoryCandidate;

pub(super) fn persist_promotion_candidate(
    session: &Session,
    candidate: &ResolvedMemoryCandidate,
    snapshots: Option<&mut BTreeMap<String, Option<String>>>,
) -> Result<AxiomUri> {
    let uri = resolve_target_uri(candidate)?;
    let path = session.fs.resolve_uri(&uri);

    if let Some(existing_snapshots) = snapshots {
        let key = uri.to_string();
        if let std::collections::btree_map::Entry::Vacant(entry) = existing_snapshots.entry(key) {
            let previous = if path.exists() {
                Some(fs::read_to_string(&path)?)
            } else {
                None
            };
            entry.insert(previous);
        }
    }

    write_memory_core(session, candidate, &uri)?;
    Ok(uri)
}

pub(super) fn persist_memory(
    session: &Session,
    candidate: &ResolvedMemoryCandidate,
) -> Result<AxiomUri> {
    let uri = resolve_target_uri(candidate)?;
    write_memory_core(session, candidate, &uri)?;

    session.state.enqueue(
        "upsert",
        &uri.to_string(),
        serde_json::json!({"category": candidate.category}),
    )?;

    Ok(uri)
}

fn resolve_target_uri(candidate: &ResolvedMemoryCandidate) -> Result<AxiomUri> {
    if let Some(target_uri) = candidate.target_uri.as_ref() {
        Ok(target_uri.clone())
    } else {
        memory_uri_for_category_key(&candidate.category, &candidate.key)
    }
}

fn write_memory_core(
    session: &Session,
    candidate: &ResolvedMemoryCandidate,
    uri: &AxiomUri,
) -> Result<()> {
    let path = session.fs.resolve_uri(uri);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut merged = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };

    for source_message_id in dedup_source_ids(&candidate.source_message_ids) {
        let source = MemorySource {
            session_id: session.session_id.clone(),
            message_id: source_message_id.clone(),
        };
        let memory_candidate = MemoryCandidate {
            category: candidate.category.clone(),
            key: candidate.key.clone(),
            text: candidate.text.clone(),
            source_message_id,
        };
        merged = merge_memory_markdown(&merged, &memory_candidate, &source);
    }

    fs::write(path, merged)?;
    Ok(())
}

pub(super) fn reindex_memory_uris(session: &Session, uris: &[AxiomUri]) -> Result<()> {
    let mut index = session
        .index
        .write()
        .map_err(|_| AxiomError::lock_poisoned("index"))?;

    for uri in uris {
        if let Some(parent) = uri.parent() {
            ensure_directory_record(&session.fs, &mut index, &parent)?;
            if let Some(record) = index.get(&parent.to_string()).cloned() {
                session.state.upsert_search_document(&record)?;
            }
        }
        if has_markdown_extension(&uri.to_string()) {
            let text = session.fs.read(uri)?;
            let parent_uri = uri.parent().map(|u| u.to_string());
            let record = IndexRecord {
                id: Uuid::new_v4().to_string(),
                uri: uri.to_string(),
                parent_uri,
                is_leaf: true,
                context_type: "memory".to_string(),
                name: uri.last_segment().unwrap_or("memory").to_string(),
                abstract_text: text.lines().next().unwrap_or_default().to_string(),
                content: text,
                tags: vec!["memory".to_string()],
                updated_at: Utc::now(),
                depth: uri.segments().len(),
            };
            index.upsert(record.clone());
            session.state.upsert_search_document(&record)?;
        }
    }

    drop(index);
    Ok(())
}

pub(super) fn has_markdown_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}
