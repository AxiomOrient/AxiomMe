use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::models::BuildQualityGateDetails;
#[cfg(test)]
use crate::models::{
    BlockerRollupGateDetails, ReleaseGateStatus, ReleaseSecurityAuditMode, SecurityAuditGateDetails,
};
#[cfg(test)]
use crate::models::{DependencyAuditStatus, EvidenceStatus};

#[cfg(test)]
pub(crate) use self::workspace_command::with_workspace_command_mocks;
use crate::error::Result;
use crate::models::{
    BenchmarkGateResult, CommandProbeResult, EpisodicSemverProbeResult, EvalLoopReport,
    OntologyContractPolicy, OntologyContractProbeResult, OntologyInvariantCheckSummary,
    OntologySchemaCardinality, OntologySchemaVersionProbe, OperabilityEvidenceReport,
    ReleaseGateDecision, ReleaseGateDetails, ReleaseGateId, ReleaseGatePackReport,
    ReliabilityEvidenceReport, SecurityAuditReport,
};
use crate::text::truncate_text;
use crate::uri::{AxiomUri, Scope};

mod build_quality;
mod contract_integrity;
mod contract_probe;
mod decision;
mod episodic_semver;
mod policy;
#[cfg(test)]
mod test_support;
mod workspace;
mod workspace_command;

const CONTRACT_EXECUTION_TEST_NAME: &str =
    "client::tests::relation_trace_logs::contract_execution_probe_validates_core_algorithms";
const EPISODIC_API_PROBE_TEST_NAME: &str =
    "client::tests::relation_trace_logs::episodic_api_probe_validates_om_contract";
const ONTOLOGY_CONTRACT_PROBE_TEST_NAME: &str =
    "ontology::validate::tests::ontology_contract_probe_default_schema_is_compilable";
const EPISODIC_DEPENDENCY_NAME: &str = "episodic";
const EPISODIC_REQUIRED_MAJOR: u64 = 0;
const EPISODIC_REQUIRED_MINOR: u64 = 1;
const CRATES_IO_INDEX_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";
const EPISODIC_ALLOWED_MANIFEST_OPERATORS: &[&str] = &["exact", "caret", "tilde"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct EpisodicManifestDependency {
    version_req: String,
    has_path: bool,
    has_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EpisodicLockDependency {
    version: String,
    source: Option<String>,
}

impl CommandProbeResult {
    fn from_test_run(test_name: &str, command_ok: bool, output: String) -> Self {
        let matched = output.contains(test_name) && output.contains("ok");
        Self {
            test_name: test_name.to_string(),
            command_ok,
            matched,
            output_excerpt: truncate_text(&output, 200),
            passed: command_ok && matched,
        }
    }

    fn from_error(test_name: &str, error: String) -> Self {
        Self {
            test_name: test_name.to_string(),
            command_ok: false,
            matched: false,
            output_excerpt: error,
            passed: false,
        }
    }
}

impl EpisodicSemverProbeResult {
    fn from_error(error: String) -> Self {
        Self {
            passed: false,
            error: Some(error),
            manifest_req: None,
            manifest_req_ok: None,
            manifest_uses_path: None,
            manifest_uses_git: None,
            manifest_source_ok: None,
            lock_version: None,
            lock_version_ok: None,
            lock_source: None,
            lock_source_ok: None,
        }
    }
}

impl OntologyContractProbeResult {
    fn from_error(error: String, command_probe: CommandProbeResult, schema_uri: String) -> Self {
        Self {
            passed: false,
            error: Some(error),
            command_probe,
            schema: OntologySchemaVersionProbe {
                schema_uri,
                schema_version: None,
                schema_version_ok: false,
            },
            cardinality: OntologySchemaCardinality {
                object_type_count: 0,
                link_type_count: 0,
                action_type_count: 0,
                invariant_count: 0,
            },
            invariant_checks: OntologyInvariantCheckSummary {
                passed: 0,
                failed: 0,
            },
        }
    }
}

pub fn release_gate_pack_report_uri(pack_id: &str) -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("release")?
        .join("packs")?
        .join(&format!("{pack_id}.json"))
}

pub fn gate_decision(
    gate_id: ReleaseGateId,
    passed: bool,
    details: ReleaseGateDetails,
    evidence_uri: Option<String>,
) -> ReleaseGateDecision {
    decision::gate_decision(gate_id, passed, details, evidence_uri)
}

pub fn reliability_evidence_gate_decision(
    report: &ReliabilityEvidenceReport,
) -> ReleaseGateDecision {
    decision::reliability_evidence_gate_decision(report)
}

pub fn eval_quality_gate_decision(report: &EvalLoopReport) -> ReleaseGateDecision {
    decision::eval_quality_gate_decision(report)
}

pub fn session_memory_gate_decision(
    passed: bool,
    memory_category_miss: usize,
    details: &str,
) -> ReleaseGateDecision {
    decision::session_memory_gate_decision(passed, memory_category_miss, details)
}

pub fn security_audit_gate_decision(report: &SecurityAuditReport) -> ReleaseGateDecision {
    decision::security_audit_gate_decision(report)
}

pub fn benchmark_release_gate_decision(report: &BenchmarkGateResult) -> ReleaseGateDecision {
    decision::benchmark_release_gate_decision(report)
}

pub fn operability_evidence_gate_decision(
    report: &OperabilityEvidenceReport,
) -> ReleaseGateDecision {
    decision::operability_evidence_gate_decision(report)
}

pub fn unresolved_blockers(decisions: &[ReleaseGateDecision]) -> usize {
    decision::unresolved_blockers(decisions)
}

pub fn blocker_rollup_gate_decision(unresolved_blockers: usize) -> ReleaseGateDecision {
    decision::blocker_rollup_gate_decision(unresolved_blockers)
}

pub fn finalize_release_gate_pack_report(
    pack_id: String,
    workspace_dir: String,
    decisions: Vec<ReleaseGateDecision>,
    report_uri: String,
) -> ReleaseGatePackReport {
    decision::finalize_release_gate_pack_report(pack_id, workspace_dir, decisions, report_uri)
}

pub fn resolve_workspace_dir(workspace_dir: Option<&str>) -> Result<PathBuf> {
    workspace::resolve_workspace_dir(workspace_dir)
}

pub fn evaluate_contract_integrity_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    contract_integrity::evaluate_contract_integrity_gate(workspace_dir)
}

pub fn evaluate_build_quality_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    build_quality::evaluate_build_quality_gate(workspace_dir)
}

fn run_workspace_command(workspace_dir: &Path, cmd: &str, args: &[&str]) -> (bool, String) {
    workspace_command::run_workspace_command(workspace_dir, cmd, args)
}

fn run_contract_execution_probe(workspace_dir: &Path) -> CommandProbeResult {
    contract_probe::run_contract_execution_probe(workspace_dir)
}

fn run_episodic_api_probe(workspace_dir: &Path) -> CommandProbeResult {
    contract_probe::run_episodic_api_probe(workspace_dir)
}

fn run_ontology_contract_probe(
    workspace_dir: &Path,
    policy: &OntologyContractPolicy,
) -> OntologyContractProbeResult {
    contract_probe::run_ontology_contract_probe(workspace_dir, policy)
}

fn run_episodic_semver_probe(workspace_dir: &Path) -> EpisodicSemverProbeResult {
    episodic_semver::run_episodic_semver_probe(workspace_dir)
}

#[cfg(test)]
fn parse_manifest_episodic_dependency(
    manifest: &str,
) -> std::result::Result<EpisodicManifestDependency, String> {
    episodic_semver::parse_manifest_episodic_dependency(manifest)
}

#[cfg(test)]
fn episodic_manifest_req_contract_matches(raw: &str) -> bool {
    episodic_semver::episodic_manifest_req_contract_matches(raw)
}

#[cfg(test)]
fn episodic_lock_version_contract_matches(raw: &str) -> bool {
    episodic_semver::episodic_lock_version_contract_matches(raw)
}

#[cfg(test)]
mod tests;
