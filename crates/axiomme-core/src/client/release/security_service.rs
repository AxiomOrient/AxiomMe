use std::time::Instant;

use chrono::Utc;

use crate::catalog::security_audit_report_uri;
use crate::error::Result;
use crate::evidence::evidence_status;
use crate::models::{
    DependencyAuditSummary, DependencyInventorySummary, EvidenceStatus, SecurityAuditCheck,
    SecurityAuditReport,
};
use crate::release_gate::resolve_workspace_dir;
use crate::security_audit::{
    build_security_audit_checks, dependency_audit_summary, dependency_inventory_summary,
    resolve_security_audit_mode,
};

use super::AxiomMe;

struct SecurityAuditRuntimeState {
    workspace_dir: String,
    inventory: DependencyInventorySummary,
    dependency_audit: DependencyAuditSummary,
    checks: Vec<SecurityAuditCheck>,
    passed: bool,
    status: EvidenceStatus,
}

impl AxiomMe {
    pub fn run_security_audit(&self, workspace_dir: Option<&str>) -> Result<SecurityAuditReport> {
        self.run_security_audit_with_mode(workspace_dir, None)
    }

    pub fn run_security_audit_with_mode(
        &self,
        workspace_dir: Option<&str>,
        mode: Option<&str>,
    ) -> Result<SecurityAuditReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let workspace_input = workspace_dir.unwrap_or(".").to_string();
        let mode_input = mode.unwrap_or("offline").to_string();

        let output = (|| -> Result<SecurityAuditReport> {
            let runtime =
                Self::collect_security_audit_runtime_state(&workspace_input, &mode_input)?;

            let report_id = uuid::Uuid::new_v4().to_string();
            let report_uri = security_audit_report_uri(&report_id)?;
            let report = SecurityAuditReport {
                report_id,
                created_at: Utc::now().to_rfc3339(),
                workspace_dir: runtime.workspace_dir,
                passed: runtime.passed,
                status: runtime.status,
                inventory: runtime.inventory,
                dependency_audit: runtime.dependency_audit,
                checks: runtime.checks,
                report_uri: report_uri.to_string(),
            };
            self.fs
                .write(&report_uri, &serde_json::to_string_pretty(&report)?, true)?;
            Ok(report)
        })();

        match output {
            Ok(report) => {
                self.log_request_status(
                    request_id,
                    "security.audit",
                    report.status.as_str(),
                    started,
                    None,
                    Some(serde_json::json!({
                        "workspace_dir": report.workspace_dir,
                        "passed": report.passed,
                        "report_uri": report.report_uri,
                        "advisories_found": report.dependency_audit.advisories_found,
                        "audit_status": report.dependency_audit.status,
                        "audit_mode": report.dependency_audit.mode,
                    })),
                );
                Ok(report)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "security.audit",
                    started,
                    None,
                    &err,
                    Some(serde_json::json!({
                        "workspace_dir": workspace_input,
                        "audit_mode": mode_input,
                    })),
                );
                Err(err)
            }
        }
    }

    fn collect_security_audit_runtime_state(
        workspace_input: &str,
        mode_input: &str,
    ) -> Result<SecurityAuditRuntimeState> {
        let workspace_path = resolve_workspace_dir(Some(workspace_input))?;
        let workspace_dir = workspace_path.display().to_string();
        let mode = resolve_security_audit_mode(Some(mode_input))?;
        let inventory = dependency_inventory_summary(&workspace_path);
        let dependency_audit = dependency_audit_summary(&workspace_path, mode);
        let checks = build_security_audit_checks(&inventory, &dependency_audit);
        let passed = checks.iter().all(|check| check.passed);
        let status = evidence_status(passed);

        Ok(SecurityAuditRuntimeState {
            workspace_dir,
            inventory,
            dependency_audit,
            checks,
            passed,
            status,
        })
    }
}
