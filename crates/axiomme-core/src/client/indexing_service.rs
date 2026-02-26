use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::config::{InternalTierPolicy, TierSynthesisMode, should_persist_scope_tiers};
#[cfg(test)]
use crate::config::{resolve_internal_tier_policy, resolve_tier_synthesis_mode};
use crate::context_ops::{RecordInput, build_record, classify_context, infer_tags};
use crate::error::{AxiomError, Result};
use crate::models::IndexRecord;
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;
mod helpers;
#[cfg(test)]
mod tests;

use helpers::{
    MAX_INDEX_READ_BYTES, MAX_TRUNCATED_MARKDOWN_TAIL_HEADING_KEYS,
    collect_markdown_tail_heading_keys, directory_record_name, index_state_changed,
    is_markdown_file, metadata_mtime_nanos, path_mtime_nanos, read_index_source_bytes,
    should_skip_indexing_file, synthesize_directory_tiers,
};

fn append_truncated_markdown_heading_index(text: &mut String, headings: &[String]) {
    if headings.is_empty() {
        return;
    }
    text.push_str("\n\n[index markdown heading keys]\n");
    for heading in headings {
        text.push_str("- ");
        text.push_str(heading);
        text.push('\n');
    }
}

impl AxiomMe {
    fn prune_generated_tiers_recursive(&self, root: &AxiomUri) -> Result<usize> {
        let root_path = self.fs.resolve_uri(root);
        if !root_path.exists() {
            return Ok(0);
        }

        let mut removed = 0usize;
        for entry in WalkDir::new(&root_path).follow_links(false) {
            let entry = entry.map_err(|e| AxiomError::Validation(e.to_string()))?;
            if !entry.path().is_dir() {
                continue;
            }
            for generated_name in [".abstract.md", ".overview.md"] {
                let generated_path = entry.path().join(generated_name);
                if generated_path.exists() {
                    fs::remove_file(generated_path)?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    pub(super) fn ensure_scope_tiers(&self) -> Result<()> {
        let internal_policy = self.config.indexing.internal_tier_policy;
        for scope in [
            Scope::Resources,
            Scope::User,
            Scope::Agent,
            Scope::Session,
            Scope::Temp,
            Scope::Queue,
        ] {
            if !should_persist_scope_tiers(scope, internal_policy) {
                self.prune_generated_tiers_recursive(&AxiomUri::root(scope))?;
                continue;
            }
            let uri = AxiomUri::root(scope);
            self.ensure_directory_tiers(&uri)?;
        }
        Ok(())
    }

    pub(super) fn ensure_tiers_recursive(&self, root: &AxiomUri) -> Result<()> {
        let internal_policy = self.config.indexing.internal_tier_policy;
        if !should_persist_scope_tiers(root.scope(), internal_policy) {
            return Ok(());
        }

        let root_path = self.fs.resolve_uri(root);
        if !root_path.exists() {
            return Ok(());
        }

        for entry in WalkDir::new(&root_path).follow_links(false) {
            let entry = entry.map_err(|e| AxiomError::Validation(e.to_string()))?;
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

        let mode = self.config.indexing.tier_synthesis_mode;
        let (abstract_text, overview) = synthesize_directory_tiers(uri, &path, mode)?;

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

    fn maybe_upsert_index_record(
        &self,
        record: IndexRecord,
        hash: &str,
        mtime: i64,
        outbox_kind: &str,
    ) -> Result<()> {
        let uri = record.uri.clone();
        let current_state = self.state.get_index_state(&uri)?;
        let state_changed = index_state_changed(current_state.as_ref(), hash, mtime);
        let index_missing = self
            .index
            .read()
            .map_err(|_| AxiomError::lock_poisoned("index"))?
            .get(&uri)
            .is_none();
        let needs_upsert = state_changed || index_missing;
        if !needs_upsert {
            return Ok(());
        }

        self.state.upsert_search_document(&record)?;
        self.index
            .write()
            .map_err(|_| AxiomError::lock_poisoned("index"))?
            .upsert(record);

        if state_changed {
            self.state
                .upsert_index_state(&uri, hash, mtime, "indexed")?;
            let event_id =
                self.state
                    .enqueue("upsert", &uri, serde_json::json!({"kind": outbox_kind}))?;
            self.state.mark_outbox_status(event_id, "done", false)?;
        }
        Ok(())
    }

    fn load_directory_tiers_for_index(
        &self,
        uri: &AxiomUri,
        path: &Path,
        internal_policy: InternalTierPolicy,
        tier_mode: TierSynthesisMode,
    ) -> Result<(String, String)> {
        if !should_persist_scope_tiers(uri.scope(), internal_policy) {
            return synthesize_directory_tiers(uri, path, tier_mode);
        }
        if let (Ok(abstract_text), Ok(overview_text)) =
            (self.fs.read_abstract(uri), self.fs.read_overview(uri))
        {
            return Ok((abstract_text, overview_text));
        }

        let (abstract_text, overview_text) = synthesize_directory_tiers(uri, path, tier_mode)?;
        self.fs.write_tiers(uri, &abstract_text, &overview_text)?;
        Ok((abstract_text, overview_text))
    }

    fn index_directory_entry(
        &self,
        uri: &AxiomUri,
        path: &Path,
        internal_policy: InternalTierPolicy,
        tier_mode: TierSynthesisMode,
    ) -> Result<()> {
        let (abstract_text, overview_text) =
            self.load_directory_tiers_for_index(uri, path, internal_policy, tier_mode)?;
        let record = build_record(RecordInput {
            uri,
            parent_uri: uri.parent().as_ref(),
            is_leaf: false,
            context_type: classify_context(uri),
            name: directory_record_name(uri),
            abstract_text,
            content: overview_text,
            tags: vec![],
        });
        let hash = blake3::hash(record.content.as_bytes()).to_hex().to_string();
        let mtime = path_mtime_nanos(path);
        self.maybe_upsert_index_record(record, &hash, mtime, "dir")
    }

    fn index_file_entry(&self, uri: &AxiomUri, path: &Path) -> Result<()> {
        let name = path
            .file_name()
            .and_then(|segment| segment.to_str())
            .unwrap_or_default()
            .to_string();
        if should_skip_indexing_file(&name) {
            return Ok(());
        }

        let metadata = fs::metadata(path)?;
        let mtime = metadata_mtime_nanos(&metadata);
        let (content, truncated) = read_index_source_bytes(path, MAX_INDEX_READ_BYTES)?;
        let parsed = self.parser_registry.parse_file(path, &content);
        let crate::parse::ParsedDocument {
            is_text,
            title,
            text_preview,
            normalized_text,
            tags: parsed_tags,
            ..
        } = parsed;

        let mut text = if is_text {
            normalized_text.unwrap_or_else(|| String::from_utf8_lossy(&content).to_string())
        } else {
            text_preview
        };
        if truncated {
            let _ = write!(
                text,
                "\n\n[indexing truncated at {MAX_INDEX_READ_BYTES} bytes]"
            );
            if is_markdown_file(&name) {
                let tail_headings = collect_markdown_tail_heading_keys(
                    path,
                    MAX_TRUNCATED_MARKDOWN_TAIL_HEADING_KEYS,
                )?;
                if !tail_headings.is_empty() {
                    let text_lower = text.to_lowercase();
                    let missing_tail_headings = tail_headings
                        .into_iter()
                        .filter(|heading| !text_lower.contains(&heading.to_lowercase()))
                        .collect::<Vec<_>>();
                    append_truncated_markdown_heading_index(&mut text, &missing_tail_headings);
                }
            }
        }

        let abstract_text = title
            .or_else(|| text.lines().next().map(ToString::to_string))
            .unwrap_or_else(|| "content truncated for indexing".to_string());
        let mut tags = infer_tags(&name, &text);
        tags.extend(parsed_tags);
        tags.sort();
        tags.dedup();
        let record = build_record(RecordInput {
            uri,
            parent_uri: uri.parent().as_ref(),
            is_leaf: true,
            context_type: classify_context(uri),
            name,
            abstract_text,
            content: text,
            tags,
        });

        let hash = if truncated {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&content);
            hasher.update(b"|truncated|");
            hasher.update(&metadata.len().to_le_bytes());
            hasher.finalize().to_hex().to_string()
        } else {
            blake3::hash(&content).to_hex().to_string()
        };
        self.maybe_upsert_index_record(record, &hash, mtime, "file")
    }

    pub(super) fn reindex_uri_tree(&self, root_uri: &AxiomUri) -> Result<()> {
        if root_uri.scope().is_internal() {
            return Ok(());
        }
        let root_path = self.fs.resolve_uri(root_uri);
        if !root_path.exists() {
            return Ok(());
        }

        let internal_policy = self.config.indexing.internal_tier_policy;
        let tier_mode = self.config.indexing.tier_synthesis_mode;
        if should_persist_scope_tiers(root_uri.scope(), internal_policy) {
            self.ensure_tiers_recursive(root_uri)?;
        }

        for entry in WalkDir::new(&root_path).follow_links(false) {
            let entry = entry.map_err(|e| AxiomError::Validation(e.to_string()))?;
            let path = entry.path();
            if entry.file_type().is_symlink() {
                continue;
            }
            let uri = self.fs.uri_from_path(path)?;

            if entry.file_type().is_dir() {
                self.index_directory_entry(&uri, path, internal_policy, tier_mode)?;
                continue;
            }

            self.index_file_entry(&uri, path)?;
        }

        Ok(())
    }

    pub(super) fn reindex_document_with_ancestors(&self, leaf_uri: &AxiomUri) -> Result<()> {
        if leaf_uri.scope().is_internal() {
            return Ok(());
        }
        let Some(parent_uri) = leaf_uri.parent() else {
            return Err(AxiomError::Validation(format!(
                "targeted reindex requires non-root document uri: {leaf_uri}"
            )));
        };
        let leaf_path = self.fs.resolve_uri(leaf_uri);
        if !leaf_path.exists() || !leaf_path.is_file() {
            return Err(AxiomError::Validation(format!(
                "targeted reindex requires existing file target: {leaf_uri}"
            )));
        }

        let internal_policy = self.config.indexing.internal_tier_policy;
        let tier_mode = self.config.indexing.tier_synthesis_mode;
        self.index_file_entry(leaf_uri, &leaf_path)?;

        for dir_uri in directory_ancestor_chain(&parent_uri) {
            let dir_path = self.fs.resolve_uri(&dir_uri);
            if !dir_path.exists() || !dir_path.is_dir() {
                continue;
            }
            self.index_directory_entry(&dir_uri, &dir_path, internal_policy, tier_mode)?;
        }

        Ok(())
    }

    pub(super) fn reindex_scopes(&self, scopes: &[Scope]) -> Result<()> {
        let scope_set = scopes
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        {
            let mut index = self
                .index
                .write()
                .map_err(|_| AxiomError::lock_poisoned("index"))?;
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

        for scope in scopes {
            self.reindex_uri_tree(&AxiomUri::root(*scope))?;
        }
        Ok(())
    }
}

fn directory_ancestor_chain(start: &AxiomUri) -> Vec<AxiomUri> {
    let mut out = Vec::<AxiomUri>::new();
    let mut cursor = Some(start.clone());
    while let Some(uri) = cursor {
        out.push(uri.clone());
        cursor = uri.parent();
    }
    out
}
