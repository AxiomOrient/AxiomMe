use std::collections::HashSet;

use chrono::Utc;

use crate::catalog::{
    benchmark_fixture_uri, eval_case_key, eval_case_ordering, eval_golden_uri,
    normalize_eval_case_source, parse_benchmark_fixture_document,
};
use crate::error::{AxiomError, Result};
use crate::models::{
    BenchmarkRunOptions, EvalGoldenDocument, EvalQueryCase, FindResult, RetrievalTrace,
    SearchOptions, TraceIndexEntry,
};
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

impl AxiomMe {
    pub(super) fn persist_trace_result(&self, result: &mut FindResult) -> Result<()> {
        let Some(trace) = result.trace.as_ref() else {
            result.trace_uri = None;
            return Ok(());
        };

        let trace_uri = self.persist_trace(trace)?;
        result.trace_uri = Some(trace_uri);
        Ok(())
    }

    pub(super) fn persist_trace(&self, trace: &RetrievalTrace) -> Result<String> {
        let trace_uri = AxiomUri::root(Scope::Queue)
            .join("traces")?
            .join(&format!("{}.json", trace.trace_id))?;
        let serialized = serde_json::to_string_pretty(trace)?;
        self.fs.write(&trace_uri, &serialized, true)?;

        self.state.upsert_trace_index(&TraceIndexEntry {
            trace_id: trace.trace_id.clone(),
            uri: trace_uri.to_string(),
            request_type: trace.request_type.clone(),
            query: trace.query.clone(),
            target_uri: trace.target_uri.clone(),
            created_at: Utc::now().to_rfc3339(),
        })?;

        Ok(trace_uri.to_string())
    }

    pub(super) fn collect_trace_eval_cases(
        &self,
        trace_limit: usize,
    ) -> Result<(Vec<EvalQueryCase>, usize)> {
        let trace_entries = self.list_traces(trace_limit)?;
        let traces_scanned = trace_entries.len();
        let mut cases = Vec::<EvalQueryCase>::new();

        for entry in trace_entries {
            let Some(trace) = self.get_trace(&entry.trace_id)? else {
                continue;
            };
            cases.push(EvalQueryCase {
                source_trace_id: trace.trace_id,
                query: trace.query,
                target_uri: trace.target_uri,
                expected_top_uri: trace.final_topk.first().map(|x| x.uri.clone()),
                source: "trace".to_string(),
            });
        }

        Ok((cases, traces_scanned))
    }

    pub(super) fn collect_benchmark_query_cases(
        &self,
        options: &BenchmarkRunOptions,
        query_limit: usize,
    ) -> Result<Vec<EvalQueryCase>> {
        if let Some(fixture_name) = options.fixture_name.as_deref() {
            let fixture_uri = benchmark_fixture_uri(fixture_name)?;
            let raw = self.fs.read(&fixture_uri)?;
            let mut doc = parse_benchmark_fixture_document(&raw)?;
            for case in &mut doc.cases {
                if case.source.trim().is_empty() {
                    case.source = "fixture".to_string();
                }
            }
            doc.cases.sort_by(eval_case_ordering);
            doc.cases.truncate(query_limit.max(1));
            return Ok(doc.cases);
        }

        let mut seen = HashSet::<(String, Option<String>)>::new();
        let mut query_cases = Vec::<EvalQueryCase>::new();
        if options.include_golden {
            for mut case in self.list_eval_golden_queries()? {
                if query_cases.len() >= query_limit {
                    break;
                }
                normalize_eval_case_source(&mut case, "golden");
                if !seen.insert(eval_case_key(&case)) {
                    continue;
                }
                query_cases.push(case);
            }
        }
        if options.include_trace && query_cases.len() < query_limit {
            let trace_limit = query_limit.saturating_mul(4).max(query_limit);
            let (trace_cases, _) = self.collect_trace_eval_cases(trace_limit)?;
            for mut case in trace_cases {
                if query_cases.len() >= query_limit {
                    break;
                }
                normalize_eval_case_source(&mut case, "trace");
                if !seen.insert(eval_case_key(&case)) {
                    continue;
                }
                query_cases.push(case);
            }
        }
        Ok(query_cases)
    }

    pub(super) fn eval_top_result_uri(
        &self,
        query: &str,
        target_uri: Option<&str>,
        search_limit: usize,
    ) -> Result<Option<String>> {
        let uris = self.eval_result_uris(query, target_uri, search_limit, "eval")?;
        Ok(uris.first().cloned())
    }

    pub(super) fn eval_result_uris(
        &self,
        query: &str,
        target_uri: Option<&str>,
        search_limit: usize,
        request_type: &str,
    ) -> Result<Vec<String>> {
        let target = target_uri.map(AxiomUri::parse).transpose()?;
        let options = SearchOptions {
            query: query.to_string(),
            target_uri: target,
            session: None,
            session_hints: Vec::new(),
            budget: None,
            limit: search_limit.max(1),
            score_threshold: None,
            filter: None,
            request_type: request_type.to_string(),
        };
        let index = self
            .index
            .read()
            .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
        let result = self.drr.run(&index, options);
        Ok(result.query_results.into_iter().map(|x| x.uri).collect())
    }

    pub(super) fn persist_eval_golden_queries(&self, cases: &[EvalQueryCase]) -> Result<String> {
        let golden_uri = eval_golden_uri()?;
        let document = EvalGoldenDocument {
            version: 1,
            updated_at: Utc::now().to_rfc3339(),
            cases: cases.to_vec(),
        };
        self.fs
            .write(&golden_uri, &serde_json::to_string_pretty(&document)?, true)?;
        Ok(golden_uri.to_string())
    }
}
