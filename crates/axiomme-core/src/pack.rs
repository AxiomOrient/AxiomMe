use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{AxiomError, Result};
use crate::fs::LocalContextFs;
use crate::uri::AxiomUri;

pub fn export_ovpack(
    fs: &LocalContextFs,
    source: &AxiomUri,
    destination: &Path,
) -> Result<PathBuf> {
    let source_path = fs.resolve_uri(source);
    if !source_path.exists() {
        return Err(AxiomError::NotFound(source.to_string()));
    }
    if !source_path.is_dir() {
        return Err(AxiomError::Validation(
            "ovpack export source must be a directory".to_string(),
        ));
    }

    let mut out_path = destination.to_path_buf();
    if out_path.extension().is_none()
        || out_path.extension().and_then(|s| s.to_str()) != Some("ovpack")
    {
        out_path.set_extension("ovpack");
    }
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::create(&out_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let base_name = source
        .last_segment()
        .map(ToString::to_string)
        .unwrap_or_else(|| source.scope().as_str().to_string());
    let transformed_root = transform_component(&base_name);

    zip.add_directory(format!("{}/", transformed_root), options)?;

    for entry in WalkDir::new(&source_path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if entry.path() == source_path {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(&source_path)
            .map_err(|e| AxiomError::Validation(e.to_string()))?;

        let transformed_rel = rel
            .components()
            .map(|c| transform_component(&c.as_os_str().to_string_lossy()))
            .collect::<Vec<_>>()
            .join("/");

        let zip_path = format!("{}/{}", transformed_root, transformed_rel);
        if entry.path().is_dir() {
            zip.add_directory(format!("{}/", zip_path), options)?;
        } else {
            zip.start_file(zip_path, options)?;
            let bytes = fs::read(entry.path())?;
            zip.write_all(&bytes)?;
        }
    }

    zip.finish()?;
    Ok(out_path)
}

pub fn import_ovpack(
    fs: &LocalContextFs,
    file_path: &Path,
    parent: &AxiomUri,
    force: bool,
) -> Result<AxiomUri> {
    if !file_path.exists() {
        return Err(AxiomError::NotFound(file_path.display().to_string()));
    }

    let file = fs::File::open(file_path)?;
    let mut archive = ZipArchive::new(file)?;
    if archive.is_empty() {
        return Err(AxiomError::InvalidArchive("empty archive".to_string()));
    }

    let root_component = {
        let first = archive.by_index(0)?.name().to_string();
        first
            .split('/')
            .find(|s| !s.is_empty())
            .ok_or_else(|| AxiomError::InvalidArchive("archive has invalid root".to_string()))?
            .to_string()
    };

    let base_name = reverse_component(&root_component);
    let target_root = parent.join(&base_name)?;

    if fs.exists(&target_root) {
        if !force {
            return Err(AxiomError::Conflict(format!(
                "target exists: {}",
                target_root
            )));
        }
        fs.rm(&target_root, true, true)?;
    }

    fs.create_dir_all(&target_root, true)?;

    let target_root_path = fs.resolve_uri(&target_root);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if name.contains('\\') {
            return Err(AxiomError::SecurityViolation(format!(
                "backslash archive entry: {}",
                name
            )));
        }

        if name.starts_with('/') || looks_like_windows_abs(&name) {
            return Err(AxiomError::SecurityViolation(format!(
                "absolute archive entry: {}",
                name
            )));
        }

        let raw_parts = name
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if raw_parts.is_empty() {
            continue;
        }
        if raw_parts[0] != root_component {
            return Err(AxiomError::InvalidArchive(format!(
                "archive root mismatch: expected {}, got {}",
                root_component, raw_parts[0]
            )));
        }

        let mut parts = Vec::new();
        for raw in raw_parts {
            let reversed = reverse_component(raw);
            if reversed == "." || reversed == ".." {
                return Err(AxiomError::SecurityViolation(format!(
                    "traversal archive entry: {}",
                    name
                )));
            }
            parts.push(reversed);
        }

        if parts.is_empty() {
            continue;
        }

        // parts[0] is root folder from archive. We map that to target_root.
        let mut dest = target_root_path.clone();
        for part in parts.iter().skip(1) {
            if part.contains(std::path::MAIN_SEPARATOR) {
                return Err(AxiomError::SecurityViolation(format!(
                    "invalid path separator in entry: {}",
                    name
                )));
            }
            dest.push(part);
        }

        if !dest.starts_with(&target_root_path) {
            return Err(AxiomError::SecurityViolation(format!(
                "zip-slip attempt detected: {}",
                name
            )));
        }

        if entry.is_dir() || name.ends_with('/') {
            fs::create_dir_all(&dest)?;
            continue;
        }

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        fs::write(dest, buf)?;
    }

    Ok(target_root)
}

fn looks_like_windows_abs(path: &str) -> bool {
    let chars = path.chars().collect::<Vec<_>>();
    chars.len() >= 2 && chars[1] == ':'
}

fn transform_component(component: &str) -> String {
    if let Some(stripped) = component.strip_prefix('.') {
        format!("_._{}", stripped)
    } else {
        component.to_string()
    }
}

fn reverse_component(component: &str) -> String {
    if let Some(stripped) = component.strip_prefix("_._") {
        format!(".{}", stripped)
    } else {
        component.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;
    use crate::uri::Scope;

    #[test]
    fn ovpack_roundtrip_preserves_dotfiles() {
        let temp = tempdir().expect("tempdir");
        let fsys = LocalContextFs::new(temp.path());
        fsys.initialize().expect("init failed");

        let src = AxiomUri::root(Scope::Resources).join("demo").expect("join");
        fsys.create_dir_all(&src, true).expect("mkdir");
        fs::write(fsys.resolve_uri(&src).join(".abstract.md"), "hello").expect("write");
        fs::write(fsys.resolve_uri(&src).join("note.txt"), "world").expect("write");

        let pack_path = export_ovpack(&fsys, &src, &temp.path().join("demo")).expect("export");
        let imported = import_ovpack(&fsys, &pack_path, &AxiomUri::root(Scope::Resources), true)
            .expect("import");

        let imported_path = fsys.resolve_uri(&imported);
        assert!(imported_path.join(".abstract.md").exists());
        assert_eq!(
            fs::read_to_string(imported_path.join("note.txt")).expect("read"),
            "world"
        );
    }

    #[test]
    fn ovpack_rejects_zip_slip() {
        let temp = tempdir().expect("tempdir");
        let fsys = LocalContextFs::new(temp.path());
        fsys.initialize().expect("init failed");

        let attack = temp.path().join("attack.ovpack");
        let file = fs::File::create(&attack).expect("create");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer
            .start_file("root/../../pwned.txt", options)
            .expect("start file");
        writer.write_all(b"x").expect("write file");
        writer.finish().expect("finish");

        let err = import_ovpack(&fsys, &attack, &AxiomUri::root(Scope::Resources), false)
            .expect_err("must fail");

        assert!(matches!(err, AxiomError::SecurityViolation(_)));
    }

    #[test]
    fn ovpack_rejects_mixed_archive_roots() {
        let temp = tempdir().expect("tempdir");
        let fsys = LocalContextFs::new(temp.path());
        fsys.initialize().expect("init failed");

        let attack = temp.path().join("mixed-roots.ovpack");
        let file = fs::File::create(&attack).expect("create");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer
            .start_file("root/a.txt", options)
            .expect("start file root");
        writer.write_all(b"a").expect("write root file");
        writer
            .start_file("other/b.txt", options)
            .expect("start file other");
        writer.write_all(b"b").expect("write other file");
        writer.finish().expect("finish");

        let err = import_ovpack(&fsys, &attack, &AxiomUri::root(Scope::Resources), false)
            .expect_err("must fail");
        assert!(matches!(err, AxiomError::InvalidArchive(_)));
    }

    #[test]
    fn ovpack_rejects_backslash_path_entries() {
        let temp = tempdir().expect("tempdir");
        let fsys = LocalContextFs::new(temp.path());
        fsys.initialize().expect("init failed");

        let attack = temp.path().join("backslash.ovpack");
        let file = fs::File::create(&attack).expect("create");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer
            .start_file("root\\pwned.txt", options)
            .expect("start file");
        writer.write_all(b"x").expect("write file");
        writer.finish().expect("finish");

        let err = import_ovpack(&fsys, &attack, &AxiomUri::root(Scope::Resources), false)
            .expect_err("must fail");
        assert!(
            matches!(
                err,
                AxiomError::SecurityViolation(_)
                    | AxiomError::InvalidArchive(_)
                    | AxiomError::InvalidUri(_)
            ),
            "unexpected error: {err:?}"
        );
    }
}
