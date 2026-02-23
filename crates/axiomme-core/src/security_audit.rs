use std::path::{Path, PathBuf};

use crate::error::AxiomError;
use crate::host_tools::{HostCommandResult, HostCommandSpec, run_host_command};
use crate::models::{
    DependencyAuditStatus, DependencyAuditSummary, DependencyInventorySummary,
    ReleaseSecurityAuditMode, SecurityAuditCheck,
};
use crate::text::{OutputTrimMode, first_non_empty_output, truncate_text};

#[derive(Debug, Clone, PartialEq, Eq)]
enum CargoAuditProbe {
    Available { tool_version: Option<String> },
    Missing,
    HostToolsDisabled { reason: String },
}

pub fn resolve_security_audit_mode(
    raw: Option<&str>,
) -> Result<ReleaseSecurityAuditMode, AxiomError> {
    match raw
        .unwrap_or("offline")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "offline" => Ok(ReleaseSecurityAuditMode::Offline),
        "strict" => Ok(ReleaseSecurityAuditMode::Strict),
        other => Err(AxiomError::Validation(format!(
            "invalid security audit mode: {other} (expected offline|strict)"
        ))),
    }
}

fn cargo_audit_args(mode: ReleaseSecurityAuditMode, advisory_db_path: &Path) -> Vec<String> {
    let mut args = vec![
        "audit".to_string(),
        "--json".to_string(),
        "--db".to_string(),
        advisory_db_path.display().to_string(),
    ];
    match mode {
        ReleaseSecurityAuditMode::Offline => {
            args.push("--no-fetch".to_string());
            args.push("--stale".to_string());
        }
        ReleaseSecurityAuditMode::Strict => {}
    }
    args
}

pub fn dependency_inventory_summary(workspace_dir: &Path) -> DependencyInventorySummary {
    let lockfile_present = workspace_dir.join("Cargo.lock").exists();
    let package_count = match run_host_command(
        HostCommandSpec::new(
            "security_audit:inventory",
            "cargo",
            &["metadata", "--format-version", "1"],
        )
        .with_current_dir(workspace_dir),
    ) {
        HostCommandResult::Completed {
            success: true,
            stdout,
            ..
        } => serde_json::from_str::<serde_json::Value>(&stdout)
            .ok()
            .and_then(|value| {
                value
                    .get("packages")
                    .and_then(|v| v.as_array())
                    .map(Vec::len)
            })
            .unwrap_or(0),
        _ => 0,
    };

    DependencyInventorySummary {
        lockfile_present,
        package_count,
    }
}

pub fn dependency_audit_summary(
    workspace_dir: &Path,
    mode: ReleaseSecurityAuditMode,
) -> DependencyAuditSummary {
    let advisory_db_path = resolve_advisory_db_path(workspace_dir);
    let tool_version = match probe_cargo_audit_tool() {
        CargoAuditProbe::HostToolsDisabled { reason } => {
            return DependencyAuditSummary {
                tool: "cargo-audit".to_string(),
                mode,
                available: false,
                executed: false,
                status: DependencyAuditStatus::HostToolsDisabled,
                advisories_found: 0,
                tool_version: None,
                output_excerpt: Some(format_audit_output_excerpt(&advisory_db_path, Some(reason))),
            };
        }
        CargoAuditProbe::Missing => {
            return DependencyAuditSummary {
                tool: "cargo-audit".to_string(),
                mode,
                available: false,
                executed: false,
                status: DependencyAuditStatus::ToolMissing,
                advisories_found: 0,
                tool_version: None,
                output_excerpt: None,
            };
        }
        CargoAuditProbe::Available { tool_version } => tool_version,
    };

    if let Err(reason) = prepare_advisory_db_directory(&advisory_db_path, mode) {
        return DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            mode,
            available: true,
            executed: false,
            status: DependencyAuditStatus::Error,
            advisories_found: 0,
            tool_version,
            output_excerpt: Some(format_audit_output_excerpt(&advisory_db_path, Some(reason))),
        };
    }

    let audit_args = cargo_audit_args(mode, &advisory_db_path);
    let audit_arg_refs = audit_args.iter().map(String::as_str).collect::<Vec<_>>();
    match run_host_command(
        HostCommandSpec::new("security_audit:dependency_audit", "cargo", &audit_arg_refs)
            .with_current_dir(workspace_dir),
    ) {
        HostCommandResult::Completed {
            success,
            stdout,
            stderr,
        } => {
            let advisories = parse_cargo_audit_advisory_count(&stdout)
                .or_else(|| parse_cargo_audit_advisory_count(&stderr))
                .unwrap_or(0);
            let status = if advisories > 0 {
                DependencyAuditStatus::VulnerabilitiesFound
            } else if success {
                DependencyAuditStatus::Passed
            } else {
                DependencyAuditStatus::Error
            };
            let output_excerpt = Some(format_audit_output_excerpt(
                &advisory_db_path,
                first_non_empty_output(&stdout, &stderr, OutputTrimMode::Trim),
            ));

            DependencyAuditSummary {
                tool: "cargo-audit".to_string(),
                mode,
                available: true,
                executed: true,
                status,
                advisories_found: advisories,
                tool_version,
                output_excerpt,
            }
        }
        HostCommandResult::SpawnError { error } => DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            mode,
            available: true,
            executed: true,
            status: DependencyAuditStatus::Error,
            advisories_found: 0,
            tool_version,
            output_excerpt: Some(format_audit_output_excerpt(&advisory_db_path, Some(error))),
        },
        HostCommandResult::Blocked { reason } => DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            mode,
            available: false,
            executed: false,
            status: DependencyAuditStatus::HostToolsDisabled,
            advisories_found: 0,
            tool_version: None,
            output_excerpt: Some(format_audit_output_excerpt(&advisory_db_path, Some(reason))),
        },
    }
}

pub fn build_security_audit_checks(
    inventory: &DependencyInventorySummary,
    dependency_audit: &DependencyAuditSummary,
) -> Vec<SecurityAuditCheck> {
    vec![
        SecurityAuditCheck {
            name: "lockfile_present".to_string(),
            passed: inventory.lockfile_present,
            details: if inventory.lockfile_present {
                "Cargo.lock detected".to_string()
            } else {
                "Cargo.lock missing".to_string()
            },
        },
        SecurityAuditCheck {
            name: "dependency_inventory".to_string(),
            passed: inventory.package_count > 0,
            details: format!("packages={}", inventory.package_count),
        },
        SecurityAuditCheck {
            name: "cargo_audit_tool".to_string(),
            passed: dependency_audit.available,
            details: if dependency_audit.available {
                format!(
                    "cargo-audit available ({})",
                    dependency_audit
                        .tool_version
                        .as_deref()
                        .unwrap_or("unknown-version")
                )
            } else {
                "cargo-audit not installed".to_string()
            },
        },
        SecurityAuditCheck {
            name: "dependency_vulnerabilities".to_string(),
            passed: dependency_audit.available
                && dependency_audit.executed
                && dependency_audit.advisories_found == 0
                && dependency_audit.status == DependencyAuditStatus::Passed,
            details: format!(
                "mode={} status={} advisories_found={}",
                dependency_audit.mode, dependency_audit.status, dependency_audit.advisories_found
            ),
        },
    ]
}

fn probe_cargo_audit_tool() -> CargoAuditProbe {
    match run_host_command(HostCommandSpec::new(
        "security_audit:probe_cargo_audit_tool",
        "cargo",
        &["audit", "-V"],
    )) {
        HostCommandResult::Completed {
            success,
            stdout,
            stderr,
        } => {
            let stdout = stdout.trim().to_string();
            if success {
                let tool_version = if stdout.is_empty() {
                    None
                } else {
                    Some(stdout)
                };
                return CargoAuditProbe::Available { tool_version };
            }
            if stderr.contains("no such command") {
                return CargoAuditProbe::Missing;
            }
            let tool_version = if stdout.is_empty() {
                None
            } else {
                Some(stdout)
            };
            CargoAuditProbe::Available { tool_version }
        }
        HostCommandResult::SpawnError { .. } => CargoAuditProbe::Missing,
        HostCommandResult::Blocked { reason } => CargoAuditProbe::HostToolsDisabled { reason },
    }
}

fn parse_cargo_audit_advisory_count(raw: &str) -> Option<usize> {
    let value = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    let pointers = [
        "/vulnerabilities/counts/total",
        "/vulnerabilities/counts/found",
        "/vulnerabilities/found",
    ];
    for pointer in pointers {
        if let Some(count) = value
            .pointer(pointer)
            .and_then(serde_json::Value::as_u64)
            .map(saturating_u64_to_usize)
        {
            return Some(count);
        }
    }

    if let Some(items) = value
        .pointer("/vulnerabilities/list")
        .and_then(|v| v.as_array())
    {
        return Some(items.len());
    }
    if let Some(items) = value.pointer("/vulnerabilities").and_then(|v| v.as_array()) {
        return Some(items.len());
    }
    Some(0)
}

fn resolve_advisory_db_path(workspace_dir: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("AXIOMME_ADVISORY_DB")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return path;
    }

    workspace_dir.join(".axiomme").join("advisory-db")
}

fn prepare_advisory_db_directory(
    advisory_db_path: &Path,
    mode: ReleaseSecurityAuditMode,
) -> Result<(), String> {
    let Some(parent) = advisory_db_path.parent() else {
        return Err("invalid advisory db path without parent".to_string());
    };
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create advisory db parent: {err}"))?;

    if advisory_db_path.exists() {
        let metadata = std::fs::metadata(advisory_db_path)
            .map_err(|err| format!("failed to read advisory db metadata: {err}"))?;
        if metadata.is_file() {
            match mode {
                ReleaseSecurityAuditMode::Strict => std::fs::remove_file(advisory_db_path)
                    .map_err(|err| format!("failed to reset advisory db file path: {err}"))?,
                ReleaseSecurityAuditMode::Offline => {
                    return Err(
                        "offline mode does not fetch advisory data; run strict once to bootstrap advisory-db"
                            .to_string(),
                    )
                }
            }
        }
    }

    if advisory_db_path.exists() && advisory_db_path.is_dir() {
        let has_entries = std::fs::read_dir(advisory_db_path)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_some();
        let has_git_dir = advisory_db_path.join(".git").is_dir();
        if has_entries && !has_git_dir {
            match mode {
                ReleaseSecurityAuditMode::Strict => {
                    std::fs::remove_dir_all(advisory_db_path).map_err(
                    |err| format!("failed to reset invalid advisory db directory: {err}"),
                )?
                }
                ReleaseSecurityAuditMode::Offline => {
                    return Err(
                        "offline mode requires a bootstrapped advisory-db metadata directory; run strict once to initialize advisory-db"
                            .to_string(),
                    )
                }
            }
        }
    }

    if matches!(mode, ReleaseSecurityAuditMode::Offline) && !advisory_db_path.join(".git").is_dir()
    {
        return Err(
            "offline mode requires a bootstrapped advisory-db metadata directory; run strict once to initialize advisory-db"
                .to_string(),
        );
    }

    Ok(())
}

fn format_audit_output_excerpt(advisory_db_path: &Path, output: Option<String>) -> String {
    let context = format!("advisory_db={}", advisory_db_path.display());
    match output {
        Some(text) if !text.trim().is_empty() => {
            truncate_text(&format!("{context}; {}", text.trim()), 1200)
        }
        _ => context,
    }
}

fn saturating_u64_to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cargo_audit_advisory_count_supports_counts_total_shape() {
        let payload = r#"{"vulnerabilities":{"counts":{"total":3}}}"#;
        assert_eq!(parse_cargo_audit_advisory_count(payload), Some(3));
    }

    #[test]
    fn parse_cargo_audit_advisory_count_supports_list_shape() {
        let payload = r#"{"vulnerabilities":{"list":[{"id":"A"},{"id":"B"}]}}"#;
        assert_eq!(parse_cargo_audit_advisory_count(payload), Some(2));
    }

    #[test]
    fn parse_cargo_audit_advisory_count_defaults_to_zero_for_known_json_without_matches() {
        let payload = r#"{"vulnerabilities":{"counts":{"unknown":1}}}"#;
        assert_eq!(parse_cargo_audit_advisory_count(payload), Some(0));
    }

    #[test]
    fn build_security_audit_checks_flags_fail_when_audit_missing() {
        let inventory = DependencyInventorySummary {
            lockfile_present: true,
            package_count: 12,
        };
        let audit = DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            mode: ReleaseSecurityAuditMode::Offline,
            available: false,
            executed: false,
            status: DependencyAuditStatus::ToolMissing,
            advisories_found: 0,
            tool_version: None,
            output_excerpt: None,
        };
        let checks = build_security_audit_checks(&inventory, &audit);
        assert!(checks.iter().any(|check| !check.passed));
        assert_eq!(checks.len(), 4);
    }

    #[test]
    fn dependency_audit_summary_roundtrips_mode_and_status_contract_values() {
        let summary = DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            mode: ReleaseSecurityAuditMode::Strict,
            available: true,
            executed: true,
            status: DependencyAuditStatus::VulnerabilitiesFound,
            advisories_found: 2,
            tool_version: Some("cargo-audit 1.0.0".to_string()),
            output_excerpt: Some("advisory_db=/tmp/db; found advisories".to_string()),
        };
        let json = serde_json::to_value(&summary).expect("serialize dependency audit summary");
        assert_eq!(json["mode"], "strict");
        assert_eq!(json["status"], "vulnerabilities_found");

        let roundtrip: DependencyAuditSummary =
            serde_json::from_value(json).expect("deserialize dependency audit summary");
        assert_eq!(roundtrip.mode, ReleaseSecurityAuditMode::Strict);
        assert_eq!(
            roundtrip.status,
            DependencyAuditStatus::VulnerabilitiesFound
        );
    }

    #[test]
    fn resolve_security_audit_mode_supports_offline_and_strict() {
        assert_eq!(
            resolve_security_audit_mode(Some("offline")).expect("offline"),
            ReleaseSecurityAuditMode::Offline
        );
        assert_eq!(
            resolve_security_audit_mode(Some("strict")).expect("strict"),
            ReleaseSecurityAuditMode::Strict
        );
    }

    #[test]
    fn resolve_security_audit_mode_rejects_unknown_value() {
        let err = resolve_security_audit_mode(Some("fast")).expect_err("must reject");
        assert!(err.to_string().contains("invalid security audit mode"));
    }

    #[test]
    fn cargo_audit_args_include_db_and_mode_flags() {
        let db = Path::new("/tmp/advisory-db");
        let strict = cargo_audit_args(ReleaseSecurityAuditMode::Strict, db);
        assert_eq!(
            strict,
            vec![
                "audit".to_string(),
                "--json".to_string(),
                "--db".to_string(),
                "/tmp/advisory-db".to_string()
            ]
        );

        let offline = cargo_audit_args(ReleaseSecurityAuditMode::Offline, db);
        assert_eq!(
            offline,
            vec![
                "audit".to_string(),
                "--json".to_string(),
                "--db".to_string(),
                "/tmp/advisory-db".to_string(),
                "--no-fetch".to_string(),
                "--stale".to_string()
            ]
        );
    }

    #[test]
    fn resolve_advisory_db_path_defaults_to_workspace_scoped_advisory_db() {
        let workspace = Path::new("/tmp/axiomme-workspace");
        let path = resolve_advisory_db_path(workspace);
        assert_eq!(path, workspace.join(".axiomme").join("advisory-db"));
    }

    #[test]
    fn format_audit_output_excerpt_includes_db_context() {
        let db = Path::new("/tmp/advisory-db");
        let excerpt = format_audit_output_excerpt(db, Some("failure".to_string()));
        assert!(excerpt.contains("advisory_db=/tmp/advisory-db"));
        assert!(excerpt.contains("failure"));
    }

    #[test]
    fn prepare_advisory_db_directory_offline_requires_bootstrapped_advisory_db() {
        let temp = tempfile::tempdir().expect("tempdir");
        let advisory_db = temp.path().join("advisory-db");
        let err = prepare_advisory_db_directory(&advisory_db, ReleaseSecurityAuditMode::Offline)
            .expect_err("offline must fail without bootstrapped advisory db");
        assert!(err.contains("offline mode requires a bootstrapped advisory-db"));
    }

    #[test]
    fn prepare_advisory_db_directory_strict_resets_non_git_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let advisory_db = temp.path().join("advisory-db");
        std::fs::create_dir_all(&advisory_db).expect("create advisory dir");
        std::fs::write(advisory_db.join("junk.txt"), "junk").expect("write junk");
        prepare_advisory_db_directory(&advisory_db, ReleaseSecurityAuditMode::Strict)
            .expect("strict should reset invalid advisory db dir");
        assert!(!advisory_db.exists());
    }

    #[test]
    fn dependency_audit_summary_strict_attempts_recovery_from_non_git_advisory_db_directory() {
        if !matches!(probe_cargo_audit_tool(), CargoAuditProbe::Available { .. }) {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let advisory_db = temp.path().join(".axiomme").join("advisory-db");
        std::fs::create_dir_all(&advisory_db).expect("create advisory dir");
        std::fs::write(advisory_db.join("junk.txt"), "junk").expect("write junk");

        let summary = dependency_audit_summary(temp.path(), ReleaseSecurityAuditMode::Strict);
        assert!(summary.executed);
        assert!(
            summary
                .output_excerpt
                .as_deref()
                .unwrap_or("")
                .contains("advisory_db=")
        );
        assert!(
            !summary
                .output_excerpt
                .as_deref()
                .unwrap_or("")
                .contains("non-empty but not initialized as a git repository")
        );
    }
}
