use crate::error::{AxiomError, Result};
use crate::models::{
    BenchmarkFixtureDocument, EvalGoldenDocument, EvalQueryCase, TraceMetricsSnapshotDocument,
};
use crate::uri::{AxiomUri, Scope};

pub(crate) fn eval_case_key(case: &EvalQueryCase) -> (String, Option<String>) {
    (case.query.trim().to_lowercase(), case.target_uri.clone())
}

pub(crate) fn eval_case_ordering(a: &EvalQueryCase, b: &EvalQueryCase) -> std::cmp::Ordering {
    eval_case_key(a)
        .cmp(&eval_case_key(b))
        .then_with(|| a.expected_top_uri.cmp(&b.expected_top_uri))
}

pub(crate) fn normalize_eval_case_source(case: &mut EvalQueryCase, fallback: &str) {
    if case.source.trim().is_empty() {
        case.source = fallback.to_string();
    }
}

pub(crate) fn parse_golden_cases_document(raw: &str) -> Result<Vec<EvalQueryCase>> {
    let doc = serde_json::from_str::<EvalGoldenDocument>(raw)?;
    Ok(doc.cases)
}

pub(crate) fn parse_benchmark_fixture_document(raw: &str) -> Result<BenchmarkFixtureDocument> {
    serde_json::from_str::<BenchmarkFixtureDocument>(raw).map_err(AxiomError::from)
}

pub(crate) fn parse_trace_metrics_snapshot_document(
    raw: &str,
) -> Result<TraceMetricsSnapshotDocument> {
    serde_json::from_str::<TraceMetricsSnapshotDocument>(raw).map_err(AxiomError::from)
}

pub(crate) fn request_log_uri() -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("logs")?
        .join("requests.jsonl")
}

fn eval_base_uri() -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue).join("eval")
}

pub(crate) fn eval_golden_uri() -> Result<AxiomUri> {
    eval_base_uri()?.join("golden_queries.json")
}

pub(crate) fn eval_query_set_uri(run_id: &str) -> Result<AxiomUri> {
    eval_base_uri()?
        .join("query_sets")?
        .join(&format!("{run_id}.json"))
}

pub(crate) fn eval_report_json_uri(run_id: &str) -> Result<AxiomUri> {
    eval_base_uri()?
        .join("reports")?
        .join(&format!("{run_id}.json"))
}

pub(crate) fn eval_report_markdown_uri(run_id: &str) -> Result<AxiomUri> {
    eval_base_uri()?
        .join("reports")?
        .join(&format!("{run_id}.md"))
}

pub(crate) fn benchmark_base_uri() -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue).join("benchmarks")
}

pub(crate) fn benchmark_fixture_uri(name: &str) -> Result<AxiomUri> {
    benchmark_base_uri()?
        .join("fixtures")?
        .join(&format!("{}.json", sanitize_component(name)))
}

pub(crate) fn benchmark_case_set_uri(run_id: &str) -> Result<AxiomUri> {
    benchmark_base_uri()?
        .join("query_sets")?
        .join(&format!("{run_id}.json"))
}

pub(crate) fn benchmark_report_json_uri(run_id: &str) -> Result<AxiomUri> {
    benchmark_base_uri()?
        .join("reports")?
        .join(&format!("{run_id}.json"))
}

pub(crate) fn benchmark_report_markdown_uri(run_id: &str) -> Result<AxiomUri> {
    benchmark_base_uri()?
        .join("reports")?
        .join(&format!("{run_id}.md"))
}

pub(crate) fn benchmark_gate_result_uri(run_id: &str) -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("release")?
        .join("gates")?
        .join(&format!("{run_id}.json"))
}

fn trace_metrics_base_uri() -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue).join("metrics")?.join("traces")
}

pub(crate) fn trace_metrics_snapshots_uri() -> Result<AxiomUri> {
    trace_metrics_base_uri()?.join("snapshots")
}

pub(crate) fn trace_metrics_snapshot_uri(snapshot_id: &str) -> Result<AxiomUri> {
    trace_metrics_snapshots_uri()?.join(&format!("{snapshot_id}.json"))
}

pub(crate) fn release_check_result_uri(check_id: &str) -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("release")?
        .join("checks")?
        .join(&format!("{check_id}.json"))
}

pub(crate) fn security_audit_report_uri(report_id: &str) -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("release")?
        .join("security")?
        .join(&format!("{report_id}.json"))
}

pub(crate) fn operability_evidence_report_uri(report_id: &str) -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("release")?
        .join("operability")?
        .join(&format!("{report_id}.json"))
}

pub(crate) fn reliability_evidence_report_uri(report_id: &str) -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("release")?
        .join("reliability")?
        .join(&format!("{report_id}.json"))
}

pub(crate) fn normalize_gate_profile(profile: &str) -> String {
    let trimmed = profile.trim();
    if trimmed.is_empty() {
        "custom".to_string()
    } else {
        sanitize_component(trimmed)
    }
}

pub(crate) fn sanitize_component(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if (c == '-' || c == '_' || c == '.') && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "resource".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_component_normalizes_and_trims() {
        assert_eq!(sanitize_component("OAuth Flow@V2.md"), "oauthflowv2-md");
        assert_eq!(sanitize_component("___"), "resource");
    }

    #[test]
    fn normalize_gate_profile_defaults_to_custom_when_empty() {
        assert_eq!(normalize_gate_profile("   "), "custom");
    }

    #[test]
    fn benchmark_fixture_uri_sanitizes_name() {
        let uri = benchmark_fixture_uri("RC Release Fixture").expect("fixture uri");
        assert_eq!(
            uri.to_string(),
            "axiom://queue/benchmarks/fixtures/rcreleasefixture.json"
        );
    }

    #[test]
    fn parse_benchmark_fixture_document_rejects_legacy_array_shape() {
        let raw = r#"[{"source_trace_id":"t1","query":"oauth","target_uri":null,"expected_top_uri":null,"source":"legacy"}]"#;
        let err = parse_benchmark_fixture_document(raw).expect_err("must reject legacy shape");
        assert_eq!(err.code(), "JSON_ERROR");
    }
}
