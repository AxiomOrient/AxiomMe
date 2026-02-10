use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{AxiomError, Result};
use crate::fs::LocalContextFs;
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
    pub fn new(fs: LocalContextFs, parser: ParserRegistry) -> Self {
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
    pub fn ingest_id(&self) -> &str {
        &self.ingest_id
    }

    pub fn stage_local_path(&mut self, source: &Path) -> Result<()> {
        let staged_path = self.fs.resolve_uri(&self.staged_uri);
        if source.is_file() {
            let name = source
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| AxiomError::Validation("invalid source file name".to_string()))?;
            fs::copy(source, staged_path.join(name))?;
            return Ok(());
        }

        copy_dir_contents(source, &staged_path)
    }

    pub fn stage_text(&mut self, file_name: &str, text: &str) -> Result<()> {
        let staged_path = self.fs.resolve_uri(&self.staged_uri);
        fs::write(staged_path.join(file_name), text)?;
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

    pub fn finalize_to(&mut self, target_uri: &AxiomUri) -> Result<()> {
        let staged_path = self.fs.resolve_uri(&self.staged_uri);
        let target_path = self.fs.resolve_uri(target_uri);

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if target_path.exists() {
            if target_path.is_dir() {
                fs::remove_dir_all(&target_path)?;
            } else {
                fs::remove_file(&target_path)?;
            }
        }

        fs::rename(&staged_path, &target_path)?;
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

    for entry in WalkDir::new(staged_path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
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

    for entry in WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

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
        session.finalize_to(&target).expect("finalize");

        assert!(fs.resolve_uri(&target).join("a.md").exists());
        let temp_root = fs.resolve_uri(&AxiomUri::parse("axiom://temp/ingest").expect("temp uri"));
        let entries = fs::read_dir(&temp_root).expect("read temp root");
        assert_eq!(entries.count(), 0);
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
}
