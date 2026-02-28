use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSetBuilder};
use walkdir::WalkDir;

use crate::error::{AxiomError, Result};
use crate::models::{Entry, TreeNode, TreeResult};
use crate::uri::{AxiomUri, Scope};

#[derive(Debug, Clone)]
pub struct LocalContextFs {
    root: PathBuf,
}

impl LocalContextFs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        for scope in [
            Scope::Resources,
            Scope::User,
            Scope::Agent,
            Scope::Session,
            Scope::Temp,
            Scope::Queue,
        ] {
            let path = self.root.join(scope.as_str());
            fs::create_dir_all(path)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn resolve_uri(&self, uri: &AxiomUri) -> PathBuf {
        let mut out = self.root.join(uri.scope().as_str());
        for segment in uri.segments() {
            out.push(segment);
        }
        out
    }

    pub fn uri_from_path(&self, path: &Path) -> Result<AxiomUri> {
        let relative = path.strip_prefix(&self.root).map_err(|_| {
            AxiomError::Validation(format!("path is outside root: {}", path.display()))
        })?;

        let mut components = relative.components();
        let scope = components
            .next()
            .ok_or_else(|| AxiomError::Validation("missing scope component".to_string()))?;

        let scope_str = match scope {
            Component::Normal(s) => s.to_string_lossy().to_string(),
            _ => {
                return Err(AxiomError::Validation(
                    "invalid scope component".to_string(),
                ));
            }
        };

        let mut uri = AxiomUri::parse(&format!("axiom://{scope_str}"))?;
        for comp in components {
            if let Component::Normal(s) = comp {
                uri = uri.join(&s.to_string_lossy())?;
            }
        }
        Ok(uri)
    }

    #[must_use]
    pub fn exists(&self, uri: &AxiomUri) -> bool {
        self.resolve_uri(uri).exists()
    }

    #[must_use]
    pub fn is_dir(&self, uri: &AxiomUri) -> bool {
        self.resolve_uri(uri).is_dir()
    }

    pub fn create_dir_all(&self, uri: &AxiomUri, system: bool) -> Result<()> {
        Self::ensure_writable(uri, system)?;
        let path = self.resolve_uri(uri);
        self.ensure_path_within_root(&path)?;
        fs::create_dir_all(path)?;
        Ok(())
    }

    pub fn read(&self, uri: &AxiomUri) -> Result<String> {
        let path = self.resolve_uri(uri);
        if !path.exists() {
            return Err(AxiomError::NotFound(uri.to_string()));
        }
        if path.is_dir() {
            return Err(AxiomError::Validation(format!(
                "cannot read directory: {uri}"
            )));
        }
        self.ensure_path_within_root(&path)?;
        Ok(fs::read_to_string(path)?)
    }

    pub fn write(&self, uri: &AxiomUri, content: &str, system: bool) -> Result<()> {
        Self::ensure_writable(uri, system)?;
        let path = self.resolve_uri(uri);
        self.ensure_path_within_root(&path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    pub fn write_atomic(&self, uri: &AxiomUri, content: &str, system: bool) -> Result<()> {
        Self::ensure_writable(uri, system)?;
        let path = self.resolve_uri(uri);
        self.ensure_path_within_root(&path)?;
        let parent = path
            .parent()
            .ok_or_else(|| AxiomError::Validation(format!("target has no parent: {uri}")))?;
        fs::create_dir_all(parent)?;

        let file_name = path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| AxiomError::Validation(format!("invalid target filename: {uri}")))?;
        let tmp_name = format!(".{file_name}.axiomme.tmp.{}", uuid::Uuid::new_v4().simple());
        let tmp_path = parent.join(tmp_name);
        self.ensure_path_within_root(&tmp_path)?;

        {
            let mut tmp = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp_path)?;
            tmp.write_all(content.as_bytes())?;
            tmp.sync_all()?;
        }

        if let Err(err) = fs::rename(&tmp_path, &path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(AxiomError::from(err));
        }

        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    pub fn append(&self, uri: &AxiomUri, content: &str, system: bool) -> Result<()> {
        Self::ensure_writable(uri, system)?;
        let path = self.resolve_uri(uri);
        self.ensure_path_within_root(&path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    pub fn write_bytes(&self, uri: &AxiomUri, bytes: &[u8], system: bool) -> Result<()> {
        Self::ensure_writable(uri, system)?;
        let path = self.resolve_uri(uri);
        self.ensure_path_within_root(&path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn read_bytes(&self, uri: &AxiomUri) -> Result<Vec<u8>> {
        let path = self.resolve_uri(uri);
        if !path.exists() {
            return Err(AxiomError::NotFound(uri.to_string()));
        }
        if path.is_dir() {
            return Err(AxiomError::Validation(format!(
                "cannot read directory: {uri}"
            )));
        }
        self.ensure_path_within_root(&path)?;
        Ok(fs::read(path)?)
    }

    pub fn list(&self, uri: &AxiomUri, recursive: bool) -> Result<Vec<Entry>> {
        let base = self.resolve_uri(uri);
        if !base.exists() {
            return Err(AxiomError::NotFound(uri.to_string()));
        }
        self.ensure_path_within_root(&base)?;
        let mut entries = Vec::new();

        if recursive {
            for item in WalkDir::new(&base).follow_links(false) {
                let item = item.map_err(|e| AxiomError::Validation(e.to_string()))?;
                if item.path() == base {
                    continue;
                }
                let meta = item
                    .metadata()
                    .map_err(|e| AxiomError::Validation(e.to_string()))?;
                let item_uri = self.uri_from_path(item.path())?;
                entries.push(Entry {
                    uri: item_uri.to_string(),
                    name: item.file_name().to_string_lossy().to_string(),
                    is_dir: meta.is_dir(),
                    size: if meta.is_file() { meta.len() } else { 0 },
                });
            }
        } else {
            for item in fs::read_dir(&base)? {
                let item = item?;
                let path = item.path();
                let meta = fs::symlink_metadata(&path)?;
                let item_uri = self.uri_from_path(&path)?;
                entries.push(Entry {
                    uri: item_uri.to_string(),
                    name: item.file_name().to_string_lossy().to_string(),
                    is_dir: meta.file_type().is_dir(),
                    size: if meta.is_file() { meta.len() } else { 0 },
                });
            }
        }

        entries.sort_by(|a, b| a.uri.cmp(&b.uri));
        Ok(entries)
    }

    pub fn glob(&self, uri: Option<&AxiomUri>, pattern: &str) -> Result<Vec<String>> {
        let base_uri = uri
            .cloned()
            .unwrap_or_else(|| AxiomUri::root(Scope::Resources));
        let base = self.resolve_uri(&base_uri);
        if !base.exists() {
            return Ok(Vec::new());
        }
        self.ensure_path_within_root(&base)?;

        let mut builder = GlobSetBuilder::new();
        builder.add(Glob::new(pattern).map_err(|e| AxiomError::Validation(e.to_string()))?);
        let matcher = builder
            .build()
            .map_err(|e| AxiomError::Validation(e.to_string()))?;

        let mut matched_uris = Vec::new();
        for item in WalkDir::new(&base).follow_links(false) {
            let item = item.map_err(|e| AxiomError::Validation(e.to_string()))?;
            if item.path() == base {
                continue;
            }
            let rel = item
                .path()
                .strip_prefix(&base)
                .map_err(|e| AxiomError::Validation(e.to_string()))?;
            if matcher.is_match(rel) {
                let item_uri = self.uri_from_path(item.path())?;
                matched_uris.push(item_uri.to_string());
            }
        }
        matched_uris.sort();
        Ok(matched_uris)
    }

    pub fn rm(&self, uri: &AxiomUri, recursive: bool, system: bool) -> Result<()> {
        Self::ensure_writable(uri, system)?;
        let path = self.resolve_uri(uri);
        if !path.exists() {
            return Ok(());
        }
        self.ensure_path_within_root(&path)?;
        if path.is_dir() {
            if recursive {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_dir(path)?;
            }
        } else {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn mv(&self, from: &AxiomUri, to: &AxiomUri, system: bool) -> Result<()> {
        Self::ensure_writable(from, system)?;
        Self::ensure_writable(to, system)?;
        let from_path = self.resolve_uri(from);
        let to_path = self.resolve_uri(to);
        self.ensure_path_within_root(&from_path)?;
        self.ensure_path_within_root(&to_path)?;
        if let Some(parent) = to_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(from_path, to_path)?;
        Ok(())
    }

    pub fn tree(&self, uri: &AxiomUri) -> Result<TreeResult> {
        let path = self.resolve_uri(uri);
        if !path.exists() {
            return Err(AxiomError::NotFound(uri.to_string()));
        }
        self.ensure_path_within_root(&path)?;
        let root = self.build_tree(uri, &path)?;
        Ok(TreeResult { root })
    }

    fn build_tree(&self, uri: &AxiomUri, path: &Path) -> Result<TreeNode> {
        let meta = fs::symlink_metadata(path)?;
        let is_dir = meta.file_type().is_dir();
        let mut node = TreeNode {
            uri: uri.to_string(),
            is_dir,
            children: Vec::new(),
        };

        if is_dir {
            let mut children = Vec::new();
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let child_path = entry.path();
                let child_uri = self.uri_from_path(&child_path)?;
                children.push(self.build_tree(&child_uri, &child_path)?);
            }
            children.sort_by(|a, b| a.uri.cmp(&b.uri));
            node.children = children;
        }

        Ok(node)
    }

    fn ensure_writable(uri: &AxiomUri, system: bool) -> Result<()> {
        if !system && matches!(uri.scope(), Scope::Queue) {
            return Err(AxiomError::PermissionDenied(
                "queue scope is read-only for non-system operations".to_string(),
            ));
        }
        Ok(())
    }

    fn ensure_path_within_root(&self, path: &Path) -> Result<()> {
        let root = self.canonical_root()?;
        let mut probe = path.to_path_buf();
        while !probe.exists() {
            if !probe.pop() {
                return Err(AxiomError::SecurityViolation(format!(
                    "path has no existing ancestor: {}",
                    path.display()
                )));
            }
        }

        let probe_canonical = fs::canonicalize(&probe)?;
        if !probe_canonical.starts_with(&root) {
            return Err(AxiomError::SecurityViolation(format!(
                "path escapes root boundary: {}",
                path.display()
            )));
        }

        if path.exists() {
            let path_canonical = fs::canonicalize(path)?;
            if !path_canonical.starts_with(&root) {
                return Err(AxiomError::SecurityViolation(format!(
                    "path resolves outside root boundary: {}",
                    path.display()
                )));
            }
        }

        Ok(())
    }

    fn canonical_root(&self) -> Result<PathBuf> {
        if !self.root.exists() {
            fs::create_dir_all(&self.root)?;
        }
        Ok(fs::canonicalize(&self.root)?)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::models::RelationLink;
    use crate::relation_documents::{read_relations, write_relations};
    use crate::tier_documents::{read_abstract, read_overview, write_tiers};

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn queue_scope_is_read_only() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let uri = AxiomUri::parse("axiom://queue/events.log").expect("parse failed");
        let err = fs.write(&uri, "x", false).expect_err("must fail");
        assert!(matches!(err, AxiomError::PermissionDenied(_)));
    }

    #[test]
    fn append_supports_incremental_log_writes() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let uri = AxiomUri::parse("axiom://queue/logs/requests.jsonl").expect("parse failed");
        fs.append(&uri, "{\"a\":1}\n", true).expect("append 1");
        fs.append(&uri, "{\"b\":2}\n", true).expect("append 2");
        let raw = fs.read(&uri).expect("read");
        assert!(raw.contains("{\"a\":1}"));
        assert!(raw.contains("{\"b\":2}"));
    }

    #[test]
    fn write_atomic_overwrites_existing_file() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let uri = AxiomUri::parse("axiom://resources/docs/atomic.md").expect("parse");
        fs.write(&uri, "v1", true).expect("write v1");
        fs.write_atomic(&uri, "v2", true).expect("write atomic");

        let raw = fs.read(&uri).expect("read");
        assert_eq!(raw, "v2");
    }

    #[cfg(unix)]
    #[test]
    fn write_rejects_symlink_escape_outside_root() {
        let temp = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let link_path = temp.path().join("resources").join("escape-link");
        symlink(outside.path(), &link_path).expect("symlink");

        let uri = AxiomUri::parse("axiom://resources/escape-link/pwned.txt").expect("parse uri");
        let err = fs.write(&uri, "owned", true).expect_err("must fail");
        assert!(matches!(err, AxiomError::SecurityViolation(_)));
    }

    #[cfg(unix)]
    #[test]
    fn read_rejects_symlink_escape_outside_root() {
        let temp = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "secret").expect("write outside");

        let link_path = temp.path().join("resources").join("secret-link.txt");
        symlink(&outside_file, &link_path).expect("symlink file");

        let uri = AxiomUri::parse("axiom://resources/secret-link.txt").expect("parse uri");
        let err = fs.read(&uri).expect_err("must fail");
        assert!(matches!(err, AxiomError::SecurityViolation(_)));
    }

    #[cfg(unix)]
    #[test]
    fn write_tiers_rejects_symlink_escape_outside_root() {
        let temp = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let link_path = temp.path().join("resources").join("escape-tiers");
        symlink(outside.path(), &link_path).expect("symlink");

        let uri = AxiomUri::parse("axiom://resources/escape-tiers").expect("parse uri");
        let err = write_tiers(&fs, &uri, "abstract", "overview", true).expect_err("must fail");
        assert!(matches!(err, AxiomError::SecurityViolation(_)));
    }

    #[cfg(unix)]
    #[test]
    fn read_tiers_reject_symlink_escape_outside_root() {
        let temp = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        fs::write(outside.path().join(".abstract.md"), "secret abstract").expect("write abstract");
        fs::write(outside.path().join(".overview.md"), "secret overview").expect("write overview");

        let link_path = temp.path().join("resources").join("escape-tiers");
        symlink(outside.path(), &link_path).expect("symlink");
        let uri = AxiomUri::parse("axiom://resources/escape-tiers").expect("parse uri");

        let abstract_err = read_abstract(&fs, &uri).expect_err("must fail abstract read");
        assert!(matches!(abstract_err, AxiomError::SecurityViolation(_)));
        let overview_err = read_overview(&fs, &uri).expect_err("must fail overview read");
        assert!(matches!(overview_err, AxiomError::SecurityViolation(_)));
    }

    #[test]
    fn relations_roundtrip_read_write() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let owner = AxiomUri::parse("axiom://resources/docs").expect("owner parse");
        let links = vec![RelationLink {
            id: "auth-security".to_string(),
            uris: vec![
                "axiom://resources/docs/auth".to_string(),
                "axiom://resources/docs/security".to_string(),
            ],
            reason: "Security dependency".to_string(),
        }];

        write_relations(&fs, &owner, &links, true).expect("write relations");
        let loaded = read_relations(&fs, &owner).expect("read relations");
        assert_eq!(loaded, links);
    }

    #[test]
    fn relations_reject_invalid_uri_schema() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let owner = AxiomUri::parse("axiom://resources/docs").expect("owner parse");
        let links = vec![RelationLink {
            id: "invalid".to_string(),
            uris: vec![
                "axiom://resources/docs/auth".to_string(),
                "not-a-axiom-uri".to_string(),
            ],
            reason: "Broken relation".to_string(),
        }];

        let err = write_relations(&fs, &owner, &links, true).expect_err("must reject invalid uri");
        assert!(matches!(err, AxiomError::Validation(_)));
    }

    #[test]
    fn relations_reject_duplicate_ids() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let owner = AxiomUri::parse("axiom://resources/docs").expect("owner parse");
        let links = vec![
            RelationLink {
                id: "dup".to_string(),
                uris: vec![
                    "axiom://resources/docs/auth".to_string(),
                    "axiom://resources/docs/security".to_string(),
                ],
                reason: "First".to_string(),
            },
            RelationLink {
                id: "dup".to_string(),
                uris: vec![
                    "axiom://resources/docs/auth".to_string(),
                    "axiom://resources/docs/api".to_string(),
                ],
                reason: "Second".to_string(),
            },
        ];

        let err = write_relations(&fs, &owner, &links, true).expect_err("must reject duplicate id");
        assert!(matches!(err, AxiomError::Validation(_)));
    }

    #[test]
    fn relations_owner_must_be_directory() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let file_uri = AxiomUri::parse("axiom://resources/docs/readme.md").expect("uri parse");
        fs.write(&file_uri, "hello", true).expect("write file");
        let err = read_relations(&fs, &file_uri).expect_err("must fail");
        assert!(matches!(err, AxiomError::Validation(_)));
    }
}
