use std::io::Read;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context, Result};
use axiomme_core::AxiomMe;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::cli::OntologyCommand;

use super::support::print_json;

pub(super) fn handle_ontology_command(app: &AxiomMe, command: OntologyCommand) -> Result<()> {
    match command {
        OntologyCommand::Validate { uri } => {
            let uri =
                uri.unwrap_or_else(|| axiomme_core::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string());
            let raw = app.read(&uri)?;
            let schema = axiomme_core::ontology::parse_schema_v1(&raw)?;
            let version = schema.version;
            let object_type_count = schema.object_types.len();
            let link_type_count = schema.link_types.len();
            let action_type_count = schema.action_types.len();
            let invariant_count = schema.invariants.len();
            let _compiled = axiomme_core::ontology::compile_schema(schema)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "uri": uri,
                "schema_version": version,
                "object_type_count": object_type_count,
                "link_type_count": link_type_count,
                "action_type_count": action_type_count,
                "invariant_count": invariant_count
            }))?;
        }
        OntologyCommand::Pressure {
            uri,
            min_action_types,
            min_invariants,
            min_action_invariant_total,
            min_link_types_per_object_basis_points,
        } => {
            let uri =
                uri.unwrap_or_else(|| axiomme_core::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string());
            let raw = app.read(&uri)?;
            let schema = axiomme_core::ontology::parse_schema_v1(&raw)?;
            let _compiled = axiomme_core::ontology::compile_schema(schema.clone())?;
            let policy = axiomme_core::ontology::OntologyV2PressurePolicy {
                min_action_types,
                min_invariants,
                min_action_invariant_total,
                min_link_types_per_object_basis_points,
            };
            let report = axiomme_core::ontology::evaluate_v2_pressure(&schema, policy);
            print_json(&serde_json::json!({
                "status": "ok",
                "uri": uri,
                "report": report
            }))?;
        }
        OntologyCommand::Trend {
            history_dir,
            min_samples,
            consecutive_v2_candidate,
        } => {
            let samples = load_ontology_pressure_samples(&history_dir)?;
            let policy = axiomme_core::ontology::validate_v2_pressure_trend_policy(
                axiomme_core::ontology::OntologyV2PressureTrendPolicy {
                    min_samples,
                    consecutive_v2_candidate,
                },
            )?;
            let report = axiomme_core::ontology::evaluate_v2_pressure_trend(samples, policy);
            print_json(&serde_json::json!({
                "status": "ok",
                "history_dir": history_dir,
                "report": report
            }))?;
        }
        OntologyCommand::ActionValidate {
            uri,
            action_id,
            queue_event_type,
            input_json,
            input_file,
            input_stdin,
        } => {
            let uri =
                uri.unwrap_or_else(|| axiomme_core::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string());
            let raw = app.read(&uri)?;
            let parsed = axiomme_core::ontology::parse_schema_v1(&raw)?;
            let compiled = axiomme_core::ontology::compile_schema(parsed)?;
            let input = read_ontology_action_input(input_json, input_file, input_stdin)?;
            let request = axiomme_core::ontology::OntologyActionRequestV1 {
                action_id,
                queue_event_type,
                input,
            };
            let report = axiomme_core::ontology::validate_action_request(&compiled, &request)?;
            print_json(&serde_json::json!({
                "status": "ok",
                "uri": uri,
                "report": report
            }))?;
        }
        OntologyCommand::ActionEnqueue {
            uri,
            target_uri,
            action_id,
            queue_event_type,
            input_json,
            input_file,
            input_stdin,
        } => {
            let uri =
                uri.unwrap_or_else(|| axiomme_core::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string());
            let input = read_ontology_action_input(input_json, input_file, input_stdin)?;
            let (event_id, target_uri, report) = app.enqueue_ontology_action(
                &uri,
                &target_uri,
                &action_id,
                &queue_event_type,
                input,
            )?;
            print_json(&serde_json::json!({
                "status": "ok",
                "uri": uri,
                "target_uri": target_uri,
                "event_id": event_id,
                "report": report
            }))?;
        }
        OntologyCommand::InvariantCheck { uri, enforce } => {
            let uri =
                uri.unwrap_or_else(|| axiomme_core::ontology::ONTOLOGY_SCHEMA_URI_V1.to_string());
            let raw = app.read(&uri)?;
            let parsed = axiomme_core::ontology::parse_schema_v1(&raw)?;
            let compiled = axiomme_core::ontology::compile_schema(parsed)?;
            let report = axiomme_core::ontology::evaluate_invariants(&compiled);
            print_json(&serde_json::json!({
                "status": "ok",
                "uri": uri,
                "report": report
            }))?;
            if enforce && report.failed > 0 {
                anyhow::bail!(
                    "ontology invariant check failed: {} invariant(s) failed",
                    report.failed
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct OntologyPressureSnapshotEnvelope {
    generated_at_utc: String,
    #[serde(default)]
    label: Option<String>,
    pressure: OntologyPressureSnapshotPayload,
}

#[derive(Debug, Deserialize)]
struct OntologyPressureSnapshotPayload {
    report: axiomme_core::ontology::OntologyV2PressureReport,
}

pub(super) fn load_ontology_pressure_samples(
    history_dir: &Path,
) -> Result<Vec<axiomme_core::ontology::OntologyV2PressureSample>> {
    if !history_dir.exists() {
        anyhow::bail!(
            "ontology pressure history directory does not exist: {}",
            history_dir.display()
        );
    }
    if !history_dir.is_dir() {
        anyhow::bail!(
            "ontology pressure history path is not a directory: {}",
            history_dir.display()
        );
    }

    let mut snapshot_paths = Vec::<PathBuf>::new();
    for entry in fs::read_dir(history_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            snapshot_paths.push(path);
        }
    }
    snapshot_paths.sort();
    if snapshot_paths.is_empty() {
        anyhow::bail!(
            "ontology pressure history has no JSON snapshots: {}",
            history_dir.display()
        );
    }

    let mut loaded_snapshots =
        Vec::<(DateTime<Utc>, PathBuf, OntologyPressureSnapshotEnvelope)>::new();
    for snapshot_path in snapshot_paths {
        let raw = fs::read_to_string(&snapshot_path).with_context(|| {
            format!(
                "failed to read ontology pressure snapshot: {}",
                snapshot_path.display()
            )
        })?;
        let envelope = serde_json::from_str::<OntologyPressureSnapshotEnvelope>(&raw)
            .with_context(|| {
                format!(
                    "invalid ontology pressure snapshot JSON: {}",
                    snapshot_path.display()
                )
            })?;
        let generated_at = DateTime::parse_from_rfc3339(&envelope.generated_at_utc)
            .with_context(|| {
                format!(
                    "invalid ontology pressure snapshot timestamp (generated_at_utc) in {}: {}",
                    snapshot_path.display(),
                    envelope.generated_at_utc
                )
            })?
            .with_timezone(&Utc);
        loaded_snapshots.push((generated_at, snapshot_path, envelope));
    }
    loaded_snapshots.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut samples = Vec::<axiomme_core::ontology::OntologyV2PressureSample>::new();
    for (_, snapshot_path, envelope) in loaded_snapshots {
        let file_name = snapshot_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown");
        let label = envelope.label.unwrap_or_else(|| "snapshot".to_string());
        samples.push(axiomme_core::ontology::OntologyV2PressureSample {
            sample_id: format!("{label}:{file_name}"),
            generated_at_utc: envelope.generated_at_utc,
            v2_candidate: envelope.pressure.report.v2_candidate,
            trigger_reasons: envelope.pressure.report.trigger_reasons,
        });
    }
    Ok(samples)
}

fn read_ontology_action_input(
    inline_json: Option<String>,
    from: Option<std::path::PathBuf>,
    stdin: bool,
) -> Result<serde_json::Value> {
    validate_ontology_action_input_source_selection(
        inline_json.as_deref(),
        from.as_deref(),
        stdin,
    )?;

    let raw = if let Some(inline_json) = inline_json {
        Some(inline_json)
    } else if let Some(path) = from {
        Some(fs::read_to_string(path)?)
    } else if stdin {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        Some(buffer)
    } else {
        None
    };

    match raw {
        Some(raw) => serde_json::from_str(raw.as_str())
            .with_context(|| "invalid ontology action input JSON".to_string()),
        None => Ok(serde_json::Value::Null),
    }
}

pub(super) fn validate_ontology_action_input_source_selection(
    inline_json: Option<&str>,
    from: Option<&std::path::Path>,
    stdin: bool,
) -> Result<()> {
    let selected =
        bool_to_count(inline_json.is_some()) + bool_to_count(from.is_some()) + bool_to_count(stdin);
    if selected > 1 {
        anyhow::bail!(
            "ontology action input accepts at most one source: --input-json, --input-file, --input-stdin"
        );
    }
    Ok(())
}

const fn bool_to_count(value: bool) -> u8 {
    if value { 1 } else { 0 }
}
