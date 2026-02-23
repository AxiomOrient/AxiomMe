use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use semver::{Comparator, Op, Version, VersionReq};

#[cfg(test)]
use crate::models::{DependencyAuditStatus, EvidenceStatus};
#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::collections::HashMap;

use crate::error::{AxiomError, Result};
use crate::host_tools::{HostCommandResult, HostCommandSpec, run_host_command};
use crate::models::{
    BenchmarkGateDetails, BenchmarkGateResult, BlockerRollupGateDetails, BuildQualityGateDetails,
    CommandProbeResult, ContractIntegrityGateDetails, EpisodicSemverPolicy,
    EpisodicSemverProbeResult, EvalLoopReport, EvalQualityGateDetails, OntologyContractPolicy,
    OntologyContractProbeResult, OperabilityEvidenceReport, OperabilityGateDetails,
    ReleaseGateDecision, ReleaseGateDetails, ReleaseGateId, ReleaseGatePackReport,
    ReleaseGateStatus, ReleaseSecurityAuditMode, ReliabilityEvidenceReport, ReliabilityGateDetails,
    SecurityAuditGateDetails, SecurityAuditReport, SessionMemoryGateDetails,
};
use crate::text::{OutputTrimMode, first_non_empty_output, truncate_text};
use crate::uri::{AxiomUri, Scope};

const RELEASE_EVAL_MIN_TOP1_ACCURACY: f32 = 0.75;
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
            schema_uri,
            schema_version: None,
            schema_version_ok: false,
            object_type_count: 0,
            link_type_count: 0,
            action_type_count: 0,
            invariant_count: 0,
            invariant_check_passed: 0,
            invariant_check_failed: 0,
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
    ReleaseGateDecision {
        gate_id,
        passed,
        status: ReleaseGateStatus::from_passed(passed),
        details,
        evidence_uri,
    }
}

pub fn reliability_evidence_gate_decision(
    report: &ReliabilityEvidenceReport,
) -> ReleaseGateDecision {
    gate_decision(
        ReleaseGateId::ReliabilityEvidence,
        report.passed,
        ReleaseGateDetails::ReliabilityEvidence(ReliabilityGateDetails {
            status: report.status,
            replay_done: report.replay_totals.done,
            dead_letter: report.final_dead_letter,
        }),
        Some(report.report_uri.clone()),
    )
}

pub fn eval_quality_gate_decision(report: &EvalLoopReport) -> ReleaseGateDecision {
    let filter_ignored = eval_bucket_count(report, "filter_ignored");
    let relation_missing = eval_bucket_count(report, "relation_missing");
    let passed = report.executed_cases > 0
        && report.top1_accuracy >= RELEASE_EVAL_MIN_TOP1_ACCURACY
        && filter_ignored == 0
        && relation_missing == 0;
    gate_decision(
        ReleaseGateId::EvalQuality,
        passed,
        ReleaseGateDetails::EvalQuality(EvalQualityGateDetails {
            executed_cases: report.executed_cases,
            top1_accuracy: report.top1_accuracy,
            min_top1_accuracy: RELEASE_EVAL_MIN_TOP1_ACCURACY,
            failed: report.failed,
            filter_ignored,
            relation_missing,
        }),
        Some(report.report_uri.clone()),
    )
}

pub fn session_memory_gate_decision(
    passed: bool,
    memory_category_miss: usize,
    details: &str,
) -> ReleaseGateDecision {
    let gate_passed = passed && memory_category_miss == 0;
    gate_decision(
        ReleaseGateId::SessionMemory,
        gate_passed,
        ReleaseGateDetails::SessionMemory(SessionMemoryGateDetails {
            base_details: details.to_string(),
            memory_category_miss,
        }),
        None,
    )
}

pub fn security_audit_gate_decision(report: &SecurityAuditReport) -> ReleaseGateDecision {
    let strict_mode = report.dependency_audit.mode == ReleaseSecurityAuditMode::Strict;
    let passed = report.passed && strict_mode;
    gate_decision(
        ReleaseGateId::SecurityAudit,
        passed,
        ReleaseGateDetails::SecurityAudit(SecurityAuditGateDetails {
            status: report.status,
            mode: report.dependency_audit.mode,
            strict_mode_required: true,
            strict_mode,
            audit_status: report.dependency_audit.status,
            advisories_found: report.dependency_audit.advisories_found,
            packages: report.inventory.package_count,
        }),
        Some(report.report_uri.clone()),
    )
}

pub fn benchmark_release_gate_decision(report: &BenchmarkGateResult) -> ReleaseGateDecision {
    let evidence_uri = report
        .release_check_uri
        .clone()
        .or_else(|| report.gate_record_uri.clone());
    gate_decision(
        ReleaseGateId::Benchmark,
        report.passed,
        ReleaseGateDetails::Benchmark(BenchmarkGateDetails {
            passed: report.passed,
            evaluated_runs: report.evaluated_runs,
            passing_runs: report.passing_runs,
            reasons: report.reasons.clone(),
        }),
        evidence_uri,
    )
}

pub fn operability_evidence_gate_decision(
    report: &OperabilityEvidenceReport,
) -> ReleaseGateDecision {
    gate_decision(
        ReleaseGateId::OperabilityEvidence,
        report.passed,
        ReleaseGateDetails::OperabilityEvidence(OperabilityGateDetails {
            status: report.status,
            traces_analyzed: report.traces_analyzed,
            request_logs_scanned: report.request_logs_scanned,
        }),
        Some(report.report_uri.clone()),
    )
}

pub fn unresolved_blockers(decisions: &[ReleaseGateDecision]) -> usize {
    decisions.iter().filter(|d| !d.passed).count()
}

pub fn blocker_rollup_gate_decision(unresolved_blockers: usize) -> ReleaseGateDecision {
    gate_decision(
        ReleaseGateId::BlockerRollup,
        unresolved_blockers == 0,
        ReleaseGateDetails::BlockerRollup(BlockerRollupGateDetails {
            unresolved_blockers,
        }),
        None,
    )
}

pub fn finalize_release_gate_pack_report(
    pack_id: String,
    workspace_dir: String,
    mut decisions: Vec<ReleaseGateDecision>,
    report_uri: String,
) -> ReleaseGatePackReport {
    let unresolved_blockers = unresolved_blockers(&decisions);
    let g8 = blocker_rollup_gate_decision(unresolved_blockers);
    let passed = g8.passed;
    decisions.push(g8);

    ReleaseGatePackReport {
        pack_id,
        created_at: Utc::now().to_rfc3339(),
        workspace_dir,
        passed,
        status: ReleaseGateStatus::from_passed(passed),
        unresolved_blockers,
        decisions,
        report_uri,
    }
}

pub fn resolve_workspace_dir(workspace_dir: Option<&str>) -> Result<PathBuf> {
    let input = workspace_dir.unwrap_or(".");
    let raw = PathBuf::from(input);
    let absolute = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir()?.join(raw)
    };
    if !absolute.exists() {
        return Err(AxiomError::NotFound(format!(
            "workspace directory not found: {}",
            absolute.display()
        )));
    }
    let workspace = fs::canonicalize(absolute)?;
    if !workspace.join("Cargo.toml").exists() {
        return Err(AxiomError::Validation(format!(
            "workspace missing Cargo.toml: {}",
            workspace.display()
        )));
    }
    Ok(workspace)
}

pub fn evaluate_contract_integrity_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    let contract_probe = run_contract_execution_probe(workspace_dir);
    let episodic_semver_probe = run_episodic_semver_probe(workspace_dir);
    let episodic_api_probe = run_episodic_api_probe(workspace_dir);
    let ontology_policy = ontology_contract_policy();
    let ontology_probe = run_ontology_contract_probe(workspace_dir, &ontology_policy);
    let passed = contract_probe.passed
        && episodic_semver_probe.passed
        && episodic_api_probe.passed
        && ontology_probe.passed;
    let details = ReleaseGateDetails::ContractIntegrity(Box::new(ContractIntegrityGateDetails {
        policy: episodic_semver_policy(),
        contract_probe,
        episodic_api_probe,
        episodic_semver_probe,
        ontology_policy: Some(ontology_policy),
        ontology_probe: Some(ontology_probe),
    }));
    gate_decision(ReleaseGateId::ContractIntegrity, passed, details, None)
}

fn episodic_semver_policy() -> EpisodicSemverPolicy {
    EpisodicSemverPolicy {
        required_major: EPISODIC_REQUIRED_MAJOR,
        required_minor: EPISODIC_REQUIRED_MINOR,
        required_lock_source_prefix: CRATES_IO_INDEX_SOURCE.to_string(),
        allowed_manifest_operators: EPISODIC_ALLOWED_MANIFEST_OPERATORS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    }
}

fn ontology_contract_policy() -> OntologyContractPolicy {
    OntologyContractPolicy {
        schema_uri: crate::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string(),
        required_schema_version: 1,
        probe_test_name: ONTOLOGY_CONTRACT_PROBE_TEST_NAME.to_string(),
    }
}

pub fn evaluate_build_quality_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    let check = run_workspace_command(workspace_dir, "cargo", &["check", "--workspace"]);
    let fmt = run_workspace_command(workspace_dir, "cargo", &["fmt", "--all", "--check"]);
    let clippy = run_workspace_command(
        workspace_dir,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    );
    let passed = check.0 && fmt.0 && clippy.0;
    let details = ReleaseGateDetails::BuildQuality(BuildQualityGateDetails {
        cargo_check: check.0,
        cargo_fmt: fmt.0,
        cargo_clippy: clippy.0,
        check_output: truncate_text(&check.1, 240),
        fmt_output: truncate_text(&fmt.1, 240),
        clippy_output: truncate_text(&clippy.1, 240),
    });
    gate_decision(ReleaseGateId::BuildQuality, passed, details, None)
}

fn run_workspace_command(workspace_dir: &Path, cmd: &str, args: &[&str]) -> (bool, String) {
    #[cfg(test)]
    if let Some(mock) = run_workspace_command_mock(cmd, args) {
        return mock;
    }

    let operation = format!("release_gate:{cmd}");
    match run_host_command(
        HostCommandSpec::new(&operation, cmd, args).with_current_dir(workspace_dir),
    ) {
        HostCommandResult::Blocked { reason } => (false, reason),
        HostCommandResult::SpawnError { error } => (false, error),
        HostCommandResult::Completed {
            success,
            stdout,
            stderr,
        } => {
            let text = first_non_empty_output(&stdout, &stderr, OutputTrimMode::Preserve)
                .unwrap_or_default();
            (success, text)
        }
    }
}

#[cfg(test)]
fn workspace_command_key(cmd: &str, args: &[&str]) -> String {
    if args.is_empty() {
        cmd.to_string()
    } else {
        format!("{cmd} {}", args.join(" "))
    }
}

#[cfg(test)]
fn run_workspace_command_mock(cmd: &str, args: &[&str]) -> Option<(bool, String)> {
    let key = workspace_command_key(cmd, args);
    WORKSPACE_COMMAND_MOCK_STORE.with(|store| store.borrow().get(&key).cloned())
}

#[cfg(test)]
thread_local! {
    static WORKSPACE_COMMAND_MOCK_STORE: RefCell<HashMap<String, (bool, String)>> =
        RefCell::new(HashMap::new());
}

#[cfg(test)]
struct WorkspaceCommandMockResetGuard {
    previous: HashMap<String, (bool, String)>,
}

#[cfg(test)]
impl WorkspaceCommandMockResetGuard {
    fn install(mocks: &[(&str, &[&str], bool, &str)]) -> Self {
        let mut current = HashMap::new();
        for (cmd, args, ok, output) in mocks {
            current.insert(
                workspace_command_key(cmd, args),
                (*ok, (*output).to_string()),
            );
        }
        let previous = WORKSPACE_COMMAND_MOCK_STORE
            .with(|store| std::mem::replace(&mut *store.borrow_mut(), current));
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for WorkspaceCommandMockResetGuard {
    fn drop(&mut self) {
        let mut previous = HashMap::new();
        std::mem::swap(&mut previous, &mut self.previous);
        WORKSPACE_COMMAND_MOCK_STORE.with(|store| {
            *store.borrow_mut() = previous;
        });
    }
}

#[cfg(test)]
pub fn with_workspace_command_mocks<T>(
    mocks: &[(&str, &[&str], bool, &str)],
    run: impl FnOnce() -> T,
) -> T {
    let _reset = WorkspaceCommandMockResetGuard::install(mocks);
    run()
}

fn eval_bucket_count(report: &EvalLoopReport, name: &str) -> usize {
    report
        .buckets
        .iter()
        .find(|bucket| bucket.name == name)
        .map_or(0, |bucket| bucket.count)
}

fn run_contract_execution_probe(workspace_dir: &Path) -> CommandProbeResult {
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

fn run_episodic_api_probe(workspace_dir: &Path) -> CommandProbeResult {
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

fn run_ontology_contract_probe(
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
        schema_uri,
        schema_version: Some(schema_version),
        schema_version_ok,
        object_type_count,
        link_type_count,
        action_type_count,
        invariant_count,
        invariant_check_passed: invariant_report.passed,
        invariant_check_failed: invariant_report.failed,
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

fn run_episodic_semver_probe(workspace_dir: &Path) -> EpisodicSemverProbeResult {
    let core_manifest = workspace_dir
        .join("crates")
        .join("axiomme-core")
        .join("Cargo.toml");
    if !core_manifest.exists() {
        return EpisodicSemverProbeResult::from_error("missing_axiomme_core_crate".to_string());
    }

    let manifest_text = match fs::read_to_string(&core_manifest) {
        Ok(value) => value,
        Err(err) => {
            return EpisodicSemverProbeResult::from_error(format!(
                "manifest_read_error={} path={}",
                err,
                core_manifest.display()
            ));
        }
    };
    let manifest_dep = match parse_manifest_episodic_dependency(&manifest_text) {
        Ok(dep) => dep,
        Err(reason) => return EpisodicSemverProbeResult::from_error(reason),
    };

    let lock_path = workspace_dir.join("Cargo.lock");
    if !lock_path.exists() {
        return EpisodicSemverProbeResult::from_error(format!(
            "missing_workspace_lockfile path={}",
            lock_path.display()
        ));
    }
    let lock_text = match fs::read_to_string(&lock_path) {
        Ok(value) => value,
        Err(err) => {
            return EpisodicSemverProbeResult::from_error(format!(
                "lockfile_read_error={} path={}",
                err,
                lock_path.display()
            ));
        }
    };
    let lock_dep = match parse_lockfile_episodic_dependency(&lock_text) {
        Ok(dep) => dep,
        Err(reason) => return EpisodicSemverProbeResult::from_error(reason),
    };

    let manifest_req_ok = episodic_manifest_req_contract_matches(&manifest_dep.version_req);
    let manifest_source_ok = !manifest_dep.has_path && !manifest_dep.has_git;
    let lock_version_ok = episodic_lock_version_contract_matches(&lock_dep.version);
    let lock_source_ok = lock_dep
        .source
        .as_deref()
        .is_some_and(|source| source.starts_with(CRATES_IO_INDEX_SOURCE));

    let passed = manifest_req_ok && manifest_source_ok && lock_version_ok && lock_source_ok;
    EpisodicSemverProbeResult {
        passed,
        error: None,
        manifest_req: Some(manifest_dep.version_req),
        manifest_req_ok: Some(manifest_req_ok),
        manifest_uses_path: Some(manifest_dep.has_path),
        manifest_uses_git: Some(manifest_dep.has_git),
        manifest_source_ok: Some(manifest_source_ok),
        lock_version: Some(lock_dep.version),
        lock_version_ok: Some(lock_version_ok),
        lock_source: lock_dep.source,
        lock_source_ok: Some(lock_source_ok),
    }
}

fn parse_manifest_episodic_dependency(
    manifest: &str,
) -> std::result::Result<EpisodicManifestDependency, String> {
    let manifest_doc: toml::Value =
        toml::from_str(manifest).map_err(|err| format!("manifest_toml_parse_error={err}"))?;
    let dependencies = manifest_doc
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "manifest_missing_dependencies_table".to_string())?;
    let episodic = dependencies
        .get(EPISODIC_DEPENDENCY_NAME)
        .ok_or_else(|| "missing_episodic_dependency".to_string())?;

    match episodic {
        toml::Value::String(version_req) => Ok(EpisodicManifestDependency {
            version_req: version_req.to_string(),
            has_path: false,
            has_git: false,
        }),
        toml::Value::Table(fields) => {
            let version_req = fields
                .get("version")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| "episodic_dependency_missing_version".to_string())?;
            Ok(EpisodicManifestDependency {
                version_req: version_req.to_string(),
                has_path: fields.contains_key("path"),
                has_git: fields.contains_key("git"),
            })
        }
        _ => Err("episodic_dependency_unsupported_shape".to_string()),
    }
}

fn parse_lockfile_episodic_dependency(
    lockfile: &str,
) -> std::result::Result<EpisodicLockDependency, String> {
    let lock_doc: toml::Value =
        toml::from_str(lockfile).map_err(|err| format!("lockfile_toml_parse_error={err}"))?;
    let packages = lock_doc
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "lockfile_missing_package_array".to_string())?;

    for package in packages {
        let Some(package_table) = package.as_table() else {
            continue;
        };
        let name = package_table
            .get("name")
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        if name != EPISODIC_DEPENDENCY_NAME {
            continue;
        }
        let version = package_table
            .get("version")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| "lockfile_episodic_missing_version".to_string())?
            .to_string();
        let source = package_table
            .get("source")
            .and_then(toml::Value::as_str)
            .map(str::to_string);
        return Ok(EpisodicLockDependency { version, source });
    }

    Err("missing_episodic_lock_entry".to_string())
}

fn episodic_manifest_req_contract_matches(raw: &str) -> bool {
    let requirement = match VersionReq::parse(raw.trim()) {
        Ok(value) => value,
        Err(_) => return false,
    };
    if requirement.comparators.len() != 1 {
        return false;
    }
    comparator_matches_episodic_contract(&requirement.comparators[0])
}

fn comparator_matches_episodic_contract(comparator: &Comparator) -> bool {
    if !matches!(comparator.op, Op::Exact | Op::Caret | Op::Tilde) {
        return false;
    }
    comparator.major == EPISODIC_REQUIRED_MAJOR && comparator.minor == Some(EPISODIC_REQUIRED_MINOR)
}

fn episodic_lock_version_contract_matches(raw: &str) -> bool {
    Version::parse(raw.trim()).is_ok_and(|version| {
        version.major == EPISODIC_REQUIRED_MAJOR && version.minor == EPISODIC_REQUIRED_MINOR
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::models::{
        BenchmarkGateRunResult, BenchmarkSummary, DependencyAuditSummary,
        DependencyInventorySummary, EvalBucket, EvalCaseResult,
    };

    fn eval_report(executed_cases: usize, top1_accuracy: f32) -> EvalLoopReport {
        EvalLoopReport {
            run_id: "run-1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            trace_limit: 10,
            query_limit: 10,
            search_limit: 5,
            include_golden: true,
            golden_only: false,
            traces_scanned: 10,
            trace_cases_used: 5,
            golden_cases_used: 5,
            executed_cases,
            passed: 0,
            failed: 0,
            top1_accuracy,
            buckets: Vec::<EvalBucket>::new(),
            report_uri: "axiom://queue/eval/reports/x.json".to_string(),
            query_set_uri: "axiom://queue/eval/query_sets/x.json".to_string(),
            markdown_report_uri: "axiom://queue/eval/reports/x.md".to_string(),
            failures: Vec::<EvalCaseResult>::new(),
        }
    }

    fn benchmark_gate_result(
        release_check_uri: Option<&str>,
        gate_record_uri: Option<&str>,
    ) -> BenchmarkGateResult {
        BenchmarkGateResult {
            passed: true,
            gate_profile: "rc-release".to_string(),
            threshold_p95_ms: 1000,
            min_top1_accuracy: 0.75,
            min_stress_top1_accuracy: None,
            max_p95_regression_pct: Some(0.1),
            max_top1_regression_pct: Some(2.0),
            window_size: 3,
            required_passes: 1,
            evaluated_runs: 1,
            passing_runs: 1,
            latest: Some(BenchmarkSummary {
                run_id: "run".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                executed_cases: 10,
                top1_accuracy: 0.9,
                p95_latency_ms: 700,
                p95_latency_us: Some(699_420),
                report_uri: "axiom://queue/benchmarks/reports/run.json".to_string(),
            }),
            previous: None,
            regression_pct: None,
            top1_regression_pct: None,
            stress_top1_accuracy: None,
            run_results: vec![BenchmarkGateRunResult {
                run_id: "run".to_string(),
                passed: true,
                p95_latency_ms: 700,
                p95_latency_us: Some(699_420),
                top1_accuracy: 0.9,
                stress_top1_accuracy: None,
                regression_pct: None,
                top1_regression_pct: None,
                reasons: vec!["ok".to_string()],
            }],
            gate_record_uri: gate_record_uri.map(ToString::to_string),
            release_check_uri: release_check_uri.map(ToString::to_string),
            embedding_provider: Some("semantic-model-http".to_string()),
            embedding_strict_error: None,
            reasons: vec!["ok".to_string()],
        }
    }

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
        report.buckets = vec![
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

    fn write_contract_gate_workspace_fixture(
        root: &Path,
        episodic_dep: &str,
        lock_source: Option<&str>,
    ) {
        let core = root.join("crates").join("axiomme-core");
        fs::create_dir_all(&core).expect("mkdir core");

        fs::write(
            core.join("Cargo.toml"),
            format!(
                "[package]\nname=\"axiomme-core\"\nversion=\"0.1.0\"\n\n[dependencies]\n{episodic_dep}\n"
            ),
        )
        .expect("write core cargo");

        let lock_source_line = lock_source
            .map(|value| format!("source = \"{value}\"\n"))
            .unwrap_or_default();
        fs::write(
            root.join("Cargo.lock"),
            format!("[[package]]\nname = \"episodic\"\nversion = \"0.1.0\"\n{lock_source_line}\n"),
        )
        .expect("write lockfile");
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
        let episodic_output =
            format!("running 1 test\ntest {EPISODIC_API_PROBE_TEST_NAME} ... ok\n");
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
}
