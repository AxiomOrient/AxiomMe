use std::{fs, path::PathBuf};

use axiomme_core::error::AxiomError;
use axiomme_core::models::QueueStatus;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

const FIXED_TRACE_ID: &str = "00000000-0000-0000-0000-000000000000";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoreContractFixture {
    queue_status: QueueStatus,
    error_payload_invalid_uri: Value,
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("core_contract_fixture.json")
}

fn load_fixture() -> CoreContractFixture {
    let raw = fs::read_to_string(fixture_path()).expect("read core contract fixture");
    serde_json::from_str(&raw).expect("parse core contract fixture")
}

fn load_fixture_raw_value() -> Value {
    let raw = fs::read_to_string(fixture_path()).expect("read core contract fixture raw");
    serde_json::from_str(&raw).expect("parse core contract fixture raw")
}

fn fixture_section(raw: &Value, key: &str) -> Value {
    raw.get(key)
        .cloned()
        .unwrap_or_else(|| panic!("missing fixture section: {key}"))
}

#[test]
fn queue_status_fixture_roundtrip_matches_contract_shape() {
    let fixture = load_fixture();
    let raw = load_fixture_raw_value();

    let serialized = serde_json::to_value(&fixture.queue_status).expect("serialize queue status");
    assert_eq!(serialized, fixture_section(&raw, "queue_status"));
}

#[test]
fn queue_status_fixture_rejects_invalid_field_types() {
    let raw = load_fixture_raw_value();
    let mut queue = fixture_section(&raw, "queue_status");
    queue["semantic"]["new_total"] = Value::String("2".to_string());
    assert!(
        serde_json::from_value::<QueueStatus>(queue).is_err(),
        "numeric fields must reject string payloads"
    );
}

#[test]
fn error_payload_fixture_matches_invalid_uri_contract() {
    let fixture = load_fixture();

    let payload = AxiomError::InvalidUri("axiom://invalid".to_string())
        .to_payload("read", Some("axiom://invalid".to_string()));
    let mut serialized = serde_json::to_value(payload).expect("serialize error payload");

    let trace_id = serialized
        .get("trace_id")
        .and_then(Value::as_str)
        .expect("trace_id string");
    Uuid::parse_str(trace_id).expect("trace_id must be a UUID");
    serialized["trace_id"] = Value::String(FIXED_TRACE_ID.to_string());

    assert!(
        serialized.get("details").is_none(),
        "details must be omitted when empty"
    );
    assert_eq!(serialized, fixture.error_payload_invalid_uri);
}
