use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{AxiomError, Result};
use crate::fs::LocalContextFs;
use crate::models::AddResourceIngestOptions;
use crate::parse::ParserRegistry;
use crate::uri::{AxiomUri, Scope};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestManifest {
    pub ingest_id: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub files: Vec<IngestFileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestFileInfo {
    pub relative_path: String,
    pub parser: String,
    pub is_text: bool,
    pub bytes: u64,
    pub line_count: usize,
    pub content_hash: String,
    pub title: Option<String>,
    pub preview: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestFinalizeMode {
    ReplaceTarget,
    MergeIntoTarget,
}

#[derive(Clone)]
pub struct IngestManager {
    fs: LocalContextFs,
    parser: ParserRegistry,
}

impl std::fmt::Debug for IngestManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestManager").finish_non_exhaustive()
    }
}

impl IngestManager {
    #[must_use]
    pub const fn new(fs: LocalContextFs, parser: ParserRegistry) -> Self {
        Self { fs, parser }
    }

    pub fn start_session(&self) -> Result<IngestSession> {
        let ingest_id = uuid::Uuid::new_v4().to_string();
        let root_uri = AxiomUri::root(Scope::Temp)
            .join("ingest")?
            .join(&ingest_id)?;
        let staged_uri = root_uri.join("staged")?;
        self.fs.create_dir_all(&staged_uri, true)?;

        Ok(IngestSession {
            fs: self.fs.clone(),
            parser: self.parser.clone(),
            ingest_id,
            root_uri,
            staged_uri,
            finalized: false,
        })
    }
}

pub struct IngestSession {
    fs: LocalContextFs,
    parser: ParserRegistry,
    ingest_id: String,
    root_uri: AxiomUri,
    staged_uri: AxiomUri,
    finalized: bool,
}

impl std::fmt::Debug for IngestSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestSession")
            .field("ingest_id", &self.ingest_id)
            .field("root_uri", &self.root_uri)
            .field("staged_uri", &self.staged_uri)
            .finish_non_exhaustive()
    }
}

impl IngestSession {
    #[must_use]
    pub fn ingest_id(&self) -> &str {
        &self.ingest_id
    }

    pub fn stage_local_path(&mut self, source: &Path) -> Result<()> {
        self.stage_local_path_with_options(source, &AddResourceIngestOptions::default())
    }

    pub fn stage_local_path_with_options(
        &mut self,
        source: &Path,
        options: &AddResourceIngestOptions,
    ) -> Result<()> {
        let staged_path = self.fs.resolve_uri(&self.staged_uri);
        if source.is_dir() && options == &AddResourceIngestOptions::default() {
            return copy_dir_contents(source, &staged_path);
        }
        let filter = IngestPathFilter::new(options)?;
        if source.is_file() {
            let name = source
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| AxiomError::Validation("invalid source file name".to_string()))?;
            if !filter.allows_file(Path::new(name)) {
                return Err(AxiomError::Validation(format!(
                    "source file excluded by ingest options: {name}"
                )));
            }
            fs::copy(source, staged_path.join(name))?;
            return Ok(());
        }

        let copied = copy_dir_contents_filtered(source, &staged_path, &filter)?;
        if copied == 0 {
            return Err(AxiomError::Validation(
                "ingest filter excluded all files in source directory".to_string(),
            ));
        }
        Ok(())
    }

    pub fn stage_text(&mut self, file_name: &str, text: &str) -> Result<()> {
        if file_name.trim().is_empty() {
            return Err(AxiomError::Validation(
                "stage_text file_name must not be empty".to_string(),
            ));
        }
        if Path::new(file_name).is_absolute() {
            return Err(AxiomError::PathTraversal(file_name.to_string()));
        }
        let target_uri = self.staged_uri.join(file_name)?;
        if target_uri == self.staged_uri {
            return Err(AxiomError::Validation(
                "stage_text file_name must include at least one path segment".to_string(),
            ));
        }
        self.fs.write(&target_uri, text, true)?;
        Ok(())
    }

    pub fn write_manifest(&self, source: &str) -> Result<IngestManifest> {
        let staged_path = self.fs.resolve_uri(&self.staged_uri);
        let files = scan_manifest_files(&self.parser, &staged_path)?;

        let manifest = IngestManifest {
            ingest_id: self.ingest_id.clone(),
            source: source.to_string(),
            created_at: Utc::now(),
            files,
        };

        let manifest_uri = self.root_uri.join("manifest.json")?;
        self.fs.write(
            &manifest_uri,
            &serde_json::to_string_pretty(&manifest)?,
            true,
        )?;

        Ok(manifest)
    }

    pub fn finalize_to(&mut self, target_uri: &AxiomUri, mode: IngestFinalizeMode) -> Result<()> {
        let staged_path = self.fs.resolve_uri(&self.staged_uri);
        let target_path = self.fs.resolve_uri(target_uri);

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        match mode {
            IngestFinalizeMode::ReplaceTarget => {
                remove_path_if_exists(&target_path)?;
                fs::rename(&staged_path, &target_path)?;
            }
            IngestFinalizeMode::MergeIntoTarget => {
                if !target_path.exists() {
                    fs::rename(&staged_path, &target_path)?;
                } else {
                    let staged_metadata = fs::symlink_metadata(&staged_path)?;
                    let target_metadata = fs::symlink_metadata(&target_path)?;
                    if staged_metadata.is_dir() && target_metadata.is_dir() {
                        merge_directory_contents(&staged_path, &target_path)?;
                        fs::remove_dir_all(&staged_path)?;
                    } else {
                        remove_path_if_exists(&target_path)?;
                        fs::rename(&staged_path, &target_path)?;
                    }
                }
            }
        }
        self.finalized = true;

        // Cleanup session root after staged folder has moved.
        self.fs.rm(&self.root_uri, true, true)?;
        Ok(())
    }

    pub fn abort(&mut self) {
        let _ = self.fs.rm(&self.root_uri, true, true);
    }
}

impl Drop for IngestSession {
    fn drop(&mut self) {
        if !self.finalized {
            let root: PathBuf = self.fs.resolve_uri(&self.root_uri);
            if root.exists() {
                let _ = fs::remove_dir_all(root);
            }
        }
    }
}

fn scan_manifest_files(parser: &ParserRegistry, staged_path: &Path) -> Result<Vec<IngestFileInfo>> {
    let mut out = Vec::new();

    for entry in WalkDir::new(staged_path).follow_links(false) {
        let entry = entry.map_err(|e| AxiomError::Validation(e.to_string()))?;
        if entry.path().is_dir() {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(staged_path)
            .map_err(|e| AxiomError::Validation(e.to_string()))?
            .to_string_lossy()
            .to_string();

        let bytes = fs::read(entry.path())?;
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let parsed = parser.parse_file(entry.path(), &bytes);

        out.push(IngestFileInfo {
            relative_path: rel,
            parser: parsed.parser,
            is_text: parsed.is_text,
            bytes: bytes.len() as u64,
            line_count: parsed.line_count,
            content_hash: hash,
            title: parsed.title,
            preview: parsed.text_preview,
            tags: parsed.tags,
        });
    }

    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(out)
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in WalkDir::new(src).follow_links(false) {
        let entry = entry.map_err(|e| AxiomError::Validation(e.to_string()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(src)
            .map_err(|e| AxiomError::Validation(e.to_string()))?;
        if rel.as_os_str().is_empty() {
            continue;
        }

        let out = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, out)?;
        }
    }

    Ok(())
}

fn merge_directory_contents(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if target_path.exists() && !fs::symlink_metadata(&target_path)?.is_dir() {
                remove_path_if_exists(&target_path)?;
            }
            merge_directory_contents(&source_path, &target_path)?;
            continue;
        }

        remove_path_if_exists(&target_path)?;
        fs::rename(&source_path, &target_path)?;
    }

    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[derive(Debug)]
struct IngestPathFilter {
    markdown_only: bool,
    include_hidden: bool,
    exclude: GlobSet,
}

impl IngestPathFilter {
    fn new(options: &AddResourceIngestOptions) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &options.exclude_globs {
            let trimmed = pattern.trim();
            if trimmed.is_empty() {
                continue;
            }
            let glob = Glob::new(trimmed).map_err(|err| {
                AxiomError::Validation(format!("invalid ingest exclude glob '{trimmed}': {err}"))
            })?;
            builder.add(glob);
        }

        let exclude = builder.build().map_err(|err| {
            AxiomError::Validation(format!("invalid ingest exclude globs: {err}"))
        })?;

        Ok(Self {
            markdown_only: options.markdown_only,
            include_hidden: options.include_hidden,
            exclude,
        })
    }

    fn allows_directory(&self, relative: &Path) -> bool {
        if relative.as_os_str().is_empty() {
            return true;
        }
        if !self.include_hidden && path_has_hidden_component(relative) {
            return false;
        }
        !self.exclude.is_match(relative_to_unix_path(relative))
    }

    fn allows_file(&self, relative: &Path) -> bool {
        if !self.include_hidden && path_has_hidden_component(relative) {
            return false;
        }
        if self.exclude.is_match(relative_to_unix_path(relative)) {
            return false;
        }
        if !self.markdown_only {
            return true;
        }
        relative
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| matches!(x.to_ascii_lowercase().as_str(), "md" | "markdown"))
            .unwrap_or(false)
    }
}

fn copy_dir_contents_filtered(src: &Path, dst: &Path, filter: &IngestPathFilter) -> Result<usize> {
    fs::create_dir_all(dst)?;
    let mut copied_files = 0usize;

    let entries = WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.path() == src {
                return true;
            }
            let Ok(relative) = entry.path().strip_prefix(src) else {
                return true;
            };
            if entry.file_type().is_dir() {
                filter.allows_directory(relative)
            } else {
                true
            }
        });

    for entry in entries {
        let entry = entry.map_err(|e| AxiomError::Validation(e.to_string()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(src)
            .map_err(|e| AxiomError::Validation(e.to_string()))?;
        if rel.as_os_str().is_empty() {
            continue;
        }

        let out = dst.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if !filter.allows_file(rel) {
            continue;
        }

        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, out)?;
        copied_files += 1;
    }

    Ok(copied_files)
}

fn path_has_hidden_component(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(value) => value.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

fn relative_to_unix_path(relative: &Path) -> String {
    relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::models::AddResourceIngestOptions;

    #[test]
    fn staged_ingest_finalize_moves_tree_and_cleans_temp() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init");

        let source = temp.path().join("source");
        fs::create_dir_all(&source).expect("mkdir source");
        fs::write(source.join("a.md"), "# A\ncontent").expect("write source");

        let manager = IngestManager::new(fs.clone(), ParserRegistry::new());
        let mut session = manager.start_session().expect("start session");

        session.stage_local_path(&source).expect("stage local");
        let manifest = session.write_manifest("local://source").expect("manifest");
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].parser, "markdown");

        let target = AxiomUri::parse("axiom://resources/demo").expect("target uri");
        session
            .finalize_to(&target, IngestFinalizeMode::ReplaceTarget)
            .expect("finalize");

        assert!(fs.resolve_uri(&target).join("a.md").exists());
        let temp_root = fs.resolve_uri(&AxiomUri::parse("axiom://temp/ingest").expect("temp uri"));
        let entries = fs::read_dir(&temp_root).expect("read temp root");
        assert_eq!(entries.count(), 0);
    }

    #[test]
    fn staged_ingest_finalize_merges_existing_target_directory() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init");

        let manager = IngestManager::new(fs.clone(), ParserRegistry::new());
        let target = AxiomUri::parse("axiom://resources/merge-demo").expect("target uri");

        let mut first = manager.start_session().expect("first session");
        first.stage_text("a.md", "# A").expect("stage a");
        first
            .finalize_to(&target, IngestFinalizeMode::MergeIntoTarget)
            .expect("finalize first");

        let mut second = manager.start_session().expect("second session");
        second.stage_text("b.md", "# B").expect("stage b");
        second
            .finalize_to(&target, IngestFinalizeMode::MergeIntoTarget)
            .expect("finalize second");

        let root = fs.resolve_uri(&target);
        assert!(root.join("a.md").exists());
        assert!(root.join("b.md").exists());
    }

    #[test]
    fn drop_without_finalize_cleans_temp_session() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init");

        {
            let manager = IngestManager::new(fs.clone(), ParserRegistry::new());
            let mut session = manager.start_session().expect("start");
            session
                .stage_text("source.txt", "hello world")
                .expect("stage text");
            session.write_manifest("inline").expect("manifest");
        }

        let temp_root = fs.resolve_uri(&AxiomUri::parse("axiom://temp/ingest").expect("temp uri"));
        let entries = fs::read_dir(temp_root).expect("read temp root");
        assert_eq!(entries.count(), 0);
    }

    #[test]
    fn stage_text_rejects_parent_traversal() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init");

        let manager = IngestManager::new(fs, ParserRegistry::new());
        let mut session = manager.start_session().expect("start");
        let err = session
            .stage_text("../escape.txt", "owned")
            .expect_err("must reject traversal");
        assert!(matches!(err, AxiomError::PathTraversal(_)));
    }

    #[test]
    fn stage_text_rejects_empty_normalized_name() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init");

        let manager = IngestManager::new(fs, ParserRegistry::new());
        let mut session = manager.start_session().expect("start");
        let err = session
            .stage_text("/", "owned")
            .expect_err("must reject empty normalized target");
        assert!(matches!(
            err,
            AxiomError::Validation(_) | AxiomError::PathTraversal(_)
        ));
    }

    #[test]
    fn stage_local_path_with_markdown_only_filters_hidden_and_json_files() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init");

        let source = temp.path().join("source");
        fs::create_dir_all(source.join("nested")).expect("mkdir nested");
        fs::create_dir_all(source.join(".obsidian")).expect("mkdir hidden");
        fs::write(source.join("note.md"), "# note").expect("write note");
        fs::write(source.join("meta.json"), "{\"ok\":true}").expect("write json");
        fs::write(source.join("nested").join("todo.markdown"), "# todo").expect("write nested md");
        fs::write(source.join(".obsidian").join("cache.md"), "# hidden").expect("write hidden md");

        let manager = IngestManager::new(fs.clone(), ParserRegistry::new());
        let mut session = manager.start_session().expect("start");
        session
            .stage_local_path_with_options(
                &source,
                &AddResourceIngestOptions::markdown_only_defaults(),
            )
            .expect("stage filtered");
        let manifest = session.write_manifest("local://source").expect("manifest");

        let staged_files = manifest
            .files
            .iter()
            .map(|x| x.relative_path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(staged_files.len(), 2);
        assert!(staged_files.contains(&"note.md"));
        assert!(staged_files.contains(&"nested/todo.markdown"));
    }

    #[test]
    fn stage_local_file_with_markdown_only_rejects_non_markdown_source() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init");

        let file = temp.path().join("data.json");
        fs::write(&file, "{\"x\":1}").expect("write");

        let manager = IngestManager::new(fs, ParserRegistry::new());
        let mut session = manager.start_session().expect("start");
        let err = session
            .stage_local_path_with_options(
                &file,
                &AddResourceIngestOptions::markdown_only_defaults(),
            )
            .expect_err("non-markdown file must be rejected");
        assert!(matches!(err, AxiomError::Validation(_)));
    }
}
