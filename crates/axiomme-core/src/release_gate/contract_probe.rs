use std::fs;
use std::path::Path;

use super::{
    CONTRACT_EXECUTION_TEST_NAME, EPISODIC_API_PROBE_TEST_NAME, ONTOLOGY_CONTRACT_PROBE_TEST_NAME,
    OntologyContractPolicy, run_workspace_command,
};
use crate::error::Result;
use crate::models::{
    CommandProbeResult, OntologyContractProbeResult, OntologyInvariantCheckSummary,
    OntologySchemaCardinality, OntologySchemaVersionProbe,
};

pub(super) fn run_contract_execution_probe(workspace_dir: &Path) -> CommandProbeResult {
    let core_crate = workspace_dir
        .join("crates")
        .join("axiomme-core")
        .join("Cargo.toml");
    if !core_crate.exists() {
        return CommandProbeResult::from_error(
            CONTRACT_EXECUTION_TEST_NAME,
            "missing_axiomme_core_crate".to_string(),
        );
    }

    let (ok, output) = run_workspace_command(
        workspace_dir,
        "cargo",
        &[
            "test",
            "-p",
            "axiomme-core",
            CONTRACT_EXECUTION_TEST_NAME,
            "--",
            "--exact",
        ],
    );
    CommandProbeResult::from_test_run(CONTRACT_EXECUTION_TEST_NAME, ok, output)
}

pub(super) fn run_episodic_api_probe(workspace_dir: &Path) -> CommandProbeResult {
    let (ok, output) = run_workspace_command(
        workspace_dir,
        "cargo",
        &[
            "test",
            "-p",
            "axiomme-core",
            EPISODIC_API_PROBE_TEST_NAME,
            "--",
            "--exact",
        ],
    );
    CommandProbeResult::from_test_run(EPISODIC_API_PROBE_TEST_NAME, ok, output)
}

pub(super) fn run_ontology_contract_probe(
    workspace_dir: &Path,
    policy: &OntologyContractPolicy,
) -> OntologyContractProbeResult {
    let schema_uri = policy.schema_uri.clone();
    let probe = run_workspace_command(
        workspace_dir,
        "cargo",
        &[
            "test",
            "-p",
            "axiomme-core",
            ONTOLOGY_CONTRACT_PROBE_TEST_NAME,
            "--",
            "--exact",
        ],
    );
    let command_probe =
        CommandProbeResult::from_test_run(ONTOLOGY_CONTRACT_PROBE_TEST_NAME, probe.0, probe.1);

    let parsed = match load_bootstrapped_ontology_schema(policy.schema_uri.as_str()) {
        Ok(value) => value,
        Err(error) => {
            return OntologyContractProbeResult::from_error(error, command_probe, schema_uri);
        }
    };
    let schema_version = parsed.version;
    let schema_version_ok = schema_version == policy.required_schema_version;
    if !schema_version_ok {
        return OntologyContractProbeResult::from_error(
            format!(
                "ontology_schema_version_mismatch: expected={} got={}",
                policy.required_schema_version, schema_version
            ),
            command_probe,
            schema_uri,
        );
    }

    let object_type_count = parsed.object_types.len();
    let link_type_count = parsed.link_types.len();
    let action_type_count = parsed.action_types.len();
    let invariant_count = parsed.invariants.len();

    let compiled = match crate::ontology::compile_schema(parsed) {
        Ok(value) => value,
        Err(err) => {
            return OntologyContractProbeResult::from_error(
                format!("ontology_schema_compile_failed: {err}"),
                command_probe,
                schema_uri,
            );
        }
    };
    let invariant_report = crate::ontology::evaluate_invariants(&compiled);
    let invariants_ok = invariant_report.failed == 0;
    let error = if invariants_ok {
        None
    } else {
        Some(format!(
            "ontology_invariant_check_failed: failed={} passed={}",
            invariant_report.failed, invariant_report.passed
        ))
    };

    let passed = command_probe.passed && schema_version_ok && invariants_ok;
    OntologyContractProbeResult {
        passed,
        error,
        command_probe,
        schema: OntologySchemaVersionProbe {
            schema_uri,
            schema_version: Some(schema_version),
            schema_version_ok,
        },
        cardinality: OntologySchemaCardinality {
            object_type_count,
            link_type_count,
            action_type_count,
            invariant_count,
        },
        invariant_checks: OntologyInvariantCheckSummary {
            passed: invariant_report.passed,
            failed: invariant_report.failed,
        },
    }
}

fn load_bootstrapped_ontology_schema(
    schema_uri: &str,
) -> std::result::Result<crate::ontology::OntologySchemaV1, String> {
    let probe_root = std::env::temp_dir().join(format!(
        "axiomme-ontology-contract-probe-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let app = crate::AxiomMe::new(&probe_root)
        .map_err(|err| format!("ontology_probe_app_new_failed: {err}"))?;
    let loaded = (|| -> Result<crate::ontology::OntologySchemaV1> {
        app.bootstrap()?;
        let raw = app.read(schema_uri)?;
        crate::ontology::parse_schema_v1(&raw)
    })();
    let _ = fs::remove_dir_all(&probe_root);
    loaded.map_err(|err| format!("ontology_probe_schema_load_failed: {err}"))
}
