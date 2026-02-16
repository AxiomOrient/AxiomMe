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

const TRACE_CASE_EXPANSION_FACTOR: usize = 4;

impl AxiomMe {
    pub(crate) fn persist_trace_result(&self, result: &mut FindResult) -> Result<()> {
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

    pub(crate) fn collect_trace_eval_cases(
        &self,
        trace_limit: usize,
    ) -> Result<(Vec<EvalQueryCase>, usize)> {
        self.collect_trace_eval_cases_with_expectations(trace_limit, true)
    }

    pub(crate) fn collect_trace_eval_cases_with_expectations(
        &self,
        trace_limit: usize,
        include_expectations: bool,
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
                expected_top_uri: if include_expectations {
                    trace.final_topk.first().map(|x| x.uri.clone())
                } else {
                    None
                },
                source: if include_expectations {
                    "trace".to_string()
                } else {
                    "trace-unlabeled".to_string()
                },
            });
        }

        Ok((cases, traces_scanned))
    }

    pub(crate) fn collect_benchmark_query_cases(
        &self,
        options: &BenchmarkRunOptions,
        query_limit: usize,
    ) -> Result<Vec<EvalQueryCase>> {
        let query_limit = query_limit.max(1);
        if let Some(fixture_name) = options.fixture_name.as_deref() {
            let fixture_uri = benchmark_fixture_uri(fixture_name)?;
            let raw = self.fs.read(&fixture_uri)?;
            let mut doc = parse_benchmark_fixture_document(&raw)?;
            for case in &mut doc.cases {
                if case.source.trim().is_empty() {
                    case.source = "fixture".to_string();
                }
            }
            if options.include_stress {
                let stress_cases = build_stress_cases(&doc.cases, query_limit);
                doc.cases.extend(stress_cases);
            }
            doc.cases.sort_by(eval_case_ordering);
            doc.cases.truncate(query_limit);
            return Ok(doc.cases);
        }

        let mut seen = HashSet::<(String, Option<String>)>::new();
        let mut query_cases = Vec::<EvalQueryCase>::new();
        let mut golden_seed_cases = Vec::<EvalQueryCase>::new();
        if options.include_golden {
            for mut case in self.list_eval_golden_queries()? {
                if query_cases.len() >= query_limit {
                    break;
                }
                normalize_eval_case_source(&mut case, "golden");
                if !seen.insert(eval_case_key(&case)) {
                    continue;
                }
                golden_seed_cases.push(case.clone());
                query_cases.push(case);
            }
        }
        if options.include_stress && query_cases.len() < query_limit {
            let stress_cases = build_stress_cases(
                &golden_seed_cases,
                query_limit.saturating_sub(query_cases.len()),
            );
            for mut case in stress_cases {
                if query_cases.len() >= query_limit {
                    break;
                }
                normalize_eval_case_source(&mut case, "stress");
                if !seen.insert(eval_case_key(&case)) {
                    continue;
                }
                query_cases.push(case);
            }
        }
        if options.include_trace && query_cases.len() < query_limit {
            let trace_limit = query_limit
                .saturating_mul(TRACE_CASE_EXPANSION_FACTOR)
                .max(query_limit);
            let (trace_cases, _) = self.collect_trace_eval_cases_with_expectations(
                trace_limit,
                options.trace_expectations,
            )?;
            for mut case in trace_cases {
                if query_cases.len() >= query_limit {
                    break;
                }
                let fallback = if options.trace_expectations {
                    "trace"
                } else {
                    "trace-unlabeled"
                };
                normalize_eval_case_source(&mut case, fallback);
                if !seen.insert(eval_case_key(&case)) {
                    continue;
                }
                query_cases.push(case);
            }
        }
        Ok(query_cases)
    }

    pub(crate) fn eval_top_result_uri(
        &self,
        query: &str,
        target_uri: Option<&str>,
        search_limit: usize,
    ) -> Result<Option<String>> {
        let uris = self.eval_result_uris(query, target_uri, search_limit, "eval")?;
        Ok(uris.first().cloned())
    }

    pub(crate) fn eval_result_uris(
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
            min_match_tokens: None,
            filter: None,
            request_type: request_type.to_string(),
        };
        let result = {
            let index = self
                .index
                .read()
                .map_err(|_| AxiomError::lock_poisoned("index"))?;
            self.drr.run(&index, &options)
        };
        Ok(result.query_results.into_iter().map(|x| x.uri).collect())
    }

    pub(crate) fn persist_eval_golden_queries(&self, cases: &[EvalQueryCase]) -> Result<String> {
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

fn build_stress_cases(seed_cases: &[EvalQueryCase], limit: usize) -> Vec<EvalQueryCase> {
    if seed_cases.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut out = Vec::<EvalQueryCase>::new();
    let mut seen = HashSet::<(String, Option<String>)>::new();
    for seed in seed_cases {
        for (variant_name, query) in build_stress_query_variants(&seed.query) {
            if out.len() >= limit {
                return out;
            }
            let case = EvalQueryCase {
                source_trace_id: seed.source_trace_id.clone(),
                query,
                target_uri: seed.target_uri.clone(),
                expected_top_uri: seed.expected_top_uri.clone(),
                source: format!("stress:{variant_name}"),
            };
            if !seen.insert(eval_case_key(&case)) {
                continue;
            }
            out.push(case);
        }
    }
    out
}

fn build_stress_query_variants(query: &str) -> Vec<(&'static str, String)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut variants = Vec::<(&'static str, String)>::new();
    if let Some(variant) = stress_typo_variant(trimmed) {
        variants.push(("typo", variant));
    }
    if let Some(variant) = stress_alias_variant(trimmed) {
        variants.push(("alias", variant));
    }
    if let Some(variant) = stress_paraphrase_variant(trimmed) {
        variants.push(("paraphrase", variant));
    }

    let mut unique = HashSet::<String>::new();
    let mut out = Vec::<(&'static str, String)>::new();
    for (name, variant) in variants {
        if variant.trim().is_empty() || variant.eq_ignore_ascii_case(trimmed) {
            continue;
        }
        let key = variant.trim().to_lowercase();
        if unique.insert(key) {
            out.push((name, variant));
        }
    }
    out
}

fn stress_typo_variant(query: &str) -> Option<String> {
    let mut tokens = query
        .split_whitespace()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let idx = tokens
        .iter()
        .enumerate()
        .max_by_key(|(_, token)| token.chars().count())
        .map(|(idx, _)| idx)?;

    let chars = tokens[idx].chars().collect::<Vec<_>>();
    if chars.len() < 5 {
        return None;
    }
    let mut typo = chars;
    typo.remove(typo.len() / 2);
    tokens[idx] = typo.into_iter().collect::<String>();
    Some(tokens.join(" "))
}

fn stress_alias_variant(query: &str) -> Option<String> {
    let mut changed = false;
    let tokens = query
        .to_lowercase()
        .split_whitespace()
        .map(|token| {
            let normalized = token.trim_matches(|c: char| !c.is_alphanumeric());
            let replacement = match normalized {
                "auth" | "oauth" | "authentication" | "authorization" | "login" | "signin" => {
                    Some("인증")
                }
                "token" => Some("토큰"),
                "session" => Some("세션"),
                "memory" => Some("메모리"),
                "queue" => Some("큐"),
                "benchmark" => Some("벤치마크"),
                "인증" => Some("auth"),
                "토큰" => Some("token"),
                "세션" => Some("session"),
                "메모리" => Some("memory"),
                "큐" => Some("queue"),
                "벤치마크" => Some("benchmark"),
                _ => None,
            };
            replacement.map_or_else(
                || token.to_string(),
                |alias| {
                    changed = true;
                    alias.to_string()
                },
            )
        })
        .collect::<Vec<_>>();

    if changed {
        Some(tokens.join(" "))
    } else {
        None
    }
}

fn stress_paraphrase_variant(query: &str) -> Option<String> {
    let lower = query.trim().to_ascii_lowercase();
    if lower.starts_with("how to ") {
        None
    } else if query.split_whitespace().count() >= 2 {
        Some(format!("how to {query}"))
    } else {
        None
    }
}
