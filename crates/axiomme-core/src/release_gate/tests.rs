use std::fs;

use tempfile::tempdir;

use super::test_support::{
    benchmark_gate_result, eval_report, write_contract_gate_workspace_fixture,
};
use super::*;
use crate::models::{DependencyAuditSummary, DependencyInventorySummary, EvalBucket};

#[test]
fn eval_quality_gate_decision_respects_threshold_and_case_count() {
    let no_cases = eval_quality_gate_decision(&eval_report(0, 1.0));
    assert!(!no_cases.passed);

    let low_accuracy = eval_quality_gate_decision(&eval_report(10, 0.5));
    assert!(!low_accuracy.passed);

    let passing = eval_quality_gate_decision(&eval_report(10, 0.9));
    assert!(passing.passed);
}

#[test]
fn eval_quality_gate_decision_fails_when_filter_or_relation_buckets_exist() {
    let mut report = eval_report(10, 0.9);
    report.quality.buckets = vec![
        EvalBucket {
            name: "filter_ignored".to_string(),
            count: 1,
        },
        EvalBucket {
            name: "relation_missing".to_string(),
            count: 0,
        },
    ];
    let decision = eval_quality_gate_decision(&report);
    assert!(!decision.passed);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::EvalQuality(value) => {
            assert_eq!(value.filter_ignored, 1);
        }
        other => panic!("expected eval_quality details, got {other:?}"),
    }
}

#[test]
fn session_memory_gate_decision_fails_when_category_missing() {
    let decision = session_memory_gate_decision(true, 2, "probe");
    assert!(!decision.passed);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::SessionMemory(value) => {
            assert_eq!(value.memory_category_miss, 2);
        }
        other => panic!("expected session_memory details, got {other:?}"),
    }
}

#[test]
fn benchmark_gate_prefers_release_check_evidence_uri() {
    let decision = benchmark_release_gate_decision(&benchmark_gate_result(
        Some("axiom://queue/release/checks/1.json"),
        Some("axiom://queue/release/gates/1.json"),
    ));
    assert_eq!(
        decision.evidence_uri.as_deref(),
        Some("axiom://queue/release/checks/1.json")
    );
}

#[test]
fn finalize_release_gate_pack_adds_g8_and_counts_blockers() {
    let decisions = vec![
        gate_decision(
            ReleaseGateId::ContractIntegrity,
            true,
            ReleaseGateDetails::BlockerRollup(BlockerRollupGateDetails {
                unresolved_blockers: 0,
            }),
            None,
        ),
        gate_decision(
            ReleaseGateId::BuildQuality,
            false,
            ReleaseGateDetails::BuildQuality(BuildQualityGateDetails {
                cargo_check: false,
                cargo_fmt: false,
                cargo_clippy: false,
                check_output: String::new(),
                fmt_output: String::new(),
                clippy_output: String::new(),
            }),
            None,
        ),
    ];
    let report = finalize_release_gate_pack_report(
        "pack-1".to_string(),
        "/tmp/ws".to_string(),
        decisions,
        "axiom://queue/release/packs/pack-1.json".to_string(),
    );
    assert!(!report.passed);
    assert_eq!(report.status, ReleaseGateStatus::Fail);
    assert_eq!(report.unresolved_blockers, 1);
    let g8 = report.decisions.last().expect("g8");
    assert_eq!(g8.gate_id, ReleaseGateId::BlockerRollup);
    assert!(!g8.passed);
    let details = parse_gate_details(g8);
    match details {
        ReleaseGateDetails::BlockerRollup(value) => {
            assert_eq!(value.unresolved_blockers, 1);
        }
        other => panic!("expected blocker_rollup details, got {other:?}"),
    }
}

#[test]
fn security_audit_gate_decision_contains_expected_summary() {
    let report = SecurityAuditReport {
        report_id: "sec-1".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        workspace_dir: "/tmp/ws".to_string(),
        passed: true,
        status: EvidenceStatus::Pass,
        inventory: DependencyInventorySummary {
            lockfile_present: true,
            package_count: 42,
        },
        dependency_audit: DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            mode: ReleaseSecurityAuditMode::Strict,
            available: true,
            executed: true,
            status: DependencyAuditStatus::Passed,
            advisories_found: 0,
            tool_version: Some("cargo-audit 1.0".to_string()),
            output_excerpt: None,
        },
        checks: Vec::new(),
        report_uri: "axiom://queue/release/security/sec-1.json".to_string(),
    };
    let decision = security_audit_gate_decision(&report);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::SecurityAudit(value) => {
            assert_eq!(value.advisories_found, 0);
        }
        other => panic!("expected security_audit details, got {other:?}"),
    }
    assert!(decision.passed);
    assert_eq!(
        decision.evidence_uri.as_deref(),
        Some("axiom://queue/release/security/sec-1.json")
    );
}

#[test]
fn security_audit_gate_decision_fails_when_mode_is_not_strict() {
    let mut report = SecurityAuditReport {
        report_id: "sec-1".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        workspace_dir: "/tmp/ws".to_string(),
        passed: true,
        status: EvidenceStatus::Pass,
        inventory: DependencyInventorySummary {
            lockfile_present: true,
            package_count: 42,
        },
        dependency_audit: DependencyAuditSummary {
            tool: "cargo-audit".to_string(),
            mode: ReleaseSecurityAuditMode::Offline,
            available: true,
            executed: true,
            status: DependencyAuditStatus::Passed,
            advisories_found: 0,
            tool_version: Some("cargo-audit 1.0".to_string()),
            output_excerpt: None,
        },
        checks: Vec::new(),
        report_uri: "axiom://queue/release/security/sec-1.json".to_string(),
    };
    let decision = security_audit_gate_decision(&report);
    assert!(!decision.passed);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::SecurityAudit(value) => {
            assert!(!value.strict_mode);
        }
        other => panic!("expected security_audit details, got {other:?}"),
    }

    report.dependency_audit.mode = ReleaseSecurityAuditMode::Strict;
    let strict_decision = security_audit_gate_decision(&report);
    assert!(strict_decision.passed);
}

#[test]
fn build_quality_gate_reports_failure_for_non_workspace_directory() {
    let temp = tempdir().expect("tempdir");
    let decision = evaluate_build_quality_gate(temp.path());
    assert_eq!(decision.gate_id, ReleaseGateId::BuildQuality);
    assert!(!decision.passed);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::BuildQuality(value) => {
            assert!(!value.cargo_check);
            assert!(!value.cargo_fmt);
            assert!(!value.cargo_clippy);
        }
        other => panic!("expected build_quality details, got {other:?}"),
    }
}

#[test]
fn resolve_workspace_dir_returns_canonical_workspace_path() {
    let temp = tempdir().expect("tempdir");
    fs::write(
        temp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nresolver = \"2\"\n",
    )
    .expect("write workspace manifest");

    let workspace = resolve_workspace_dir(Some(
        temp.path()
            .to_str()
            .expect("temporary workspace path must be valid UTF-8"),
    ))
    .expect("resolve workspace");
    let expected = fs::canonicalize(temp.path()).expect("canonical workspace path");
    assert_eq!(workspace, expected);
}

#[test]
fn resolve_workspace_dir_reports_not_found_for_missing_directory() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("missing-workspace");
    let error = resolve_workspace_dir(Some(
        missing
            .to_str()
            .expect("temporary workspace path must be valid UTF-8"),
    ))
    .expect_err("missing workspace must fail");
    assert!(error.to_string().contains("workspace directory not found"));
}

#[test]
fn resolve_workspace_dir_reports_validation_error_without_manifest() {
    let temp = tempdir().expect("tempdir");
    let error = resolve_workspace_dir(Some(
        temp.path()
            .to_str()
            .expect("temporary workspace path must be valid UTF-8"),
    ))
    .expect_err("workspace without manifest must fail");
    assert!(error.to_string().contains("workspace missing Cargo.toml"));
}

fn parse_gate_details(decision: &ReleaseGateDecision) -> &ReleaseGateDetails {
    &decision.details
}

#[test]
fn release_gate_decision_serializes_details_with_explicit_kind_and_data() {
    let decision = gate_decision(
        ReleaseGateId::BlockerRollup,
        true,
        ReleaseGateDetails::BlockerRollup(BlockerRollupGateDetails {
            unresolved_blockers: 0,
        }),
        None,
    );
    let json = serde_json::to_value(&decision).expect("serialize decision");
    assert_eq!(json["gate_id"], "G8");
    assert_eq!(json["status"], "pass");
    assert_eq!(json["details"]["kind"], "blocker_rollup");
    assert_eq!(json["details"]["data"]["unresolved_blockers"], 0);
}

#[test]
fn security_audit_gate_details_serializes_enum_fields_as_contract_strings() {
    let details = SecurityAuditGateDetails {
        status: EvidenceStatus::Pass,
        mode: ReleaseSecurityAuditMode::Strict,
        strict_mode_required: true,
        strict_mode: true,
        audit_status: DependencyAuditStatus::HostToolsDisabled,
        advisories_found: 0,
        packages: 42,
    };
    let json = serde_json::to_value(&details).expect("serialize security audit gate details");
    assert_eq!(json["status"], "pass");
    assert_eq!(json["mode"], "strict");
    assert_eq!(json["audit_status"], "host_tools_disabled");
}

#[test]
fn security_audit_gate_details_deserializes_contract_strings_into_typed_fields() {
    let payload = serde_json::json!({
        "status": "fail",
        "mode": "offline",
        "strict_mode_required": true,
        "strict_mode": false,
        "audit_status": "tool_missing",
        "advisories_found": 1,
        "packages": 7
    });
    let details: SecurityAuditGateDetails =
        serde_json::from_value(payload).expect("deserialize security audit gate details");
    assert_eq!(details.status, EvidenceStatus::Fail);
    assert_eq!(details.mode, ReleaseSecurityAuditMode::Offline);
    assert_eq!(details.audit_status, DependencyAuditStatus::ToolMissing);
}

#[test]
fn parse_manifest_episodic_dependency_supports_table_form() {
    let manifest = r#"
[package]
name = "axiomme-core"
version = "0.1.0"

[dependencies.episodic]
version = "0.1.0"
git = "https://example.com/episodic.git"
"#;
    let dependency = parse_manifest_episodic_dependency(manifest).expect("parse manifest");
    assert_eq!(dependency.version_req, "0.1.0");
    assert!(dependency.has_git);
    assert!(!dependency.has_path);
}

#[test]
fn parse_manifest_episodic_dependency_requires_version_field() {
    let manifest = r#"
[package]
name = "axiomme-core"
version = "0.1.0"

[dependencies.episodic]
path = "../../../episodic"
"#;
    let error = parse_manifest_episodic_dependency(manifest).expect_err("missing version");
    assert_eq!(error, "episodic_dependency_missing_version");
}

#[test]
fn episodic_manifest_req_contract_matches_accepts_supported_forms() {
    for value in ["0.1.0", "^0.1.4", "~0.1.9", "=0.1.3", "0.1"] {
        assert!(
            episodic_manifest_req_contract_matches(value),
            "expected supported manifest req: {value}"
        );
    }
}

#[test]
fn episodic_manifest_req_contract_matches_rejects_unsupported_ranges() {
    for value in [
        ">=0.1.0",
        ">0.1.0",
        "0.2.0",
        "^0.2.0",
        "0.1.*",
        ">=0.1.0,<0.2.0",
    ] {
        assert!(
            !episodic_manifest_req_contract_matches(value),
            "expected unsupported manifest req: {value}"
        );
    }
}

#[test]
fn episodic_lock_version_contract_matches_checks_exact_version_shape() {
    assert!(episodic_lock_version_contract_matches("0.1.0"));
    assert!(episodic_lock_version_contract_matches("0.1.99"));
    assert!(!episodic_lock_version_contract_matches("0.2.0"));
    assert!(!episodic_lock_version_contract_matches("invalid"));
}

#[test]
fn contract_integrity_gate_fails_when_core_crate_missing() {
    let temp = tempdir().expect("tempdir");
    let decision = evaluate_contract_integrity_gate(temp.path());
    assert!(!decision.passed);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::ContractIntegrity(value) => {
            assert_eq!(
                value.episodic_semver_probe.error.as_deref(),
                Some("missing_axiomme_core_crate")
            );
        }
        other => panic!("expected contract_integrity details, got {other:?}"),
    }
}

#[test]
fn contract_integrity_gate_passes_when_contract_probe_succeeds() {
    let temp = tempdir().expect("tempdir");
    write_contract_gate_workspace_fixture(
        temp.path(),
        "episodic = \"0.1.0\"",
        Some(CRATES_IO_INDEX_SOURCE),
    );

    let output = format!("running 1 test\ntest {CONTRACT_EXECUTION_TEST_NAME} ... ok\n");
    let episodic_output = format!("running 1 test\ntest {EPISODIC_API_PROBE_TEST_NAME} ... ok\n");
    let ontology_output =
        format!("running 1 test\ntest {ONTOLOGY_CONTRACT_PROBE_TEST_NAME} ... ok\n");
    let decision = with_workspace_command_mocks(
        &[
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    CONTRACT_EXECUTION_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                &output,
            ),
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    EPISODIC_API_PROBE_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                &episodic_output,
            ),
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    ONTOLOGY_CONTRACT_PROBE_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                &ontology_output,
            ),
        ],
        || evaluate_contract_integrity_gate(temp.path()),
    );
    assert!(decision.passed, "{:?}", decision.details);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::ContractIntegrity(value) => {
            assert_eq!(value.contract_probe.test_name, CONTRACT_EXECUTION_TEST_NAME);
            assert_eq!(
                value.episodic_api_probe.test_name,
                EPISODIC_API_PROBE_TEST_NAME
            );
            assert!(value.episodic_semver_probe.passed);
            assert_eq!(value.policy.required_minor, EPISODIC_REQUIRED_MINOR);
            assert!(
                value
                    .ontology_policy
                    .as_ref()
                    .is_some_and(|policy| policy.required_schema_version == 1)
            );
            assert!(
                value
                    .ontology_probe
                    .as_ref()
                    .is_some_and(|probe| probe.passed)
            );
        }
        other => panic!("expected contract_integrity details, got {other:?}"),
    }
}

#[test]
fn contract_integrity_gate_fails_when_contract_probe_output_does_not_match() {
    let temp = tempdir().expect("tempdir");
    write_contract_gate_workspace_fixture(
        temp.path(),
        "episodic = \"0.1.0\"",
        Some(CRATES_IO_INDEX_SOURCE),
    );

    let decision = with_workspace_command_mocks(
        &[
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    CONTRACT_EXECUTION_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                "running 1 test\ntest some_other_test ... ok\n",
            ),
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    EPISODIC_API_PROBE_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                "running 1 test\ntest client::tests::relation_trace_logs::episodic_api_probe_validates_om_contract ... ok\n",
            ),
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    ONTOLOGY_CONTRACT_PROBE_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                "running 1 test\ntest ontology::validate::tests::ontology_contract_probe_default_schema_is_compilable ... ok\n",
            ),
        ],
        || evaluate_contract_integrity_gate(temp.path()),
    );
    assert!(!decision.passed);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::ContractIntegrity(value) => {
            assert!(!value.contract_probe.matched);
        }
        other => panic!("expected contract_integrity details, got {other:?}"),
    }
}

#[test]
fn contract_integrity_gate_fails_when_episodic_dependency_uses_path() {
    let temp = tempdir().expect("tempdir");
    write_contract_gate_workspace_fixture(
        temp.path(),
        "episodic = { version = \"0.1.0\", path = \"../../../episodic\" }",
        None,
    );

    let decision = with_workspace_command_mocks(
        &[
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    CONTRACT_EXECUTION_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                "running 1 test\ntest client::tests::relation_trace_logs::contract_execution_probe_validates_core_algorithms ... ok\n",
            ),
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    EPISODIC_API_PROBE_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                "running 1 test\ntest client::tests::relation_trace_logs::episodic_api_probe_validates_om_contract ... ok\n",
            ),
            (
                "cargo",
                &[
                    "test",
                    "-p",
                    "axiomme-core",
                    ONTOLOGY_CONTRACT_PROBE_TEST_NAME,
                    "--",
                    "--exact",
                ],
                true,
                "running 1 test\ntest ontology::validate::tests::ontology_contract_probe_default_schema_is_compilable ... ok\n",
            ),
        ],
        || evaluate_contract_integrity_gate(temp.path()),
    );
    assert!(!decision.passed);
    let details = parse_gate_details(&decision);
    match details {
        ReleaseGateDetails::ContractIntegrity(value) => {
            assert_eq!(value.episodic_semver_probe.manifest_uses_path, Some(true));
        }
        other => panic!("expected contract_integrity details, got {other:?}"),
    }
}
