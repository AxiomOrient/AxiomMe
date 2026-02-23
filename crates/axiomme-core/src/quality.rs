use std::fmt::Write as _;
use std::time::Duration;

use chrono::Utc;

use crate::host_tools::{HostCommandResult, HostCommandSpec, run_host_command};
use crate::models::{
    BenchmarkAcceptanceCheck, BenchmarkAcceptanceMeasured, BenchmarkAcceptanceResult,
    BenchmarkAcceptanceThresholds, BenchmarkQuerySetMetadata, BenchmarkReport, BenchmarkSummary,
    EvalLoopReport, EvalQueryCase, TraceMetricsSnapshotDocument, TraceMetricsSnapshotSummary,
};
use crate::uri::AxiomUri;

pub fn percentile_u128(sorted: &[u128], percentile_basis_points: u16) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let bounded = usize::from(percentile_basis_points.min(10_000));
    let len_minus_one = sorted.len() - 1;
    let rank = (len_minus_one.saturating_mul(bounded) + 5_000) / 10_000;
    sorted[rank.min(sorted.len() - 1)]
}

#[allow(
    clippy::cast_precision_loss,
    reason = "latency summaries are emitted as compact f32 metrics"
)]
pub fn average_latency_ms(values: &[u128]) -> f32 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().copied().sum::<u128>() as f32 / values.len() as f32
    }
}

#[must_use]
pub fn duration_to_latency_ms(duration: Duration) -> u128 {
    let nanos = duration.as_nanos();
    if nanos == 0 {
        return 0;
    }
    nanos.saturating_add(999_999) / 1_000_000
}

#[must_use]
pub fn duration_to_latency_us(duration: Duration) -> u128 {
    let nanos = duration.as_nanos();
    if nanos == 0 {
        return 0;
    }
    nanos.saturating_add(999) / 1_000
}

pub fn command_stdout(cmd: &str, args: &[&str]) -> Option<String> {
    let operation = format!("quality:{cmd}");
    match run_host_command(HostCommandSpec::new(&operation, cmd, args)) {
        HostCommandResult::Completed {
            success: true,
            stdout,
            ..
        } => {
            let text = stdout.trim().to_string();
            if text.is_empty() { None } else { Some(text) }
        }
        _ => None,
    }
}

pub fn infer_corpus_profile(file_count: usize, total_bytes: u64) -> String {
    if file_count >= 5_000 || total_bytes >= 1_000_000_000 {
        "M".to_string()
    } else if file_count >= 1_500 || total_bytes >= 300_000_000 {
        "S".to_string()
    } else {
        "custom".to_string()
    }
}

fn classify_benchmark_query(query: &str) -> &'static str {
    let q = query.trim();
    if q.is_empty() {
        return "mixed";
    }

    let token_count = q.split_whitespace().count();
    let has_symbolic_marker =
        q.contains("::") || q.contains('/') || q.contains('.') || q.contains('_');
    let has_digit = q.chars().any(|ch| ch.is_ascii_digit());
    let has_phrase = token_count >= 4;

    if (has_symbolic_marker || has_digit) && !has_phrase {
        "lexical"
    } else if has_symbolic_marker || has_digit {
        "mixed"
    } else if has_phrase {
        "semantic"
    } else {
        "mixed"
    }
}

pub fn build_benchmark_query_set_metadata(
    query_cases: &[EvalQueryCase],
    fixture_name: Option<&str>,
) -> BenchmarkQuerySetMetadata {
    let mut semantic_queries = 0usize;
    let mut lexical_queries = 0usize;
    let mut mixed_queries = 0usize;
    for case in query_cases {
        match classify_benchmark_query(&case.query) {
            "semantic" => semantic_queries += 1,
            "lexical" => lexical_queries += 1,
            _ => mixed_queries += 1,
        }
    }

    let total_queries = query_cases.len();
    let warmup_queries = total_queries.min(20);
    let measured_queries = total_queries.saturating_sub(warmup_queries);
    let date = Utc::now().format("%Y%m%d").to_string();
    let version = format!("qset-v1-{date}");
    let source = fixture_name.map_or_else(
        || infer_generated_query_source(query_cases),
        |name| format!("fixture:{name}"),
    );

    BenchmarkQuerySetMetadata {
        version,
        source,
        total_queries,
        semantic_queries,
        lexical_queries,
        mixed_queries,
        warmup_queries,
        measured_queries,
    }
}

fn infer_generated_query_source(query_cases: &[EvalQueryCase]) -> String {
    let mut has_golden = false;
    let mut has_trace = false;
    let mut has_stress = false;

    for case in query_cases {
        let source = case.source.trim().to_ascii_lowercase();
        if source.starts_with("golden") {
            has_golden = true;
        } else if source.starts_with("stress:") {
            has_stress = true;
        } else if source.starts_with("trace") {
            has_trace = true;
        }
    }

    let mut parts = Vec::<&str>::new();
    if has_golden {
        parts.push("golden");
    }
    if has_stress {
        parts.push("stress");
    }
    if has_trace {
        parts.push("trace");
    }
    if parts.is_empty() {
        parts.push("unknown");
    }
    format!("generated:{}", parts.join("+"))
}

pub fn build_benchmark_acceptance_result(
    find_p95_latency_ms: u128,
    search_p95_latency_ms: u128,
    commit_p95_latency_ms: u128,
    ndcg_at_10: f32,
    recall_at_10: f32,
    query_set: &BenchmarkQuerySetMetadata,
) -> BenchmarkAcceptanceResult {
    let thresholds = BenchmarkAcceptanceThresholds {
        find_p95_latency_ms_max: 600,
        search_p95_latency_ms_max: 1_200,
        commit_p95_latency_ms_max: 1_500,
        min_ndcg_at_10: 0.75,
        min_recall_at_10: 0.85,
        min_total_queries: 120,
        min_semantic_queries: 60,
        min_lexical_queries: 40,
        min_mixed_queries: 20,
    };
    let measured = BenchmarkAcceptanceMeasured {
        find_p95_latency_ms,
        search_p95_latency_ms,
        commit_p95_latency_ms,
        ndcg_at_10,
        recall_at_10,
        total_queries: query_set.total_queries,
        semantic_queries: query_set.semantic_queries,
        lexical_queries: query_set.lexical_queries,
        mixed_queries: query_set.mixed_queries,
    };

    let checks = vec![
        BenchmarkAcceptanceCheck {
            name: "find_p95_latency".to_string(),
            passed: measured.find_p95_latency_ms <= thresholds.find_p95_latency_ms_max,
            expected: format!("<= {}ms", thresholds.find_p95_latency_ms_max),
            actual: format!("{}ms", measured.find_p95_latency_ms),
        },
        BenchmarkAcceptanceCheck {
            name: "search_p95_latency".to_string(),
            passed: measured.search_p95_latency_ms <= thresholds.search_p95_latency_ms_max,
            expected: format!("<= {}ms", thresholds.search_p95_latency_ms_max),
            actual: format!("{}ms", measured.search_p95_latency_ms),
        },
        BenchmarkAcceptanceCheck {
            name: "commit_p95_latency".to_string(),
            passed: measured.commit_p95_latency_ms <= thresholds.commit_p95_latency_ms_max,
            expected: format!("<= {}ms", thresholds.commit_p95_latency_ms_max),
            actual: format!("{}ms", measured.commit_p95_latency_ms),
        },
        BenchmarkAcceptanceCheck {
            name: "ndcg_at_10".to_string(),
            passed: measured.ndcg_at_10 >= thresholds.min_ndcg_at_10,
            expected: format!(">= {:.2}", thresholds.min_ndcg_at_10),
            actual: format!("{:.4}", measured.ndcg_at_10),
        },
        BenchmarkAcceptanceCheck {
            name: "recall_at_10".to_string(),
            passed: measured.recall_at_10 >= thresholds.min_recall_at_10,
            expected: format!(">= {:.2}", thresholds.min_recall_at_10),
            actual: format!("{:.4}", measured.recall_at_10),
        },
        BenchmarkAcceptanceCheck {
            name: "query_total".to_string(),
            passed: measured.total_queries >= thresholds.min_total_queries,
            expected: format!(">= {}", thresholds.min_total_queries),
            actual: measured.total_queries.to_string(),
        },
        BenchmarkAcceptanceCheck {
            name: "query_semantic".to_string(),
            passed: measured.semantic_queries >= thresholds.min_semantic_queries,
            expected: format!(">= {}", thresholds.min_semantic_queries),
            actual: measured.semantic_queries.to_string(),
        },
        BenchmarkAcceptanceCheck {
            name: "query_lexical".to_string(),
            passed: measured.lexical_queries >= thresholds.min_lexical_queries,
            expected: format!(">= {}", thresholds.min_lexical_queries),
            actual: measured.lexical_queries.to_string(),
        },
        BenchmarkAcceptanceCheck {
            name: "query_mixed".to_string(),
            passed: measured.mixed_queries >= thresholds.min_mixed_queries,
            expected: format!(">= {}", thresholds.min_mixed_queries),
            actual: measured.mixed_queries.to_string(),
        },
    ];

    let passed = checks.iter().all(|check| check.passed);
    BenchmarkAcceptanceResult {
        protocol_id: "macmini-g6-v1".to_string(),
        passed,
        thresholds,
        measured,
        checks,
    }
}

fn shell_quote(input: &str) -> String {
    if input.is_empty() {
        return "''".to_string();
    }
    let mut out = String::with_capacity(input.len() + 2);
    out.push('\'');
    for ch in input.chars() {
        if ch == '\'' {
            out.push_str("'\"'\"'");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

pub fn build_eval_replay_command(case: &EvalQueryCase, search_limit: usize) -> String {
    let mut cmd = format!(
        "axiomme find {} --limit {}",
        shell_quote(&case.query),
        search_limit.max(1)
    );
    if let Some(target) = case.target_uri.as_deref() {
        cmd.push_str(" --target ");
        cmd.push_str(target);
    }
    cmd
}

pub fn classify_eval_bucket(
    case: &EvalQueryCase,
    actual_top_uri: Option<&str>,
    passed: bool,
) -> &'static str {
    if passed {
        return "pass";
    }
    if actual_top_uri.is_none() {
        return "no_results";
    }
    if case.expected_top_uri.is_none() {
        return "missing_expectation";
    }
    if let Some(target_raw) = case.target_uri.as_deref()
        && let Some(actual_raw) = actual_top_uri
        && let Ok(target_uri) = AxiomUri::parse(target_raw)
        && let Ok(actual_uri) = AxiomUri::parse(actual_raw)
        && !actual_uri.starts_with(&target_uri)
    {
        return "target_scope_mismatch";
    }
    "ranking_regression"
}

fn write_line(out: &mut String, args: std::fmt::Arguments<'_>) {
    let _ = out.write_fmt(args);
}

fn write_section_header(out: &mut String, title: &str) {
    write_line(out, format_args!("\n## {title}\n\n"));
}

pub fn format_eval_report_markdown(report: &EvalLoopReport) -> String {
    let mut out = String::new();
    out.push_str("# Eval Report\n\n");
    write_line(&mut out, format_args!("- run_id: `{}`\n", report.run_id));
    write_line(
        &mut out,
        format_args!("- created_at: `{}`\n", report.created_at),
    );
    write_line(
        &mut out,
        format_args!("- include_golden: `{}`\n", report.include_golden),
    );
    write_line(
        &mut out,
        format_args!("- golden_only: `{}`\n", report.golden_only),
    );
    write_line(
        &mut out,
        format_args!(
            "- executed_cases: `{}` (pass `{}`, fail `{}`)\n",
            report.executed_cases, report.passed, report.failed
        ),
    );
    write_line(
        &mut out,
        format_args!("- top1_accuracy: `{:.4}`\n", report.top1_accuracy),
    );
    write_line(
        &mut out,
        format_args!(
            "- trace_cases_used: `{}`, golden_cases_used: `{}`\n",
            report.trace_cases_used, report.golden_cases_used
        ),
    );
    write_line(
        &mut out,
        format_args!("- query_set_uri: `{}`\n", report.query_set_uri),
    );
    write_line(
        &mut out,
        format_args!("- report_uri: `{}`\n", report.report_uri),
    );

    write_section_header(&mut out, "Buckets");
    for bucket in &report.buckets {
        write_line(
            &mut out,
            format_args!("- {}: {}\n", bucket.name, bucket.count),
        );
    }

    write_section_header(&mut out, "Failures");
    if report.failures.is_empty() {
        out.push_str("- none\n");
    } else {
        for failure in report.failures.iter().take(20) {
            write_line(
                &mut out,
                format_args!(
                    "- [{}] query=`{}` expected=`{}` actual=`{}` source=`{}`\n",
                    failure.bucket,
                    failure.query,
                    failure.expected_top_uri.as_deref().unwrap_or("-"),
                    failure.actual_top_uri.as_deref().unwrap_or("-"),
                    failure.source
                ),
            );
            write_line(
                &mut out,
                format_args!("  replay: `{}`\n", failure.replay_command),
            );
        }
    }
    out
}

pub fn format_benchmark_report_markdown(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    write_benchmark_header(&mut out, report);
    write_benchmark_environment(&mut out, report);
    write_benchmark_corpus(&mut out, report);
    write_benchmark_query_set(&mut out, report);
    write_benchmark_acceptance(&mut out, report);
    write_benchmark_slowest_cases(&mut out, report);
    out
}

fn write_benchmark_header(out: &mut String, report: &BenchmarkReport) {
    out.push_str("# Benchmark Report\n\n");
    write_line(out, format_args!("- run_id: `{}`\n", report.run_id));
    write_line(out, format_args!("- created_at: `{}`\n", report.created_at));
    write_line(
        out,
        format_args!("- query_limit: `{}`\n", report.query_limit),
    );
    write_line(
        out,
        format_args!("- search_limit: `{}`\n", report.search_limit),
    );
    write_line(
        out,
        format_args!("- include_golden: `{}`\n", report.include_golden),
    );
    write_line(
        out,
        format_args!("- include_trace: `{}`\n", report.include_trace),
    );
    write_line(
        out,
        format_args!(
            "- executed_cases: `{}` (pass `{}`, fail `{}`)\n",
            report.executed_cases, report.passed, report.failed
        ),
    );
    write_line(
        out,
        format_args!("- error_rate: `{:.4}`\n", report.error_rate),
    );
    write_line(
        out,
        format_args!("- top1_accuracy: `{:.4}`\n", report.top1_accuracy),
    );
    write_line(out, format_args!("- ndcg@10: `{:.4}`\n", report.ndcg_at_10));
    write_line(
        out,
        format_args!("- recall@10: `{:.4}`\n", report.recall_at_10),
    );
    write_line(
        out,
        format_args!(
            "- find_latency_ms: p50=`{}`, p95=`{}`, p99=`{}`, avg=`{:.2}`\n",
            report.p50_latency_ms,
            report.p95_latency_ms,
            report.p99_latency_ms,
            report.avg_latency_ms
        ),
    );
    if let (Some(p50), Some(p95), Some(p99)) = (
        report.p50_latency_us,
        report.p95_latency_us,
        report.p99_latency_us,
    ) {
        write_line(
            out,
            format_args!(
                "- find_latency_us: p50=`{}`, p95=`{}`, p99=`{}`\n",
                p50, p95, p99
            ),
        );
    }
    write_line(
        out,
        format_args!(
            "- search_latency_ms: p50=`{}`, p95=`{}`, p99=`{}`, avg=`{:.2}`\n",
            report.search_p50_latency_ms,
            report.search_p95_latency_ms,
            report.search_p99_latency_ms,
            report.search_avg_latency_ms
        ),
    );
    if let (Some(p50), Some(p95), Some(p99)) = (
        report.search_p50_latency_us,
        report.search_p95_latency_us,
        report.search_p99_latency_us,
    ) {
        write_line(
            out,
            format_args!(
                "- search_latency_us: p50=`{}`, p95=`{}`, p99=`{}`\n",
                p50, p95, p99
            ),
        );
    }
    write_line(
        out,
        format_args!(
            "- commit_latency_ms: p50=`{}`, p95=`{}`, p99=`{}`, avg=`{:.2}`\n",
            report.commit_p50_latency_ms,
            report.commit_p95_latency_ms,
            report.commit_p99_latency_ms,
            report.commit_avg_latency_ms
        ),
    );
    if let (Some(p50), Some(p95), Some(p99)) = (
        report.commit_p50_latency_us,
        report.commit_p95_latency_us,
        report.commit_p99_latency_us,
    ) {
        write_line(
            out,
            format_args!(
                "- commit_latency_us: p50=`{}`, p95=`{}`, p99=`{}`\n",
                p50, p95, p99
            ),
        );
    }
    write_line(
        out,
        format_args!("- case_set_uri: `{}`\n", report.case_set_uri),
    );
    write_line(out, format_args!("- report_uri: `{}`\n", report.report_uri));
}

fn write_benchmark_environment(out: &mut String, report: &BenchmarkReport) {
    write_section_header(out, "Environment");
    write_line(
        out,
        format_args!(
            "- machine_profile: `{}`\n",
            report.environment.machine_profile
        ),
    );
    write_line(
        out,
        format_args!("- cpu_model: `{}`\n", report.environment.cpu_model),
    );
    write_line(
        out,
        format_args!("- ram_bytes: `{}`\n", report.environment.ram_bytes),
    );
    write_line(
        out,
        format_args!("- os_version: `{}`\n", report.environment.os_version),
    );
    write_line(
        out,
        format_args!("- rustc_version: `{}`\n", report.environment.rustc_version),
    );
    write_line(
        out,
        format_args!(
            "- retrieval_backend: `{}`\n",
            report.environment.retrieval_backend
        ),
    );
    write_line(
        out,
        format_args!(
            "- reranker_profile: `{}`\n",
            report.environment.reranker_profile
        ),
    );
}

fn write_benchmark_corpus(out: &mut String, report: &BenchmarkReport) {
    write_section_header(out, "Corpus");
    write_line(
        out,
        format_args!("- profile: `{}`\n", report.corpus.profile),
    );
    write_line(
        out,
        format_args!("- snapshot_id: `{}`\n", report.corpus.snapshot_id),
    );
    write_line(
        out,
        format_args!("- root_uri: `{}`\n", report.corpus.root_uri),
    );
    write_line(
        out,
        format_args!("- file_count: `{}`\n", report.corpus.file_count),
    );
    write_line(
        out,
        format_args!("- total_bytes: `{}`\n", report.corpus.total_bytes),
    );
}

fn write_benchmark_query_set(out: &mut String, report: &BenchmarkReport) {
    write_section_header(out, "Query Set");
    write_line(
        out,
        format_args!("- version: `{}`\n", report.query_set.version),
    );
    write_line(
        out,
        format_args!("- source: `{}`\n", report.query_set.source),
    );
    write_line(
        out,
        format_args!("- total_queries: `{}`\n", report.query_set.total_queries),
    );
    write_line(
        out,
        format_args!(
            "- semantic_queries: `{}`\n",
            report.query_set.semantic_queries
        ),
    );
    write_line(
        out,
        format_args!(
            "- lexical_queries: `{}`\n",
            report.query_set.lexical_queries
        ),
    );
    write_line(
        out,
        format_args!("- mixed_queries: `{}`\n", report.query_set.mixed_queries),
    );
    write_line(
        out,
        format_args!("- warmup_queries: `{}`\n", report.query_set.warmup_queries),
    );
    write_line(
        out,
        format_args!(
            "- measured_queries: `{}`\n",
            report.query_set.measured_queries
        ),
    );
}

fn write_benchmark_acceptance(out: &mut String, report: &BenchmarkReport) {
    write_section_header(out, "Acceptance Mapping");
    write_line(
        out,
        format_args!("- protocol_id: `{}`\n", report.acceptance.protocol_id),
    );
    write_line(
        out,
        format_args!("- passed: `{}`\n", report.acceptance.passed),
    );
    for check in &report.acceptance.checks {
        write_line(
            out,
            format_args!(
                "- {}: {} (expected `{}`, actual `{}`)\n",
                check.name,
                if check.passed { "pass" } else { "fail" },
                check.expected,
                check.actual
            ),
        );
    }
}

fn write_benchmark_slowest_cases(out: &mut String, report: &BenchmarkReport) {
    write_section_header(out, "Slowest Cases");
    let mut results = report.results.clone();
    results.sort_by(|a, b| b.latency_ms.cmp(&a.latency_ms));
    if results.is_empty() {
        out.push_str("- none\n");
        return;
    }

    for item in results.iter().take(20) {
        let rank = item
            .expected_rank
            .map_or_else(|| "-".to_string(), |value| value.to_string());
        let latency = item.latency_us.map_or_else(
            || format!("{}ms", item.latency_ms),
            |us| format!("{}ms/{}us", item.latency_ms, us),
        );
        write_line(
            out,
            format_args!(
                "- latency={} pass={} source={} rank={} query=`{}` expected=`{}` actual=`{}`\n",
                latency,
                item.passed,
                item.source,
                rank,
                item.query,
                item.expected_top_uri.as_deref().unwrap_or("-"),
                item.actual_top_uri.as_deref().unwrap_or("-")
            ),
        );
    }
}

pub fn to_benchmark_summary(report: BenchmarkReport) -> BenchmarkSummary {
    BenchmarkSummary {
        run_id: report.run_id,
        created_at: report.created_at,
        executed_cases: report.executed_cases,
        top1_accuracy: report.top1_accuracy,
        p95_latency_ms: report.p95_latency_ms,
        p95_latency_us: report.p95_latency_us,
        report_uri: report.report_uri,
    }
}

pub fn to_trace_metrics_snapshot_summary(
    doc: &TraceMetricsSnapshotDocument,
    report_uri: &str,
) -> TraceMetricsSnapshotSummary {
    TraceMetricsSnapshotSummary {
        snapshot_id: doc.snapshot_id.clone(),
        created_at: doc.created_at.clone(),
        report_uri: report_uri.to_string(),
        traces_analyzed: doc.report.traces_analyzed,
        include_replays: doc.report.include_replays,
        window_limit: doc.report.window_limit,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn case(
        query: &str,
        target_uri: Option<&str>,
        expected_top_uri: Option<&str>,
    ) -> EvalQueryCase {
        EvalQueryCase {
            source_trace_id: "trace-1".to_string(),
            query: query.to_string(),
            target_uri: target_uri.map(ToString::to_string),
            expected_top_uri: expected_top_uri.map(ToString::to_string),
            source: "generated".to_string(),
        }
    }

    fn case_with_source(query: &str, source: &str) -> EvalQueryCase {
        EvalQueryCase {
            source_trace_id: "trace-1".to_string(),
            query: query.to_string(),
            target_uri: None,
            expected_top_uri: None,
            source: source.to_string(),
        }
    }

    #[test]
    fn benchmark_query_set_metadata_tracks_query_mix() {
        let cases = vec![
            case("how to configure oauth refresh token flow", None, None),
            case("oauth.rs::refresh_token_v2", None, None),
            case("oauth refresh token error 401", None, None),
        ];

        let metadata = build_benchmark_query_set_metadata(&cases, Some("fixture-a"));
        assert_eq!(metadata.total_queries, 3);
        assert_eq!(metadata.semantic_queries, 1);
        assert_eq!(metadata.lexical_queries, 1);
        assert_eq!(metadata.mixed_queries, 1);
        assert_eq!(metadata.warmup_queries, 3);
        assert_eq!(metadata.measured_queries, 0);
        assert_eq!(metadata.source, "fixture:fixture-a");
    }

    #[test]
    fn benchmark_query_set_metadata_infers_generated_sources() {
        let cases = vec![
            case_with_source("oauth refresh guide", "golden"),
            case_with_source("oauth refresh gide", "stress:typo"),
            case_with_source("oauth", "trace-unlabeled"),
        ];
        let metadata = build_benchmark_query_set_metadata(&cases, None);
        assert_eq!(metadata.source, "generated:golden+stress+trace");
    }

    #[test]
    fn eval_replay_command_shell_quotes_query() {
        let eval_case = case(
            "user's oauth token refresh",
            Some("axiom://resources/oauth"),
            None,
        );
        let command = build_eval_replay_command(&eval_case, 10);
        assert_eq!(
            command,
            "axiomme find 'user'\"'\"'s oauth token refresh' --limit 10 --target axiom://resources/oauth"
        );
    }

    #[test]
    fn classify_eval_bucket_detects_target_scope_mismatch() {
        let eval_case = case(
            "oauth refresh failure",
            Some("axiom://resources/auth"),
            Some("axiom://resources/auth/node.l1.md"),
        );
        let bucket = classify_eval_bucket(
            &eval_case,
            Some("axiom://resources/infra/network/node.l1.md"),
            false,
        );
        assert_eq!(bucket, "target_scope_mismatch");
    }

    #[test]
    fn benchmark_acceptance_result_fails_when_thresholds_not_met() {
        let query_set = BenchmarkQuerySetMetadata {
            version: "qset-v1".to_string(),
            source: "generated:golden+trace".to_string(),
            total_queries: 10,
            semantic_queries: 1,
            lexical_queries: 1,
            mixed_queries: 1,
            warmup_queries: 5,
            measured_queries: 5,
        };
        let result = build_benchmark_acceptance_result(700, 1300, 1700, 0.6, 0.7, &query_set);

        assert!(!result.passed);
        assert!(result.checks.iter().any(|check| !check.passed));
    }

    #[test]
    fn duration_to_latency_ms_uses_non_zero_ceil_rounding() {
        assert_eq!(duration_to_latency_ms(Duration::ZERO), 0);
        assert_eq!(duration_to_latency_ms(Duration::from_nanos(1)), 1);
        assert_eq!(duration_to_latency_ms(Duration::from_micros(999)), 1);
        assert_eq!(duration_to_latency_ms(Duration::from_micros(1_000)), 1);
        assert_eq!(duration_to_latency_ms(Duration::from_micros(1_001)), 2);
    }

    #[test]
    fn duration_to_latency_us_uses_non_zero_ceil_rounding() {
        assert_eq!(duration_to_latency_us(Duration::ZERO), 0);
        assert_eq!(duration_to_latency_us(Duration::from_nanos(1)), 1);
        assert_eq!(duration_to_latency_us(Duration::from_nanos(999)), 1);
        assert_eq!(duration_to_latency_us(Duration::from_nanos(1_000)), 1);
        assert_eq!(duration_to_latency_us(Duration::from_nanos(1_001)), 2);
    }
}
