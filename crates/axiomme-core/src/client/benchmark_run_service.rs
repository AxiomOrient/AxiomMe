use std::time::Instant;

use chrono::Utc;

use crate::catalog::{benchmark_report_json_uri, benchmark_report_markdown_uri};
use crate::error::{AxiomError, Result};
use crate::models::{BenchmarkCaseResult, BenchmarkReport, BenchmarkRunOptions, EvalQueryCase};
use crate::quality::{build_benchmark_acceptance_result, build_benchmark_query_set_metadata};

use super::{
    AxiomMe,
    benchmark_logging_service::BenchmarkRunLogContext,
    benchmark_metrics_service::{safe_ratio, safe_ratio_f32, summarize_latencies},
};

struct BenchmarkCaseMeasurement {
    result: BenchmarkCaseResult,
    find_latency_ms: u128,
    search_latency_ms: u128,
    has_expectation: bool,
    recall_hit: bool,
    ndcg_gain: f32,
}

#[derive(Default)]
struct BenchmarkEvaluation {
    results: Vec<BenchmarkCaseResult>,
    find_latencies: Vec<u128>,
    search_latencies: Vec<u128>,
    passed: usize,
    failed: usize,
    graded_cases: usize,
    recall_hits: usize,
    ndcg_total: f32,
}

impl AxiomMe {
    pub fn run_benchmark_suite(&self, options: BenchmarkRunOptions) -> Result<BenchmarkReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let query_limit = options.query_limit.max(1);
        let search_limit = options.search_limit.max(1);
        let include_golden = options.include_golden;
        let include_trace = options.include_trace;
        let fixture_name = options.fixture_name.clone();
        let run_id = uuid::Uuid::new_v4().to_string();
        let log_context = BenchmarkRunLogContext {
            run_id: run_id.clone(),
            query_limit,
            search_limit,
            include_golden,
            include_trace,
            fixture_name: fixture_name.clone(),
        };

        let output = (|| -> Result<BenchmarkReport> {
            if options.fixture_name.is_none() && !options.include_golden && !options.include_trace {
                return Err(AxiomError::Validation(
                    "benchmark must include at least one source (golden or trace)".to_string(),
                ));
            }

            let created_at = Utc::now().to_rfc3339();
            let query_cases = self.collect_benchmark_query_cases(&options, query_limit)?;
            let query_set =
                build_benchmark_query_set_metadata(&query_cases, options.fixture_name.as_deref());
            let environment = self.collect_benchmark_environment_metadata();
            let corpus = self.collect_benchmark_corpus_metadata()?;

            let case_set_uri = self.write_benchmark_case_set(&run_id, &query_cases)?;

            let evaluation = self.evaluate_benchmark_cases(&query_cases, search_limit)?;
            let find_summary = summarize_latencies(&evaluation.find_latencies);
            let search_summary = summarize_latencies(&evaluation.search_latencies);

            let commit_latencies = self.measure_benchmark_commit_latencies(5)?;
            let commit_summary = summarize_latencies(&commit_latencies);

            let executed_cases = evaluation.results.len();
            let top1_accuracy = safe_ratio(evaluation.passed, executed_cases);
            let ndcg_at_10 = safe_ratio_f32(evaluation.ndcg_total, evaluation.graded_cases);
            let recall_at_10 = safe_ratio(evaluation.recall_hits, evaluation.graded_cases);
            let error_rate = safe_ratio(evaluation.failed, executed_cases);
            let acceptance = build_benchmark_acceptance_result(
                find_summary.p95,
                search_summary.p95,
                commit_summary.p95,
                ndcg_at_10,
                recall_at_10,
                &query_set,
            );

            let report_uri = benchmark_report_json_uri(&run_id)?;
            let markdown_report_uri = benchmark_report_markdown_uri(&run_id)?;
            let report = BenchmarkReport {
                run_id: run_id.clone(),
                created_at,
                query_limit,
                search_limit,
                include_golden,
                include_trace,
                executed_cases,
                passed: evaluation.passed,
                failed: evaluation.failed,
                top1_accuracy,
                ndcg_at_10,
                recall_at_10,
                p50_latency_ms: find_summary.p50,
                p95_latency_ms: find_summary.p95,
                p99_latency_ms: find_summary.p99,
                avg_latency_ms: find_summary.avg,
                search_p50_latency_ms: search_summary.p50,
                search_p95_latency_ms: search_summary.p95,
                search_p99_latency_ms: search_summary.p99,
                search_avg_latency_ms: search_summary.avg,
                commit_p50_latency_ms: commit_summary.p50,
                commit_p95_latency_ms: commit_summary.p95,
                commit_p99_latency_ms: commit_summary.p99,
                commit_avg_latency_ms: commit_summary.avg,
                error_rate,
                environment,
                corpus,
                query_set,
                acceptance,
                report_uri: report_uri.to_string(),
                markdown_report_uri: markdown_report_uri.to_string(),
                case_set_uri,
                results: evaluation.results,
            };
            self.write_benchmark_report_artifacts(&report)?;

            Ok(report)
        })();

        match output {
            Ok(report) => {
                self.log_benchmark_run_success(request_id, started, fixture_name, &report);
                Ok(report)
            }
            Err(err) => {
                self.log_benchmark_run_error(request_id, started, log_context, &err);
                Err(err)
            }
        }
    }

    fn evaluate_benchmark_cases(
        &self,
        query_cases: &[EvalQueryCase],
        search_limit: usize,
    ) -> Result<BenchmarkEvaluation> {
        let mut evaluation = BenchmarkEvaluation::default();
        for case in query_cases {
            let measurement = self.measure_benchmark_case(case, search_limit)?;
            evaluation.find_latencies.push(measurement.find_latency_ms);
            evaluation
                .search_latencies
                .push(measurement.search_latency_ms);
            if measurement.result.passed {
                evaluation.passed += 1;
            } else {
                evaluation.failed += 1;
            }
            if measurement.has_expectation {
                evaluation.graded_cases += 1;
                if measurement.recall_hit {
                    evaluation.recall_hits += 1;
                    evaluation.ndcg_total += measurement.ndcg_gain;
                }
            }
            evaluation.results.push(measurement.result);
        }
        Ok(evaluation)
    }

    fn measure_benchmark_case(
        &self,
        case: &EvalQueryCase,
        search_limit: usize,
    ) -> Result<BenchmarkCaseMeasurement> {
        let started_find = Instant::now();
        let find_uris = self.eval_result_uris(
            &case.query,
            case.target_uri.as_deref(),
            search_limit,
            "benchmark_find",
        )?;
        let find_latency_ms = started_find.elapsed().as_millis();

        let started_search = Instant::now();
        let _ = self.eval_result_uris(
            &case.query,
            case.target_uri.as_deref(),
            search_limit,
            "benchmark_search",
        )?;
        let search_latency_ms = started_search.elapsed().as_millis();

        let actual_top_uri = find_uris.first().cloned();
        let expected_rank = case
            .expected_top_uri
            .as_ref()
            .and_then(|expected| find_uris.iter().position(|uri| uri == expected))
            .map(|idx| idx + 1);
        let case_passed = case.expected_top_uri.is_some() && expected_rank == Some(1);
        let has_expectation = case.expected_top_uri.is_some();
        let recall_hit = matches!(expected_rank, Some(rank) if rank <= 10);
        let ndcg_gain = if let Some(rank) = expected_rank {
            if rank <= 10 {
                1.0 / ((rank as f32 + 1.0).log2())
            } else {
                0.0
            }
        } else {
            0.0
        };

        Ok(BenchmarkCaseMeasurement {
            result: BenchmarkCaseResult {
                query: case.query.clone(),
                target_uri: case.target_uri.clone(),
                expected_top_uri: case.expected_top_uri.clone(),
                actual_top_uri,
                expected_rank,
                latency_ms: find_latency_ms,
                passed: case_passed,
                source: case.source.clone(),
            },
            find_latency_ms,
            search_latency_ms,
            has_expectation,
            recall_hit,
            ndcg_gain,
        })
    }
}
