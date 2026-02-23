use std::{fs, path::PathBuf};

use axiomme_core::models::{
    DependencyAuditStatus, ReleaseGateDecision, ReleaseGateDetails, ReleaseGateId,
    ReleaseGatePackReport, ReleaseSecurityAuditMode,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseContractFixture {
    release_gate_decision_contract_integrity: ReleaseGateDecision,
    release_gate_decision_security_audit: ReleaseGateDecision,
    release_gate_pack_report: ReleaseGatePackReport,
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("release_contract_fixture.json")
}

fn load_fixture() -> ReleaseContractFixture {
    let raw = fs::read_to_string(fixture_path()).expect("read release contract fixture");
    serde_json::from_str(&raw).expect("parse release contract fixture")
}

fn load_fixture_raw_value() -> Value {
    let raw = fs::read_to_string(fixture_path()).expect("read release contract fixture raw");
    serde_json::from_str(&raw).expect("parse release contract fixture raw")
}

fn fixture_section(raw: &Value, key: &str) -> Value {
    raw.get(key)
        .cloned()
        .unwrap_or_else(|| panic!("missing fixture section: {key}"))
}

#[test]
fn release_contract_fixture_roundtrip_preserves_string_enum_contracts() {
    let fixture = load_fixture();
    let raw = load_fixture_raw_value();

    assert_eq!(
        fixture.release_gate_decision_contract_integrity.gate_id,
        ReleaseGateId::ContractIntegrity
    );
    assert_eq!(
        fixture.release_gate_decision_security_audit.gate_id,
        ReleaseGateId::SecurityAudit
    );

    match &fixture.release_gate_decision_security_audit.details {
        ReleaseGateDetails::SecurityAudit(details) => {
            assert_eq!(details.mode, ReleaseSecurityAuditMode::Strict);
            assert_eq!(details.audit_status, DependencyAuditStatus::Passed);
        }
        other => panic!("expected security_audit details, got {other:?}"),
    }

    let contract_integrity_json =
        serde_json::to_value(&fixture.release_gate_decision_contract_integrity)
            .expect("serialize contract integrity decision");
    assert_eq!(
        contract_integrity_json,
        fixture_section(&raw, "release_gate_decision_contract_integrity")
    );

    let security_json = serde_json::to_value(&fixture.release_gate_decision_security_audit)
        .expect("serialize security decision");
    assert_eq!(
        security_json,
        fixture_section(&raw, "release_gate_decision_security_audit")
    );
    assert_eq!(security_json["gate_id"], "G5");
    assert_eq!(security_json["status"], "pass");
    assert_eq!(security_json["details"]["kind"], "security_audit");
    assert_eq!(security_json["details"]["data"]["status"], "pass");
    assert_eq!(security_json["details"]["data"]["mode"], "strict");
    assert_eq!(security_json["details"]["data"]["audit_status"], "passed");

    let pack_json =
        serde_json::to_value(&fixture.release_gate_pack_report).expect("serialize pack");
    assert_eq!(pack_json, fixture_section(&raw, "release_gate_pack_report"));
    assert_eq!(pack_json["status"], "pass");
    assert_eq!(pack_json["unresolved_blockers"], 0);
}

#[test]
fn release_contract_fixture_rejects_invalid_security_audit_enum_values() {
    let fixture = load_fixture();
    let mut payload =
        serde_json::to_value(&fixture.release_gate_decision_security_audit).expect("serialize");
    payload["details"]["data"]["mode"] = serde_json::Value::String("STRICT".to_string());
    assert!(
        serde_json::from_value::<ReleaseGateDecision>(payload).is_err(),
        "uppercase mode must not deserialize"
    );

    let mut payload =
        serde_json::to_value(&fixture.release_gate_decision_security_audit).expect("serialize");
    payload["details"]["data"]["audit_status"] = serde_json::Value::String("passedd".to_string());
    assert!(
        serde_json::from_value::<ReleaseGateDecision>(payload).is_err(),
        "unknown audit_status must not deserialize"
    );
}

#[test]
fn release_contract_fixture_rejects_invalid_gate_and_status_values() {
    let fixture = load_fixture();

    let mut payload =
        serde_json::to_value(&fixture.release_gate_decision_contract_integrity).expect("serialize");
    payload["gate_id"] = Value::String("G9".to_string());
    assert!(
        serde_json::from_value::<ReleaseGateDecision>(payload).is_err(),
        "unknown gate id must not deserialize"
    );

    let mut payload =
        serde_json::to_value(&fixture.release_gate_decision_contract_integrity).expect("serialize");
    payload["status"] = Value::String("PASS".to_string());
    assert!(
        serde_json::from_value::<ReleaseGateDecision>(payload).is_err(),
        "uppercase gate status must not deserialize"
    );

    let mut payload = serde_json::to_value(&fixture.release_gate_pack_report).expect("serialize");
    payload["status"] = Value::String("FAIL".to_string());
    assert!(
        serde_json::from_value::<ReleaseGatePackReport>(payload).is_err(),
        "uppercase pack status must not deserialize"
    );
}
