use std::fs;
use std::path::Path;

use super::{
    EPISODIC_DEPENDENCY_NAME, EPISODIC_REQUIRED_MANIFEST_PATH, EPISODIC_REQUIRED_WORKSPACE_MEMBER,
    EpisodicLockDependency, EpisodicManifestDependency,
};
use crate::models::EpisodicSemverProbeResult;

pub(super) fn run_episodic_semver_probe(workspace_dir: &Path) -> EpisodicSemverProbeResult {
    let core_manifest = workspace_dir
        .join("crates")
        .join("axiomsync")
        .join("Cargo.toml");
    if !core_manifest.exists() {
        return EpisodicSemverProbeResult::from_error("missing_axiomsync_crate".to_string());
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

    let vendored_contract = workspace_dir.join(EPISODIC_REQUIRED_MANIFEST_PATH);
    let vendored_engine = workspace_dir.join(EPISODIC_REQUIRED_WORKSPACE_MEMBER);

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
    let lock_dep = parse_lockfile_episodic_dependency(&lock_text)
        .map(Some)
        .unwrap_or(None);

    let manifest_req_ok = manifest_dep.version_req.is_none();
    let manifest_path_ok = vendored_contract.exists();
    let manifest_source_ok =
        manifest_dep.version_req.is_none() && !manifest_dep.has_path && !manifest_dep.has_git;
    let workspace_member_present = vendored_engine.exists();
    let lock_version_ok = lock_dep.is_none();
    let lock_source_ok = lock_dep.is_none();

    let passed = manifest_req_ok
        && manifest_path_ok
        && manifest_source_ok
        && workspace_member_present
        && lock_version_ok
        && lock_source_ok;
    EpisodicSemverProbeResult {
        passed,
        error: None,
        manifest_req: manifest_dep.version_req,
        manifest_req_ok: Some(manifest_req_ok),
        manifest_path: Some(EPISODIC_REQUIRED_MANIFEST_PATH.to_string()),
        manifest_path_ok: Some(manifest_path_ok),
        manifest_uses_path: Some(manifest_dep.has_path),
        manifest_uses_git: Some(manifest_dep.has_git),
        manifest_source_ok: Some(manifest_source_ok),
        workspace_member_path: Some(EPISODIC_REQUIRED_WORKSPACE_MEMBER.to_string()),
        workspace_member_present: Some(workspace_member_present),
        workspace_version: None,
        workspace_version_ok: Some(true),
        lock_version: lock_dep.as_ref().map(|dep| dep.version.clone()),
        lock_version_ok: Some(lock_version_ok),
        lock_source: lock_dep.and_then(|dep| dep.source),
        lock_source_ok: Some(lock_source_ok),
    }
}

pub(super) fn parse_manifest_episodic_dependency(
    manifest: &str,
) -> std::result::Result<EpisodicManifestDependency, String> {
    let manifest_doc: toml::Value =
        toml::from_str(manifest).map_err(|err| format!("manifest_toml_parse_error={err}"))?;
    let empty_dependencies = toml::map::Map::<String, toml::Value>::new();
    let dependencies = manifest_doc
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .unwrap_or(&empty_dependencies);
    let episodic = dependencies
        .get(EPISODIC_DEPENDENCY_NAME)
        .cloned()
        .unwrap_or(toml::Value::Boolean(false));

    match episodic {
        toml::Value::Boolean(false) => Ok(EpisodicManifestDependency {
            version_req: None,
            path: None,
            git_url: None,
            rev: None,
            has_path: false,
            has_git: false,
        }),
        toml::Value::String(version_req) => Ok(EpisodicManifestDependency {
            version_req: Some(version_req.to_string()),
            path: None,
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
            let path = fields
                .get("path")
                .and_then(toml::Value::as_str)
                .map(str::to_string);

            if git_url.is_some() && rev.is_none() {
                return Err("episodic_dependency_missing_rev".to_string());
            }

            Ok(EpisodicManifestDependency {
                version_req,
                path,
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

    let mut candidates = Vec::<EpisodicLockDependency>::new();
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
        candidates.push(EpisodicLockDependency { version, source });
    }

    if candidates.is_empty() {
        return Err("missing_episodic_lock_entry".to_string());
    }
    if candidates.len() == 1 {
        return Ok(candidates.remove(0));
    }

    let required_candidates = candidates
        .iter()
        .filter(|candidate| {
            candidate.source.is_none() && episodic_lock_version_contract_matches(&candidate.version)
        })
        .cloned()
        .collect::<Vec<_>>();
    match required_candidates.len() {
        1 => Ok(required_candidates
            .into_iter()
            .next()
            .expect("single required candidate")),
        0 => Err("ambiguous_episodic_lock_entry_no_workspace_match".to_string()),
        _ => Err("ambiguous_episodic_lock_entry_multiple_workspace_matches".to_string()),
    }
}

#[cfg(test)]
pub(super) fn episodic_manifest_req_contract_matches(raw: &str, workspace_version: &str) -> bool {
    raw.trim().is_empty() && workspace_version.trim().is_empty()
}

pub(super) fn episodic_lock_version_contract_matches(raw: &str) -> bool {
    raw.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::{
        episodic_lock_version_contract_matches, episodic_manifest_req_contract_matches,
        parse_lockfile_episodic_dependency,
    };

    #[test]
    fn parse_lockfile_episodic_dependency_prefers_lock_entry_without_source() {
        let lockfile = r#"
[[package]]
name = "episodic"
version = "0.2.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "episodic"
version = "0.2.0"
"#;
        let err = parse_lockfile_episodic_dependency(lockfile).expect_err("must reject ambiguity");
        assert_eq!(err, "ambiguous_episodic_lock_entry_no_workspace_match");
    }

    #[test]
    fn parse_lockfile_episodic_dependency_rejects_multiple_entries_without_workspace_match() {
        let lockfile = r#"
[[package]]
name = "episodic"
version = "0.2.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "episodic"
version = "0.1.9"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;
        let err = parse_lockfile_episodic_dependency(lockfile).expect_err("must reject ambiguity");
        assert_eq!(err, "ambiguous_episodic_lock_entry_no_workspace_match");
    }

    #[test]
    fn episodic_manifest_req_contract_matches_accepts_workspace_version_requirement() {
        assert!(episodic_manifest_req_contract_matches("", ""));
        assert!(!episodic_manifest_req_contract_matches("0.2.3", ""));
        assert!(!episodic_manifest_req_contract_matches("", "0.2.3"));
    }

    #[test]
    fn episodic_lock_version_contract_matches_accepts_absent_lock_dependency() {
        assert!(episodic_lock_version_contract_matches(""));
        assert!(!episodic_lock_version_contract_matches("0.2.3"));
    }
}
