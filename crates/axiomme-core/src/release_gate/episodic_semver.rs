use std::fs;
use std::path::Path;

use semver::Version;

use super::{
    EPISODIC_DEPENDENCY_NAME, EPISODIC_LOCK_SOURCE_PREFIX, EPISODIC_REQUIRED_GIT_REV,
    EPISODIC_REQUIRED_GIT_URL, EPISODIC_REQUIRED_MAJOR, EPISODIC_REQUIRED_MINOR,
    EpisodicLockDependency, EpisodicManifestDependency,
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

    let manifest_git_ok = manifest_dep
        .git_url
        .as_deref()
        .is_some_and(|git| git.trim() == EPISODIC_REQUIRED_GIT_URL);
    let manifest_rev_ok = manifest_dep
        .rev
        .as_deref()
        .is_some_and(episodic_manifest_req_contract_matches);
    let manifest_req_ok = manifest_rev_ok;
    let manifest_source_ok = manifest_dep.has_git && !manifest_dep.has_path && manifest_git_ok;

    let lock_version_ok = episodic_lock_version_contract_matches(&lock_dep.version);
    let lock_revision_ok = lock_dep
        .revision
        .as_deref()
        .is_some_and(|revision| revision == EPISODIC_REQUIRED_GIT_REV);
    let lock_source_ok = lock_dep.source.as_deref().is_some_and(|source| {
        source.starts_with(EPISODIC_LOCK_SOURCE_PREFIX)
            && source.contains(&format!("#{EPISODIC_REQUIRED_GIT_REV}"))
    });

    let passed = manifest_req_ok
        && manifest_source_ok
        && lock_version_ok
        && lock_source_ok
        && lock_revision_ok;
    EpisodicSemverProbeResult {
        passed,
        error: None,
        manifest_req: manifest_dep.rev.clone(),
        manifest_req_ok: Some(manifest_req_ok),
        manifest_git: manifest_dep.git_url.clone(),
        manifest_git_ok: Some(manifest_git_ok),
        manifest_rev: manifest_dep.rev.clone(),
        manifest_rev_ok: Some(manifest_rev_ok),
        manifest_uses_path: Some(manifest_dep.has_path),
        manifest_uses_git: Some(manifest_dep.has_git),
        manifest_source_ok: Some(manifest_source_ok),
        lock_version: Some(lock_dep.version),
        lock_version_ok: Some(lock_version_ok),
        lock_source: lock_dep.source,
        lock_source_ok: Some(lock_source_ok),
        lock_revision: lock_dep.revision,
        lock_revision_ok: Some(lock_revision_ok),
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
            version_req: Some(version_req.to_string()),
            git_url: None,
            rev: None,
            has_path: false,
            has_git: false,
        }),
        toml::Value::Table(fields) => {
            let git_url = fields
                .get("git")
                .and_then(toml::Value::as_str)
                .map(str::to_string);
            let rev = fields
                .get("rev")
                .and_then(toml::Value::as_str)
                .map(str::to_string);
            let version_req = fields
                .get("version")
                .and_then(toml::Value::as_str)
                .map(str::to_string);

            if git_url.is_some() && rev.is_none() {
                return Err("episodic_dependency_missing_rev".to_string());
            }

            Ok(EpisodicManifestDependency {
                version_req,
                git_url,
                rev,
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
        let revision = source
            .as_deref()
            .and_then(|value| value.split('#').nth(1))
            .map(str::to_string);
        return Ok(EpisodicLockDependency {
            version,
            source,
            revision,
        });
    }

    Err("missing_episodic_lock_entry".to_string())
}

pub(super) fn episodic_manifest_req_contract_matches(raw: &str) -> bool {
    raw.trim() == EPISODIC_REQUIRED_GIT_REV
}

pub(super) fn episodic_lock_version_contract_matches(raw: &str) -> bool {
    Version::parse(raw.trim()).is_ok_and(|version| {
        version.major == EPISODIC_REQUIRED_MAJOR && version.minor == EPISODIC_REQUIRED_MINOR
    })
}
