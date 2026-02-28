use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::error::{AxiomError, Result};
use crate::models::{IndexRecord, MemoryCandidate};
use crate::uri::AxiomUri;

use super::super::indexing::ensure_directory_record;
use super::Session;
use super::helpers::normalize_memory_text;
use super::promotion::memory_uri_for_category_key;
use super::resolve_path::dedup_source_ids;
use super::types::ResolvedMemoryCandidate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemorySource {
    pub session_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryEntry {
    pub text: String,
    pub sources: Vec<MemorySource>,
}

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

pub(super) fn merge_memory_markdown(
    existing: &str,
    candidate: &MemoryCandidate,
    source: &MemorySource,
) -> String {
    let mut entries = parse_memory_entries(existing);
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.text == candidate.text)
    {
        if !entry.sources.iter().any(|item| item == source) {
            entry.sources.push(source.clone());
        }
    } else {
        entries.push(MemoryEntry {
            text: candidate.text.clone(),
            sources: vec![source.clone()],
        });
    }

    normalize_memory_entries(&mut entries);
    render_memory_entries(&entries)
}

pub(super) fn parse_memory_entries(content: &str) -> Vec<MemoryEntry> {
    let mut entries = Vec::new();
    let mut current: Option<MemoryEntry> = None;

    for line in content.lines() {
        if let Some(text) = line.strip_prefix("- ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(MemoryEntry {
                text: normalize_memory_text(text),
                sources: Vec::new(),
            });
            continue;
        }

        if let Some(source_line) = line.strip_prefix("  - source: session ")
            && let Some((session_id, message_id)) = source_line.split_once(" message ")
            && let Some(entry) = current.as_mut()
        {
            entry.sources.push(MemorySource {
                session_id: session_id.trim().to_string(),
                message_id: message_id.trim().to_string(),
            });
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    entries
}

fn normalize_memory_entries(entries: &mut Vec<MemoryEntry>) {
    let mut normalized = Vec::<MemoryEntry>::new();
    for entry in entries.drain(..) {
        if let Some(existing) = normalized.iter_mut().find(|item| item.text == entry.text) {
            for source in entry.sources {
                if !existing.sources.iter().any(|item| item == &source) {
                    existing.sources.push(source);
                }
            }
        } else {
            normalized.push(entry);
        }
    }

    for entry in &mut normalized {
        entry.sources.sort_by(|a, b| {
            a.session_id
                .cmp(&b.session_id)
                .then_with(|| a.message_id.cmp(&b.message_id))
        });
        entry.sources.dedup();
    }

    *entries = normalized;
}

fn render_memory_entries(entries: &[MemoryEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str("- ");
        out.push_str(&normalize_memory_text(&entry.text));
        out.push('\n');
        for source in &entry.sources {
            out.push_str("  - source: session ");
            out.push_str(source.session_id.trim());
            out.push_str(" message ");
            out.push_str(source.message_id.trim());
            out.push('\n');
        }
    }
    out
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
