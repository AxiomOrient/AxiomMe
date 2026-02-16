use std::time::Instant;

use chrono::Utc;

use crate::catalog::{benchmark_report_json_uri, benchmark_report_markdown_uri};
use crate::error::{AxiomError, Result};
use crate::models::{
    BenchmarkAmortizedReport, BenchmarkAmortizedRunSummary, BenchmarkCaseResult, BenchmarkReport,
    BenchmarkRunOptions, EvalQueryCase,
};
use crate::quality::{build_benchmark_acceptance_result, build_benchmark_query_set_metadata};
use crate::uri::uri_equivalent;

use super::{
    AxiomMe,
    logging_service::BenchmarkRunLogContext,
    metrics_service::{safe_ratio, safe_ratio_f32, safe_ratio_u128, summarize_latencies},
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

#[derive(Debug, Clone)]
struct BenchmarkRunContext {
    run_id: String,
    query_limit: usize,
    search_limit: usize,
    include_golden: bool,
    include_trace: bool,
}

impl AxiomMe {
    pub fn run_benchmark_suite_amortized(
        &self,
        options: BenchmarkRunOptions,
        iterations: usize,
    ) -> Result<BenchmarkAmortizedReport> {
        let iterations = iterations.max(1);
        let wall_started = Instant::now();
        let mut runs = Vec::<BenchmarkAmortizedRunSummary>::with_capacity(iterations);
        let mut p95_samples = Vec::<u128>::with_capacity(iterations);
        let mut executed_cases_total = 0usize;
        let mut top1_total = 0.0f32;
        let mut ndcg_total = 0.0f32;
        let mut recall_total = 0.0f32;

        for iteration in 0..iterations {
            let report = self.run_benchmark_suite(&options)?;
            executed_cases_total += report.executed_cases;
            top1_total += report.top1_accuracy;
            ndcg_total += report.ndcg_at_10;
            recall_total += report.recall_at_10;
            p95_samples.push(report.p95_latency_ms);
            runs.push(BenchmarkAmortizedRunSummary {
                iteration: iteration + 1,
                run_id: report.run_id,
                created_at: report.created_at,
                executed_cases: report.executed_cases,
                top1_accuracy: report.top1_accuracy,
                ndcg_at_10: report.ndcg_at_10,
                recall_at_10: report.recall_at_10,
                p95_latency_ms: report.p95_latency_ms,
                report_uri: report.report_uri,
            });
        }

        p95_samples.sort_unstable();
        let median_idx = p95_samples.len() / 2;
        let p95_latency_ms_median = p95_samples.get(median_idx).copied().unwrap_or(0);
        let wall_total_ms = wall_started.elapsed().as_millis();

        Ok(BenchmarkAmortizedReport {
            mode: "in_process_amortized".to_string(),
            iterations,
            query_limit: options.query_limit,
            search_limit: options.search_limit,
            include_golden: options.include_golden,
            include_trace: options.include_trace,
            include_stress: options.include_stress,
            trace_expectations: options.trace_expectations,
            fixture_name: options.fixture_name,
            wall_total_ms,
            wall_avg_ms: safe_ratio_u128(wall_total_ms, iterations),
            executed_cases_total,
            top1_accuracy_avg: safe_ratio_f32(top1_total, iterations),
            ndcg_at_10_avg: safe_ratio_f32(ndcg_total, iterations),
            recall_at_10_avg: safe_ratio_f32(recall_total, iterations),
            p95_latency_ms_median,
            runs,
        })
    }

    pub fn run_benchmark_suite(&self, options: &BenchmarkRunOptions) -> Result<BenchmarkReport> {
        crate::embedding::clear_embedding_strict_error();
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let query_limit = options.query_limit.max(1);
        let search_limit = options.search_limit.max(1);
        let include_golden = options.include_golden;
        let include_trace = options.include_trace;
        let fixture_name = options.fixture_name.clone();
        let run = BenchmarkRunContext {
            run_id: uuid::Uuid::new_v4().to_string(),
            query_limit,
            search_limit,
            include_golden,
            include_trace,
        };
        let log_context = BenchmarkRunLogContext {
            run_id: run.run_id.clone(),
            query_limit,
            search_limit,
            include_golden,
            include_trace,
            fixture_name: fixture_name.clone(),
        };

        let output = self.execute_benchmark_suite(options, &run);

        match output {
            Ok(report) => {
                self.log_benchmark_run_success(
                    request_id.as_str(),
                    started,
                    fixture_name.as_deref(),
                    &report,
                );
                Ok(report)
            }
            Err(err) => {
                self.log_benchmark_run_error(request_id.as_str(), started, &log_context, &err);
                Err(err)
            }
        }
    }

    fn execute_benchmark_suite(
        &self,
        options: &BenchmarkRunOptions,
        run: &BenchmarkRunContext,
    ) -> Result<BenchmarkReport> {
        if options.fixture_name.is_none() && !options.include_golden && !options.include_trace {
            return Err(AxiomError::Validation(
                "benchmark must include at least one source (golden or trace)".to_string(),
            ));
        }

        let created_at = Utc::now().to_rfc3339();
        let query_cases = self.collect_benchmark_query_cases(options, run.query_limit)?;
        let query_set =
            build_benchmark_query_set_metadata(&query_cases, options.fixture_name.as_deref());
        let corpus = self.collect_benchmark_corpus_metadata()?;

        let case_set_uri = self.write_benchmark_case_set(&run.run_id, &query_cases)?;
        let evaluation = self.evaluate_benchmark_cases(&query_cases, run.search_limit)?;
        let find_summary = summarize_latencies(&evaluation.find_latencies);
        let search_summary = summarize_latencies(&evaluation.search_latencies);

        let commit_latencies = self.measure_benchmark_commit_latencies(5)?;
        let commit_summary = summarize_latencies(&commit_latencies);
        let environment = self.collect_benchmark_environment_metadata();

        let executed_cases = evaluation.results.len();
        let top1_accuracy = safe_ratio(evaluation.passed, evaluation.graded_cases);
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

        let report_uri = benchmark_report_json_uri(&run.run_id)?;
        let markdown_report_uri = benchmark_report_markdown_uri(&run.run_id)?;
        let report = BenchmarkReport {
            run_id: run.run_id.clone(),
            created_at,
            query_limit: run.query_limit,
            search_limit: run.search_limit,
            include_golden: run.include_golden,
            include_trace: run.include_trace,
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
            .and_then(|expected| {
                find_uris
                    .iter()
                    .position(|candidate| uri_equivalent(expected, candidate))
            })
            .map(|idx| idx + 1);
        let case_passed = case.expected_top_uri.is_some() && expected_rank == Some(1);
        let has_expectation = case.expected_top_uri.is_some();
        let recall_hit = matches!(expected_rank, Some(rank) if rank <= 10);
        let ndcg_gain = expected_rank.map_or(0.0, |rank| {
            if rank <= 10 {
                let rank_plus_one = u16::try_from(rank.saturating_add(1)).unwrap_or(u16::MAX);
                1.0 / f32::from(rank_plus_one).log2()
            } else {
                0.0
            }
        });

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
