use super::{
    EPISODIC_ALLOWED_MANIFEST_OPERATORS, EPISODIC_LOCK_SOURCE_PREFIX, EPISODIC_REQUIRED_GIT_REV,
    EPISODIC_REQUIRED_GIT_URL, EPISODIC_REQUIRED_MAJOR, EPISODIC_REQUIRED_MINOR,
    ONTOLOGY_CONTRACT_PROBE_TEST_NAME,
};
use crate::models::{EpisodicSemverPolicy, OntologyContractPolicy};

pub(super) fn episodic_semver_policy() -> EpisodicSemverPolicy {
    EpisodicSemverPolicy {
        required_major: EPISODIC_REQUIRED_MAJOR,
        required_minor: EPISODIC_REQUIRED_MINOR,
        required_lock_source_prefix: EPISODIC_LOCK_SOURCE_PREFIX.to_string(),
        allowed_manifest_operators: EPISODIC_ALLOWED_MANIFEST_OPERATORS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        required_git_url: EPISODIC_REQUIRED_GIT_URL.to_string(),
        required_git_rev: EPISODIC_REQUIRED_GIT_REV.to_string(),
    }
}

pub(super) fn ontology_contract_policy() -> OntologyContractPolicy {
    OntologyContractPolicy {
        schema_uri: crate::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string(),
        required_schema_version: 1,
        probe_test_name: ONTOLOGY_CONTRACT_PROBE_TEST_NAME.to_string(),
    }
}
