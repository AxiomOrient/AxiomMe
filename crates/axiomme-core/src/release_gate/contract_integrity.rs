use std::path::Path;

use crate::models::{
    ContractIntegrityGateDetails, ReleaseGateDecision, ReleaseGateDetails, ReleaseGateId,
};

pub(super) fn evaluate_contract_integrity_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    let contract_probe = super::run_contract_execution_probe(workspace_dir);
    let episodic_semver_probe = super::run_episodic_semver_probe(workspace_dir);
    let episodic_api_probe = super::run_episodic_api_probe(workspace_dir);
    let ontology_policy = super::policy::ontology_contract_policy();
    let ontology_probe = super::run_ontology_contract_probe(workspace_dir, &ontology_policy);

    let passed = contract_probe.passed
        && episodic_semver_probe.passed
        && episodic_api_probe.passed
        && ontology_probe.passed;
    let details = ReleaseGateDetails::ContractIntegrity(Box::new(ContractIntegrityGateDetails {
        policy: super::policy::episodic_semver_policy(),
        contract_probe,
        episodic_api_probe,
        episodic_semver_probe,
        ontology_policy: Some(ontology_policy),
        ontology_probe: Some(ontology_probe),
    }));
    super::gate_decision(ReleaseGateId::ContractIntegrity, passed, details, None)
}
