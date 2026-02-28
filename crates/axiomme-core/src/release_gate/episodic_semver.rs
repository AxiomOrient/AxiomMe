use std::fs;
use std::path::Path;

use semver::{Comparator, Op, Version, VersionReq};

use super::{
    CRATES_IO_INDEX_SOURCE, EPISODIC_DEPENDENCY_NAME, EPISODIC_REQUIRED_MAJOR,
    EPISODIC_REQUIRED_MINOR, EpisodicLockDependency, EpisodicManifestDependency,
};
use crate::models::EpisodicSemverProbeResult;

pub(super) fn run_episodic_semver_probe(workspace_dir: &Path) -> EpisodicSemverProbeResult {
    let core_manifest = workspace_dir
        .join("crates")
        .join("axiomme-core")
        .join("Cargo.toml");
    if !core_manifest.exists() {
        return EpisodicSemverProbeResult::from_error("missing_axiomme_core_crate".to_string());
    }

    let manifest_text = match fs::read_to_string(&core_manifest) {
        Ok(value) => value,
        Err(err) => {
            return EpisodicSemverProbeResult::from_error(format!(
                "manifest_read_error={} path={}",
                err,
                core_manifest.display()
            ));
        }
    };
    let manifest_dep = match parse_manifest_episodic_dependency(&manifest_text) {
        Ok(dep) => dep,
        Err(reason) => return EpisodicSemverProbeResult::from_error(reason),
    };

    let lock_path = workspace_dir.join("Cargo.lock");
    if !lock_path.exists() {
        return EpisodicSemverProbeResult::from_error(format!(
            "missing_workspace_lockfile path={}",
            lock_path.display()
        ));
    }
    let lock_text = match fs::read_to_string(&lock_path) {
        Ok(value) => value,
        Err(err) => {
            return EpisodicSemverProbeResult::from_error(format!(
                "lockfile_read_error={} path={}",
                err,
                lock_path.display()
            ));
        }
    };
    let lock_dep = match parse_lockfile_episodic_dependency(&lock_text) {
        Ok(dep) => dep,
        Err(reason) => return EpisodicSemverProbeResult::from_error(reason),
    };

    let manifest_req_ok = episodic_manifest_req_contract_matches(&manifest_dep.version_req);
    let manifest_source_ok = !manifest_dep.has_path && !manifest_dep.has_git;
    let lock_version_ok = episodic_lock_version_contract_matches(&lock_dep.version);
    let lock_source_ok = lock_dep
        .source
        .as_deref()
        .is_some_and(|source| source.starts_with(CRATES_IO_INDEX_SOURCE));

    let passed = manifest_req_ok && manifest_source_ok && lock_version_ok && lock_source_ok;
    EpisodicSemverProbeResult {
        passed,
        error: None,
        manifest_req: Some(manifest_dep.version_req),
        manifest_req_ok: Some(manifest_req_ok),
        manifest_uses_path: Some(manifest_dep.has_path),
        manifest_uses_git: Some(manifest_dep.has_git),
        manifest_source_ok: Some(manifest_source_ok),
        lock_version: Some(lock_dep.version),
        lock_version_ok: Some(lock_version_ok),
        lock_source: lock_dep.source,
        lock_source_ok: Some(lock_source_ok),
    }
}

pub(super) fn parse_manifest_episodic_dependency(
    manifest: &str,
) -> std::result::Result<EpisodicManifestDependency, String> {
    let manifest_doc: toml::Value =
        toml::from_str(manifest).map_err(|err| format!("manifest_toml_parse_error={err}"))?;
    let dependencies = manifest_doc
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "manifest_missing_dependencies_table".to_string())?;
    let episodic = dependencies
        .get(EPISODIC_DEPENDENCY_NAME)
        .ok_or_else(|| "missing_episodic_dependency".to_string())?;

    match episodic {
        toml::Value::String(version_req) => Ok(EpisodicManifestDependency {
            version_req: version_req.to_string(),
            has_path: false,
            has_git: false,
        }),
        toml::Value::Table(fields) => {
            let version_req = fields
                .get("version")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| "episodic_dependency_missing_version".to_string())?;
            Ok(EpisodicManifestDependency {
                version_req: version_req.to_string(),
                has_path: fields.contains_key("path"),
                has_git: fields.contains_key("git"),
            })
        }
        _ => Err("episodic_dependency_unsupported_shape".to_string()),
    }
}

pub(super) fn parse_lockfile_episodic_dependency(
    lockfile: &str,
) -> std::result::Result<EpisodicLockDependency, String> {
    let lock_doc: toml::Value =
        toml::from_str(lockfile).map_err(|err| format!("lockfile_toml_parse_error={err}"))?;
    let packages = lock_doc
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "lockfile_missing_package_array".to_string())?;

    for package in packages {
        let Some(package_table) = package.as_table() else {
            continue;
        };
        let name = package_table
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        if name != EPISODIC_DEPENDENCY_NAME {
            continue;
        }
        let version = package_table
            .get("version")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| "lockfile_episodic_missing_version".to_string())?
            .to_string();
        let source = package_table
            .get("source")
            .and_then(toml::Value::as_str)
            .map(str::to_string);
        return Ok(EpisodicLockDependency { version, source });
    }

    Err("missing_episodic_lock_entry".to_string())
}

pub(super) fn episodic_manifest_req_contract_matches(raw: &str) -> bool {
    let requirement = match VersionReq::parse(raw.trim()) {
        Ok(value) => value,
        Err(_) => return false,
    };
    if requirement.comparators.len() != 1 {
        return false;
    }
    comparator_matches_episodic_contract(&requirement.comparators[0])
}

fn comparator_matches_episodic_contract(comparator: &Comparator) -> bool {
    if !matches!(comparator.op, Op::Exact | Op::Caret | Op::Tilde) {
        return false;
    }
    comparator.major == EPISODIC_REQUIRED_MAJOR && comparator.minor == Some(EPISODIC_REQUIRED_MINOR)
}

pub(super) fn episodic_lock_version_contract_matches(raw: &str) -> bool {
    Version::parse(raw.trim()).is_ok_and(|version| {
        version.major == EPISODIC_REQUIRED_MAJOR && version.minor == EPISODIC_REQUIRED_MINOR
    })
}
