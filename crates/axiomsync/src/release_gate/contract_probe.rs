use std::collections::BTreeMap;
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

const RELATION_TRACE_LOGS_PATH: &str = "crates/axiomsync/src/client/tests/relation_trace_logs.rs";
type PromptSignatureVersionKey = (String, String);
type PromptSignatureFieldMap = BTreeMap<String, String>;
type PromptSignaturePolicyMap = BTreeMap<PromptSignatureVersionKey, PromptSignatureFieldMap>;

pub(super) fn run_contract_execution_probe(workspace_dir: &Path) -> CommandProbeResult {
    let core_crate = workspace_dir
        .join("crates")
        .join("axiomsync")
        .join("Cargo.toml");
    if !core_crate.exists() {
        return CommandProbeResult::from_error(
            CONTRACT_EXECUTION_TEST_NAME,
            "missing_axiomsync_crate".to_string(),
        );
    }

    let (ok, output) = run_workspace_command(
        workspace_dir,
        "cargo",
        &[
            "test",
            "-p",
            "axiomsync",
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
            "axiomsync",
            EPISODIC_API_PROBE_TEST_NAME,
            "--",
            "--exact",
        ],
    );
    let probe = CommandProbeResult::from_test_run(EPISODIC_API_PROBE_TEST_NAME, ok, output);
    if !probe.passed {
        return probe;
    }
    if let Err(reason) = verify_prompt_contract_version_bump_policy(workspace_dir) {
        return CommandProbeResult::from_error(
            EPISODIC_API_PROBE_TEST_NAME,
            format!("prompt_contract_version_bump_policy_failed: {reason}"),
        );
    }
    probe
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
            "axiomsync",
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
        "axiomsync-ontology-contract-probe-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let app = crate::AxiomSync::new(&probe_root)
        .map_err(|err| format!("ontology_probe_app_new_failed: {err}"))?;
    let loaded = (|| -> Result<crate::ontology::OntologySchemaV1> {
        app.bootstrap()?;
        let raw = app.read(schema_uri)?;
        crate::ontology::parse_schema_v1(&raw)
    })();
    let _ = fs::remove_dir_all(&probe_root);
    loaded.map_err(|err| format!("ontology_probe_schema_load_failed: {err}"))
}

fn verify_prompt_contract_version_bump_policy(
    workspace_dir: &Path,
) -> std::result::Result<(), String> {
    let (rev_ok, _rev_output) =
        run_workspace_command(workspace_dir, "git", &["rev-parse", "--verify", "HEAD~1"]);
    if !rev_ok {
        // Shallow/squash histories may not expose HEAD~1; keep the gate portable
        // by validating current policy shape instead of hard-failing.
        return parse_prompt_signature_policy_for_revision(workspace_dir, "HEAD").map(|_| ());
    }
    let previous = match parse_prompt_signature_policy_for_revision(workspace_dir, "HEAD~1") {
        Ok(value) => value,
        Err(reason) if should_fallback_to_current_policy_shape(&reason) => {
            return parse_prompt_signature_policy_for_revision(workspace_dir, "HEAD").map(|_| ());
        }
        Err(reason) => {
            return Err(format!(
                "previous_prompt_signature_policy_load_failed: {reason}"
            ));
        }
    };
    let current = parse_prompt_signature_policy_for_revision(workspace_dir, "HEAD")
        .map_err(|reason| format!("current_prompt_signature_policy_load_failed: {reason}"))?;

    if prompt_contract_signature_changed_without_version_bump(&previous, &current) {
        return Err(
            "prompt_contract_signature_changed_without_contract_or_protocol_version_bump"
                .to_string(),
        );
    }
    Ok(())
}

fn should_fallback_to_current_policy_shape(reason: &str) -> bool {
    reason.contains("git_show_failed")
        && (reason.contains("does not exist in") || reason.contains("exists on disk, but not in"))
}

fn should_load_current_policy_from_workspace(reason: &str) -> bool {
    reason.contains("git_show_failed")
        && (reason.contains("does not exist in 'HEAD'")
            || reason.contains("exists on disk, but not in 'HEAD'"))
}

fn parse_current_prompt_signature_policy_from_workspace(
    workspace_dir: &Path,
) -> std::result::Result<PromptSignaturePolicyMap, String> {
    let source_path = workspace_dir.join(RELATION_TRACE_LOGS_PATH);
    let output = fs::read_to_string(&source_path).map_err(|err| {
        format!(
            "workspace_read_failed path={} reason={err}",
            source_path.display()
        )
    })?;
    parse_prompt_signature_policy_map(&output)
        .map_err(|reason| format!("workspace_prompt_signature_policy_parse_failed: {reason}"))
}

fn parse_prompt_signature_policy_for_revision(
    workspace_dir: &Path,
    revision: &str,
) -> std::result::Result<PromptSignaturePolicyMap, String> {
    let (ok, output) = run_workspace_command(
        workspace_dir,
        "git",
        &["show", &format!("{revision}:{RELATION_TRACE_LOGS_PATH}")],
    );
    if !ok {
        if revision == "HEAD"
            && should_load_current_policy_from_workspace(&format!(
                "git_show_failed revision={revision} reason={}",
                output.trim()
            ))
        {
            return parse_current_prompt_signature_policy_from_workspace(workspace_dir);
        }
        return Err(format!(
            "git_show_failed revision={revision} reason={}",
            output.trim()
        ));
    }
    parse_prompt_signature_policy_map(&output).map_err(|reason| {
        format!("prompt_signature_policy_parse_failed revision={revision} reason={reason}")
    })
}

fn parse_prompt_signature_policy_map(
    source: &str,
) -> std::result::Result<PromptSignaturePolicyMap, String> {
    let mut map = PromptSignaturePolicyMap::new();
    let mut current_key: Option<PromptSignatureVersionKey> = None;
    let mut current_signatures = PromptSignatureFieldMap::new();
    let mut saw_prompt_contract_entry = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some((contract_version, protocol_version)) =
            parse_prompt_signature_entry_header(trimmed)
        {
            saw_prompt_contract_entry = true;
            if current_key.is_some() {
                return Err("prompt_signature_policy_nested_entry_detected".to_string());
            }
            current_key = Some((contract_version, protocol_version));
            current_signatures = BTreeMap::new();
            continue;
        }
        if current_key.is_none() {
            continue;
        }
        if let Some((field, signature)) = parse_prompt_signature_field(trimmed) {
            current_signatures.insert(field, signature);
            continue;
        }
        if trimmed.starts_with("}),") || trimmed.starts_with("})") {
            let key = current_key
                .take()
                .ok_or_else(|| "prompt_signature_policy_entry_close_without_open".to_string())?;
            if current_signatures.is_empty() {
                return Err(format!(
                    "prompt_signature_policy_entry_missing_signatures:{}/{}",
                    key.0, key.1
                ));
            }
            if map
                .insert(key, std::mem::take(&mut current_signatures))
                .is_some()
            {
                return Err("prompt_signature_policy_duplicate_version_tuple".to_string());
            }
        }
    }

    if current_key.is_some() {
        return Err("prompt_signature_policy_entry_not_closed".to_string());
    }
    if !saw_prompt_contract_entry || map.is_empty() {
        return Err("prompt_signature_policy_entries_not_found".to_string());
    }
    Ok(map)
}

fn parse_prompt_signature_entry_header(line: &str) -> Option<(String, String)> {
    let marker = "=> Some(PromptContractSignatures";
    let marker_index = line.find(marker)?;
    let tuple_part = line[..marker_index].trim();
    let tuple_part = tuple_part.strip_prefix("(\"")?;
    let contract_end = tuple_part.find('"')?;
    let contract_version = tuple_part[..contract_end].to_string();
    let rest = tuple_part[contract_end + 1..].trim_start();
    let rest = rest.strip_prefix(',')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let protocol_end = rest.find('"')?;
    let protocol_version = rest[..protocol_end].to_string();
    Some((contract_version, protocol_version))
}

fn parse_prompt_signature_field(line: &str) -> Option<(String, String)> {
    let field_end = line.find(':')?;
    let field = line[..field_end].trim();
    if !field.ends_with("_blake3") {
        return None;
    }
    let value_part = line[field_end + 1..].trim_start();
    let value_part = value_part.strip_prefix('"')?;
    let value_end = value_part.find('"')?;
    let value = value_part[..value_end].to_string();
    Some((field.to_string(), value))
}

fn prompt_contract_signature_changed_without_version_bump(
    previous: &PromptSignaturePolicyMap,
    current: &PromptSignaturePolicyMap,
) -> bool {
    for (version_tuple, current_signatures) in current {
        let Some(previous_signatures) = previous.get(version_tuple) else {
            continue;
        };
        if previous_signatures != current_signatures {
            return true;
        }
    }
    false
}
