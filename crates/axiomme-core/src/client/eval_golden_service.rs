use std::collections::HashSet;

use crate::catalog::{
    eval_case_key, eval_case_ordering, eval_golden_uri, normalize_eval_case_source,
    parse_golden_cases_document,
};
use crate::error::{AxiomError, Result};
use crate::models::{EvalGoldenAddResult, EvalGoldenMergeReport, EvalQueryCase};
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub fn list_eval_golden_queries(&self) -> Result<Vec<EvalQueryCase>> {
        let uri = eval_golden_uri()?;
        let raw = match self.fs.read(&uri) {
            Ok(raw) => raw,
            Err(AxiomError::NotFound(_)) => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };

        let mut cases = parse_golden_cases_document(&raw)?;
        for case in &mut cases {
            normalize_eval_case_source(case, "golden");
        }
        cases.sort_by(eval_case_ordering);
        Ok(cases)
    }

    pub fn add_eval_golden_query(
        &self,
        query: &str,
        target_uri: Option<&str>,
        expected_top_uri: Option<&str>,
    ) -> Result<EvalGoldenAddResult> {
        let query = query.trim();
        if query.is_empty() {
            return Err(AxiomError::Validation(
                "golden query cannot be empty".to_string(),
            ));
        }

        let target_uri = target_uri
            .map(AxiomUri::parse)
            .transpose()?
            .map(|uri| uri.to_string());
        let expected_top_uri = expected_top_uri
            .map(AxiomUri::parse)
            .transpose()?
            .map(|uri| uri.to_string());

        let mut cases = self.list_eval_golden_queries()?;
        let key = (query.to_lowercase(), target_uri.clone());
        let mut added = true;
        if let Some(existing) = cases.iter_mut().find(|case| eval_case_key(case) == key) {
            if expected_top_uri.is_some() {
                existing.expected_top_uri = expected_top_uri;
            }
            existing.source_trace_id = "golden-manual".to_string();
            existing.source = "golden".to_string();
            added = false;
        } else {
            cases.push(EvalQueryCase {
                source_trace_id: "golden-manual".to_string(),
                query: query.to_string(),
                target_uri,
                expected_top_uri,
                source: "golden".to_string(),
            });
        }
        cases.sort_by(eval_case_ordering);
        let golden_uri = self.persist_eval_golden_queries(&cases)?;
        Ok(EvalGoldenAddResult {
            golden_uri,
            added,
            count: cases.len(),
        })
    }

    pub fn merge_eval_golden_from_traces(
        &self,
        trace_limit: usize,
        max_add: usize,
    ) -> Result<EvalGoldenMergeReport> {
        let trace_limit = trace_limit.max(1);
        let max_add = max_add.max(1);
        let mut cases = self.list_eval_golden_queries()?;
        let before_count = cases.len();
        let mut seen = cases.iter().map(eval_case_key).collect::<HashSet<_>>();

        let (trace_cases_raw, _) = self.collect_trace_eval_cases(trace_limit)?;
        let mut added_count = 0usize;
        for mut case in trace_cases_raw {
            if added_count >= max_add {
                break;
            }
            if case.expected_top_uri.is_none() {
                continue;
            }
            case.source = "golden-seed".to_string();
            let key = eval_case_key(&case);
            if !seen.insert(key) {
                continue;
            }
            cases.push(case);
            added_count += 1;
        }

        cases.sort_by(eval_case_ordering);
        let golden_uri = self.persist_eval_golden_queries(&cases)?;
        Ok(EvalGoldenMergeReport {
            golden_uri,
            before_count,
            added_count,
            after_count: cases.len(),
            trace_limit,
            max_add,
        })
    }
}
