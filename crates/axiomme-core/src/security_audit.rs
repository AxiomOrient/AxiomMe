use std::path::Path;
use std::process::Command;

use crate::models::{DependencyAuditSummary, DependencyInventorySummary, SecurityAuditCheck};

pub(crate) fn dependency_inventory_summary(workspace_dir: &Path) -> DependencyInventorySummary {
    let lockfile_present = workspace_dir.join("Cargo.lock").exists();
    let package_count = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(workspace_dir)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| serde_json::from_slice::<serde_json::Value>(&out.stdout).ok())
        .and_then(|value| {
            value
                .get("packages")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
        })
        .unwrap_or(0);

    DependencyInventorySummary {
        lockfile_present,
        package_count,
    }
}

pub(crate) fn dependency_audit_summary(workspace_dir: &Path) -> DependencyAuditSummary {
    let (available, tool_version) = probe_cargo_audit_tool();
    if !available {
        return DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            available: false,
            executed: false,
            status: "tool_missing".to_string(),
            advisories_found: 0,
            tool_version,
            output_excerpt: None,
        };
    }

    match Command::new("cargo")
        .args(["audit", "--json"])
        .current_dir(workspace_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let advisories = parse_cargo_audit_advisory_count(&stdout)
                .or_else(|| parse_cargo_audit_advisory_count(&stderr))
                .unwrap_or(0);
            let status = if advisories > 0 {
                "vulnerabilities_found".to_string()
            } else if output.status.success() {
                "passed".to_string()
            } else {
                "error".to_string()
            };
            let output_excerpt =
                first_non_empty_output(&stdout, &stderr).map(|text| truncate_text(&text, 1200));

            DependencyAuditSummary {
                tool: "cargo-audit".to_string(),
                available: true,
                executed: true,
                status,
                advisories_found: advisories,
                tool_version,
                output_excerpt,
            }
        }
        Err(err) => DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            available: true,
            executed: true,
            status: "error".to_string(),
            advisories_found: 0,
            tool_version,
            output_excerpt: Some(truncate_text(&err.to_string(), 1200)),
        },
    }
}

pub(crate) fn build_security_audit_checks(
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
                && dependency_audit.status == "passed",
            details: format!(
                "status={} advisories_found={}",
                dependency_audit.status, dependency_audit.advisories_found
            ),
        },
    ]
}

fn probe_cargo_audit_tool() -> (bool, Option<String>) {
    let probe = Command::new("cargo").args(["audit", "-V"]).output();
    let Ok(output) = probe else {
        return (false, None);
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        let version = if stdout.is_empty() {
            None
        } else {
            Some(stdout)
        };
        return (true, version);
    }

    if stderr.contains("no such command") {
        return (false, None);
    }

    let version = if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    };
    (true, version)
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
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
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

fn first_non_empty_output(stdout: &str, stderr: &str) -> Option<String> {
    if !stdout.trim().is_empty() {
        Some(stdout.trim().to_string())
    } else if !stderr.trim().is_empty() {
        Some(stderr.trim().to_string())
    } else {
        None
    }
}

fn truncate_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out = text.chars().take(max).collect::<String>();
    out.push_str("...");
    out
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
            available: false,
            executed: false,
            status: "tool_missing".to_string(),
            advisories_found: 0,
            tool_version: None,
            output_excerpt: None,
        };
        let checks = build_security_audit_checks(&inventory, &audit);
        assert!(checks.iter().any(|check| !check.passed));
        assert_eq!(checks.len(), 4);
    }

    #[test]
    fn truncate_text_clips_to_char_boundary() {
        let input = "안녕하세요-hello";
        let clipped = truncate_text(input, 5);
        assert_eq!(clipped, "안녕하세요...");
    }
}
