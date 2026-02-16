use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::config::TierSynthesisMode;
use crate::error::Result;
use crate::uri::AxiomUri;

pub(super) const MAX_INDEX_READ_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone)]
pub(super) struct TierEntry {
    name: String,
    is_dir: bool,
}

fn saturating_duration_nanos_to_i64(duration: std::time::Duration) -> i64 {
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}

pub(super) fn metadata_mtime_nanos(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, saturating_duration_nanos_to_i64)
}

pub(super) fn path_mtime_nanos(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .as_ref()
        .map_or(0, metadata_mtime_nanos)
}

fn max_bytes_read_limit(max_bytes: usize) -> u64 {
    u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1)
}

pub(super) fn directory_record_name(uri: &AxiomUri) -> String {
    uri.last_segment()
        .unwrap_or_else(|| uri.scope().as_str())
        .to_string()
}

fn push_bullet_line(output: &mut String, value: &str) {
    let _ = writeln!(output, "- {value}");
}

pub(super) fn should_skip_indexing_file(name: &str) -> bool {
    matches!(
        name,
        ".abstract.md" | ".overview.md" | ".meta.json" | ".relations.json" | "messages.jsonl"
    )
}

pub(super) fn index_state_changed(current: Option<&(String, i64)>, hash: &str, mtime: i64) -> bool {
    match current {
        Some((current_hash, current_mtime)) => current_hash != hash || *current_mtime != mtime,
        None => true,
    }
}

fn list_visible_tier_entries(path: &Path) -> Result<Vec<TierEntry>> {
    let mut entries = Vec::new();
    let read_dir = fs::read_dir(path)?;
    for entry in read_dir {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_indexing_file(&name) {
            continue;
        }
        let is_dir = entry.file_type()?.is_dir();
        entries.push(TierEntry { name, is_dir });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn is_keyword_candidate(token: &str) -> bool {
    if token.len() < 3 || token.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }

    !matches!(
        token,
        "the"
            | "and"
            | "for"
            | "with"
            | "from"
            | "into"
            | "this"
            | "that"
            | "are"
            | "was"
            | "were"
            | "have"
            | "has"
            | "had"
            | "you"
            | "your"
            | "our"
            | "their"
            | "its"
            | "but"
            | "not"
            | "all"
            | "any"
            | "can"
            | "will"
            | "would"
            | "about"
            | "contains"
            | "item"
            | "items"
    )
}

fn collect_semantic_tokens(text: &str, weight: usize, freqs: &mut HashMap<String, usize>) {
    for token in crate::embedding::tokenize_vec(text) {
        if is_keyword_candidate(&token) {
            *freqs.entry(token).or_insert(0) += weight;
        }
    }
}

fn read_preview_text(path: &Path, max_bytes: u64) -> String {
    let Ok(file) = fs::File::open(path) else {
        return String::new();
    };
    let mut buf = Vec::new();
    if file.take(max_bytes).read_to_end(&mut buf).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&buf).to_string()
}

pub(super) fn read_index_source_bytes(path: &Path, max_bytes: usize) -> Result<(Vec<u8>, bool)> {
    let mut file = fs::File::open(path)?;
    let mut content = Vec::new();
    let mut limited = (&mut file).take(max_bytes_read_limit(max_bytes));
    limited.read_to_end(&mut content)?;
    let truncated = content.len() > max_bytes;
    if truncated {
        content.truncate(max_bytes);
    }
    Ok((content, truncated))
}

fn tier_semantic_keywords(path: &Path, entries: &[TierEntry], max_keywords: usize) -> Vec<String> {
    let mut freqs = HashMap::<String, usize>::new();
    for entry in entries.iter().take(64) {
        collect_semantic_tokens(&entry.name, 2, &mut freqs);

        if entry.is_dir {
            continue;
        }

        let entry_path = path.join(&entry.name);
        let preview = read_preview_text(&entry_path, 8 * 1024);
        let preview = preview.lines().take(8).collect::<Vec<_>>().join(" ");
        collect_semantic_tokens(&preview, 1, &mut freqs);
    }

    let mut ranked = freqs.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .take(max_keywords)
        .map(|(token, _)| token)
        .collect()
}

fn deterministic_tiers(uri: &AxiomUri, entries: &[TierEntry]) -> (String, String) {
    let abstract_text = format!("{uri} contains {} items", entries.len());
    let mut overview = format!("# {uri}\n\n");
    if entries.is_empty() {
        overview.push_str("(empty)\n");
    } else {
        overview.push_str("Contains:\n");
        for entry in entries.iter().take(50) {
            push_bullet_line(&mut overview, &entry.name);
        }
    }
    (abstract_text, overview)
}

fn semantic_tiers(uri: &AxiomUri, path: &Path, entries: &[TierEntry]) -> (String, String) {
    if entries.is_empty() {
        return deterministic_tiers(uri, entries);
    }

    let topics = tier_semantic_keywords(path, entries, 6);
    if topics.is_empty() {
        return deterministic_tiers(uri, entries);
    }

    let directory_count = entries.iter().filter(|entry| entry.is_dir).count();
    let file_count = entries.len().saturating_sub(directory_count);
    let abstract_text = format!(
        "{uri} semantic summary: {} items ({} directories, {} files); topics: {}",
        entries.len(),
        directory_count,
        file_count,
        topics.join(", ")
    );

    let mut overview = format!("# {uri}\n\n");
    overview.push_str("Summary:\n");
    push_bullet_line(&mut overview, &format!("topics: {}", topics.join(", ")));
    push_bullet_line(&mut overview, &format!("directories: {directory_count}"));
    let _ = writeln!(overview, "- files: {file_count}\n");
    overview.push_str("Contains:\n");
    for entry in entries.iter().take(50) {
        push_bullet_line(&mut overview, &entry.name);
    }

    (abstract_text, overview)
}

pub(super) fn synthesize_directory_tiers(
    uri: &AxiomUri,
    path: &Path,
    mode: TierSynthesisMode,
) -> Result<(String, String)> {
    let entries = list_visible_tier_entries(path)?;
    match mode {
        TierSynthesisMode::Deterministic => Ok(deterministic_tiers(uri, &entries)),
        TierSynthesisMode::SemanticLite => Ok(semantic_tiers(uri, path, &entries)),
    }
}
