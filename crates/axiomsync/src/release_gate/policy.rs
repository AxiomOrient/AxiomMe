use super::{
    EPISODIC_ALLOWED_MANIFEST_OPERATORS, EPISODIC_REQUIRED_LOCK_SOURCE, EPISODIC_REQUIRED_MAJOR,
    EPISODIC_REQUIRED_MANIFEST_PATH, EPISODIC_REQUIRED_MINOR, EPISODIC_REQUIRED_WORKSPACE_MEMBER,
    ONTOLOGY_CONTRACT_PROBE_TEST_NAME,
};
use crate::models::{EpisodicSemverPolicy, OntologyContractPolicy};

pub(super) fn episodic_semver_policy() -> EpisodicSemverPolicy {
    EpisodicSemverPolicy {
        required_major: EPISODIC_REQUIRED_MAJOR,
        required_minor: EPISODIC_REQUIRED_MINOR,
        required_manifest_path: EPISODIC_REQUIRED_MANIFEST_PATH.to_string(),
        required_workspace_member: EPISODIC_REQUIRED_WORKSPACE_MEMBER.to_string(),
        allowed_manifest_operators: EPISODIC_ALLOWED_MANIFEST_OPERATORS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        required_lock_source: EPISODIC_REQUIRED_LOCK_SOURCE.to_string(),
    }
}

pub(super) fn ontology_contract_policy() -> OntologyContractPolicy {
    OntologyContractPolicy {
        schema_uri: crate::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string(),
        required_schema_version: 1,
        probe_test_name: ONTOLOGY_CONTRACT_PROBE_TEST_NAME.to_string(),
    }
}
