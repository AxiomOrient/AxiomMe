use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

use walkdir::WalkDir;

use crate::context_ops::{RecordInput, build_record, classify_context, infer_tags};
use crate::error::{AxiomError, Result};
use crate::models::IndexRecord;
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

const TIER_SYNTHESIS_ENV: &str = "AXIOMME_TIER_SYNTHESIS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TierSynthesisMode {
    Deterministic,
    SemanticLite,
}

#[derive(Debug, Clone)]
struct TierEntry {
    name: String,
    is_dir: bool,
}

fn resolve_tier_synthesis_mode(raw: Option<&str>) -> TierSynthesisMode {
    match raw.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "semantic" || value == "semantic-lite" => {
            TierSynthesisMode::SemanticLite
        }
        _ => TierSynthesisMode::Deterministic,
    }
}

fn list_visible_tier_entries(path: &Path) -> Vec<TierEntry> {
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if matches!(
                name.as_str(),
                ".abstract.md" | ".overview.md" | ".meta.json"
            ) {
                continue;
            }
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            entries.push(TierEntry { name, is_dir });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
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
            overview.push_str(&format!("- {}\n", entry.name));
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
    overview.push_str(&format!("- topics: {}\n", topics.join(", ")));
    overview.push_str(&format!("- directories: {directory_count}\n"));
    overview.push_str(&format!("- files: {file_count}\n\n"));
    overview.push_str("Contains:\n");
    for entry in entries.iter().take(50) {
        overview.push_str(&format!("- {}\n", entry.name));
    }

    (abstract_text, overview)
}

fn synthesize_directory_tiers(
    uri: &AxiomUri,
    path: &Path,
    mode: TierSynthesisMode,
) -> (String, String) {
    let entries = list_visible_tier_entries(path);
    match mode {
        TierSynthesisMode::Deterministic => deterministic_tiers(uri, &entries),
        TierSynthesisMode::SemanticLite => semantic_tiers(uri, path, &entries),
    }
}

impl AxiomMe {
    pub(super) fn ensure_scope_tiers(&self) -> Result<()> {
        for scope in [
            Scope::Resources,
            Scope::User,
            Scope::Agent,
            Scope::Session,
            Scope::Temp,
            Scope::Queue,
        ] {
            let uri = AxiomUri::root(scope);
            self.ensure_directory_tiers(&uri)?;
        }
        Ok(())
    }

    pub(super) fn ensure_tiers_recursive(&self, root: &AxiomUri) -> Result<()> {
        let root_path = self.fs.resolve_uri(root);
        if !root_path.exists() {
            return Ok(());
        }

        for entry in WalkDir::new(&root_path)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if entry.path().is_dir() {
                let uri = self.fs.uri_from_path(entry.path())?;
                self.ensure_directory_tiers(&uri)?;
            }
        }

        Ok(())
    }

    pub(super) fn ensure_directory_tiers(&self, uri: &AxiomUri) -> Result<()> {
        let path = self.fs.resolve_uri(uri);
        if !path.exists() {
            fs::create_dir_all(&path)?;
        }

        let abs_path = self.fs.abstract_path(uri);
        let ov_path = self.fs.overview_path(uri);

        let mode = resolve_tier_synthesis_mode(std::env::var(TIER_SYNTHESIS_ENV).ok().as_deref());
        let (abstract_text, overview) = synthesize_directory_tiers(uri, &path, mode);

        let needs_write = if abs_path.exists() && ov_path.exists() {
            match (fs::read_to_string(&abs_path), fs::read_to_string(&ov_path)) {
                (Ok(existing_abs), Ok(existing_ov)) => {
                    existing_abs != abstract_text || existing_ov != overview
                }
                _ => true,
            }
        } else {
            true
        };

        if needs_write {
            self.fs.write_tiers(uri, &abstract_text, &overview)?;
        }

        Ok(())
    }

    pub(super) fn reindex_uri_tree(&self, root_uri: &AxiomUri) -> Result<()> {
        let root_path = self.fs.resolve_uri(root_uri);
        if !root_path.exists() {
            return Ok(());
        }

        self.ensure_tiers_recursive(root_uri)?;
        let mut mirror_batch = Vec::<IndexRecord>::new();

        let mut index = self
            .index
            .write()
            .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;

        for entry in WalkDir::new(&root_path)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let path = entry.path();
            let uri = self.fs.uri_from_path(path)?;

            if path.is_dir() {
                let abstract_text = self.fs.read_abstract(&uri).unwrap_or_default();
                let overview_text = self.fs.read_overview(&uri).unwrap_or_default();
                let record = build_record(RecordInput {
                    uri: &uri,
                    parent_uri: uri.parent().as_ref(),
                    is_leaf: false,
                    context_type: classify_context(&uri),
                    name: uri
                        .last_segment()
                        .unwrap_or(uri.scope().as_str())
                        .to_string(),
                    abstract_text,
                    content: overview_text,
                    tags: vec![],
                });

                let hash = blake3::hash(record.content.as_bytes()).to_hex().to_string();
                let uri_str = uri.to_string();
                let mtime = fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as i64)
                    .unwrap_or(0);

                let current_hash = self.state.get_index_state_hash(&uri_str)?;
                let needs_upsert =
                    current_hash.as_deref() != Some(hash.as_str()) || index.get(&uri_str).is_none();
                if needs_upsert {
                    self.state.upsert_search_document(&record)?;
                    mirror_batch.push(record.clone());
                    index.upsert(record);
                    if current_hash.as_deref() != Some(hash.as_str()) {
                        self.state
                            .upsert_index_state(&uri_str, &hash, mtime, "indexed")?;
                        self.state.mark_outbox_status(
                            self.state.enqueue(
                                "upsert",
                                &uri_str,
                                serde_json::json!({"kind": "dir"}),
                            )?,
                            "done",
                            false,
                        )?;
                    }
                }
            } else {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                if matches!(
                    name.as_str(),
                    ".abstract.md"
                        | ".overview.md"
                        | ".meta.json"
                        | ".relations.json"
                        | "messages.jsonl"
                ) {
                    continue;
                }
                let content = fs::read(path).unwrap_or_default();
                let text = String::from_utf8_lossy(&content).to_string();
                let abstract_text = text.lines().next().unwrap_or_default().to_string();
                let tags = infer_tags(&name, &text);
                let record = build_record(RecordInput {
                    uri: &uri,
                    parent_uri: uri.parent().as_ref(),
                    is_leaf: true,
                    context_type: classify_context(&uri),
                    name,
                    abstract_text,
                    content: text,
                    tags,
                });

                let hash = blake3::hash(&content).to_hex().to_string();
                let uri_str = uri.to_string();
                let mtime = fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as i64)
                    .unwrap_or(0);

                let current_hash = self.state.get_index_state_hash(&uri_str)?;
                let needs_upsert =
                    current_hash.as_deref() != Some(hash.as_str()) || index.get(&uri_str).is_none();
                if needs_upsert {
                    self.state.upsert_search_document(&record)?;
                    mirror_batch.push(record.clone());
                    index.upsert(record);
                    if current_hash.as_deref() != Some(hash.as_str()) {
                        self.state
                            .upsert_index_state(&uri_str, &hash, mtime, "indexed")?;
                        self.state.mark_outbox_status(
                            self.state.enqueue(
                                "upsert",
                                &uri_str,
                                serde_json::json!({"kind": "file"}),
                            )?,
                            "done",
                            false,
                        )?;
                    }
                }
            }
        }
        drop(index);

        for record in &mirror_batch {
            self.try_mirror_upsert(record, "reindex_uri_tree")?;
        }

        Ok(())
    }

    pub(super) fn reindex_scopes(&self, scopes: &[Scope]) -> Result<()> {
        let scope_set = scopes
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        {
            let mut index = self
                .index
                .write()
                .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
            let remove_uris = index
                .all_records()
                .into_iter()
                .map(|r| r.uri)
                .filter(|uri| {
                    AxiomUri::parse(uri)
                        .map(|parsed| scope_set.contains(&parsed.scope()))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            for uri in remove_uris {
                index.remove(&uri);
            }
        }

        self.ensure_qdrant_collection()?;
        for scope in scopes {
            self.reindex_uri_tree(&AxiomUri::root(scope.clone()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn resolve_tier_synthesis_mode_defaults_to_deterministic() {
        assert_eq!(
            resolve_tier_synthesis_mode(Some("semantic")),
            TierSynthesisMode::SemanticLite
        );
        assert_eq!(
            resolve_tier_synthesis_mode(Some("semantic-lite")),
            TierSynthesisMode::SemanticLite
        );
        assert_eq!(
            resolve_tier_synthesis_mode(Some("deterministic")),
            TierSynthesisMode::Deterministic
        );
        assert_eq!(
            resolve_tier_synthesis_mode(Some("")),
            TierSynthesisMode::Deterministic
        );
        assert_eq!(
            resolve_tier_synthesis_mode(None),
            TierSynthesisMode::Deterministic
        );
    }

    #[test]
    fn semantic_tier_synthesis_emits_summary_and_topics() {
        let temp = tempdir().expect("tempdir");
        let dir = temp.path().join("semantic-tier");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(
            dir.join("auth.md"),
            "OAuth authorization flow with token exchange",
        )
        .expect("write auth");
        fs::write(
            dir.join("storage.md"),
            "SQLite persistence cache storage guide",
        )
        .expect("write storage");

        let uri = AxiomUri::parse("axiom://resources/semantic-tier").expect("uri parse");
        let (abstract_text, overview) =
            synthesize_directory_tiers(&uri, &dir, TierSynthesisMode::SemanticLite);

        assert!(abstract_text.contains("semantic summary"));
        assert!(overview.contains("Summary:"));
        assert!(overview.contains("- topics:"));
        assert!(overview.contains("- auth.md"));
        assert!(overview.contains("- storage.md"));
    }

    #[test]
    fn semantic_tier_synthesis_falls_back_for_empty_directory() {
        let temp = tempdir().expect("tempdir");
        let dir = temp.path().join("empty-tier");
        fs::create_dir_all(&dir).expect("mkdir");

        let uri = AxiomUri::parse("axiom://resources/empty-tier").expect("uri parse");
        let (abstract_text, overview) =
            synthesize_directory_tiers(&uri, &dir, TierSynthesisMode::SemanticLite);

        assert_eq!(
            abstract_text,
            "axiom://resources/empty-tier contains 0 items"
        );
        assert!(overview.contains("(empty)"));
    }

    #[test]
    fn ensure_directory_tiers_rewrites_when_directory_contents_change() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app new");
        app.fs.initialize().expect("fs init");

        let uri = AxiomUri::parse("axiom://resources/tier-refresh").expect("uri parse");
        app.fs.create_dir_all(&uri, true).expect("mkdir");
        app.fs
            .write(&uri.join("alpha.md").expect("join"), "alpha payload", true)
            .expect("write alpha");

        app.ensure_directory_tiers(&uri).expect("first synth");
        let before = app.fs.read_overview(&uri).expect("before overview");

        app.fs
            .write(&uri.join("beta.md").expect("join"), "beta payload", true)
            .expect("write beta");
        app.ensure_directory_tiers(&uri).expect("second synth");
        let after = app.fs.read_overview(&uri).expect("after overview");

        assert_ne!(before, after);
        assert!(after.contains("beta.md"));
    }
}
