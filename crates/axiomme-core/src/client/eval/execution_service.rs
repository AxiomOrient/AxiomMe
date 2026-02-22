use std::collections::{HashMap, HashSet};
use std::fs;

use crate::catalog::{eval_case_key, normalize_eval_case_source};
use crate::error::Result;
use crate::models::{EvalBucket, EvalCaseResult, EvalQueryCase, MetadataFilter};
use crate::quality::{build_eval_replay_command, classify_eval_bucket};
use crate::uri::uri_equivalent;

use super::AxiomMe;

const REQUIRED_FAILURE_BUCKETS: [&str; 5] = [
    "intent_miss",
    "filter_ignored",
    "memory_category_miss",
    "archive_context_miss",
    "relation_missing",
];

pub(super) struct EvalCaseSelection {
    pub query_cases: Vec<EvalQueryCase>,
    pub traces_scanned: usize,
    pub trace_cases_used: usize,
    pub golden_cases_used: usize,
}

pub(super) struct EvalExecutionOutcome {
    pub passed: usize,
    pub failed: usize,
    pub top1_accuracy: f32,
    pub buckets: Vec<EvalBucket>,
    pub failures: Vec<EvalCaseResult>,
}

impl AxiomMe {
    pub(super) fn select_eval_query_cases(
        &self,
        trace_limit: usize,
        query_limit: usize,
        include_golden: bool,
        golden_only: bool,
    ) -> Result<EvalCaseSelection> {
        let trace_limit = trace_limit.max(1);
        let query_limit = query_limit.max(1);

        let (trace_cases_raw, traces_scanned) = self.collect_trace_eval_cases(trace_limit)?;
        let golden_cases_raw = if include_golden {
            self.list_eval_golden_queries()?
        } else {
            Vec::new()
        };

        let mut seen = HashSet::<(String, Option<String>)>::new();
        let mut query_cases = Vec::<EvalQueryCase>::new();
        let mut golden_cases_used = 0usize;
        let mut trace_cases_used = 0usize;

        if include_golden {
            for mut case in golden_cases_raw {
                if query_cases.len() >= query_limit {
                    break;
                }
                normalize_eval_case_source(&mut case, "golden");
                let key = eval_case_key(&case);
                if !seen.insert(key) {
                    continue;
                }
                query_cases.push(case);
                golden_cases_used += 1;
            }
        }

        if !golden_only {
            for mut case in trace_cases_raw {
                if query_cases.len() >= query_limit {
                    break;
                }
                normalize_eval_case_source(&mut case, "trace");
                let key = eval_case_key(&case);
                if !seen.insert(key) {
                    continue;
                }
                query_cases.push(case);
                trace_cases_used += 1;
            }
        }

        Ok(EvalCaseSelection {
            query_cases,
            traces_scanned,
            trace_cases_used,
            golden_cases_used,
        })
    }

    pub(super) fn execute_eval_cases(
        &self,
        query_cases: &[EvalQueryCase],
        search_limit: usize,
    ) -> Result<EvalExecutionOutcome> {
        let search_limit = search_limit.max(1);

        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut buckets = HashMap::<String, usize>::new();
        let mut failures = Vec::<EvalCaseResult>::new();

        for case in query_cases {
            let actual_top_uri =
                self.eval_top_result_uri(&case.query, case.target_uri.as_deref(), search_limit)?;
            let case_passed = matches!(
                (case.expected_top_uri.as_deref(), actual_top_uri.as_deref()),
                (Some(expected), Some(actual)) if uri_equivalent(expected, actual)
            );
            if case_passed {
                passed += 1;
            } else {
                failed += 1;
            }

            let bucket_name = classify_eval_bucket(case, actual_top_uri.as_deref(), case_passed);
            *buckets.entry(bucket_name.to_string()).or_insert(0) += 1;

            if !case_passed {
                failures.push(EvalCaseResult {
                    source_trace_id: case.source_trace_id.clone(),
                    query: case.query.clone(),
                    target_uri: case.target_uri.clone(),
                    expected_top_uri: case.expected_top_uri.clone(),
                    actual_top_uri,
                    passed: false,
                    bucket: bucket_name.to_string(),
                    source: case.source.clone(),
                    replay_command: build_eval_replay_command(case, search_limit),
                });
            }
        }

        for (name, count) in Self::collect_required_failure_bucket_probes(search_limit)? {
            *buckets.entry(name).or_insert(0) += count;
        }
        for name in REQUIRED_FAILURE_BUCKETS {
            buckets.entry(name.to_string()).or_insert(0);
        }

        let mut bucket_values = buckets
            .into_iter()
            .map(|(name, count)| EvalBucket { name, count })
            .collect::<Vec<_>>();
        bucket_values.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

        let top1_accuracy = if query_cases.is_empty() {
            0.0
        } else {
            fraction_usize_as_f32(passed, query_cases.len())
        };

        Ok(EvalExecutionOutcome {
            passed,
            failed,
            top1_accuracy,
            buckets: bucket_values,
            failures,
        })
    }

    fn collect_required_failure_bucket_probes(
        search_limit: usize,
    ) -> Result<HashMap<String, usize>> {
        let probe_root =
            std::env::temp_dir().join(format!("axiomme-eval-probe-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&probe_root)?;
        let probe = Self::new(&probe_root)?;
        probe.initialize()?;

        let output = (|| -> Result<HashMap<String, usize>> {
            let mut buckets = initialize_required_failure_buckets();
            let corpus = setup_eval_probe_corpus(&probe_root)?;
            add_eval_probe_corpus(&probe, &corpus)?;
            run_filter_probe(&probe, search_limit, &mut buckets)?;
            run_relation_probe(&probe, search_limit, &mut buckets)?;
            run_intent_probe(&probe, search_limit, &mut buckets)?;
            run_archive_context_probe(&probe, &mut buckets)?;
            run_memory_category_probe(&probe, &mut buckets)?;
            Ok(buckets)
        })();

        let _ = fs::remove_dir_all(&probe_root);
        output
    }
}

fn fraction_usize_as_f32(numerator: usize, denominator: usize) -> f32 {
    if denominator == 0 {
        return 0.0;
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "eval accuracy is reported as f32 and accepts integer-to-float precision loss"
    )]
    {
        numerator as f32 / denominator as f32
    }
}

fn initialize_required_failure_buckets() -> HashMap<String, usize> {
    let mut buckets = HashMap::<String, usize>::new();
    for name in REQUIRED_FAILURE_BUCKETS {
        buckets.insert(name.to_string(), 0);
    }
    buckets
}

fn add_bucket_failure(buckets: &mut HashMap<String, usize>, bucket: &str) {
    *buckets.entry(bucket.to_string()).or_insert(0) += 1;
}

fn setup_eval_probe_corpus(probe_root: &std::path::Path) -> Result<std::path::PathBuf> {
    let corpus = probe_root.join("probe_corpus");
    fs::create_dir_all(&corpus)?;
    fs::write(
        corpus.join("auth.md"),
        "OAuth auth flow and access token exchange",
    )?;
    fs::write(
        corpus.join("storage.json"),
        "{\"storage\": true, \"cache\": true}",
    )?;
    Ok(corpus)
}

fn add_eval_probe_corpus(probe: &AxiomMe, corpus: &std::path::Path) -> Result<()> {
    probe.add_resource(
        corpus.to_string_lossy().as_ref(),
        Some("axiom://resources/eval-probe"),
        None,
        None,
        true,
        None,
    )?;
    Ok(())
}

fn run_filter_probe(
    probe: &AxiomMe,
    search_limit: usize,
    buckets: &mut HashMap<String, usize>,
) -> Result<()> {
    let mut tag_fields = HashMap::new();
    tag_fields.insert("tags".to_string(), serde_json::json!(["auth"]));
    let filtered = probe.find(
        "oauth flow",
        Some("axiom://resources/eval-probe"),
        Some(search_limit.max(5)),
        None,
        Some(MetadataFilter { fields: tag_fields }),
    )?;
    let filter_ok = !filtered.query_results.is_empty()
        && filtered
            .query_results
            .iter()
            .all(|hit| hit.uri.ends_with("auth.md"));
    if !filter_ok {
        add_bucket_failure(buckets, "filter_ignored");
    }
    Ok(())
}

fn run_relation_probe(
    probe: &AxiomMe,
    search_limit: usize,
    buckets: &mut HashMap<String, usize>,
) -> Result<()> {
    probe.link(
        "axiom://resources/eval-probe",
        "probe-relation",
        vec![
            "axiom://resources/eval-probe/auth.md".to_string(),
            "axiom://resources/eval-probe/storage.json".to_string(),
        ],
        "eval relation probe",
    )?;
    let relation = probe.find(
        "oauth",
        Some("axiom://resources/eval-probe"),
        Some(search_limit.max(5)),
        None,
        None::<MetadataFilter>,
    )?;
    let relation_ok = relation
        .query_results
        .iter()
        .any(|hit| !hit.relations.is_empty());
    if !relation_ok {
        add_bucket_failure(buckets, "relation_missing");
    }
    Ok(())
}

fn run_intent_probe(
    probe: &AxiomMe,
    search_limit: usize,
    buckets: &mut HashMap<String, usize>,
) -> Result<()> {
    let intent = probe.search(
        "oauth flow",
        Some("axiom://resources/eval-probe"),
        None,
        Some(search_limit.max(5)),
        None,
        None::<MetadataFilter>,
    )?;
    let intent_ok = !intent.query_plan.typed_queries.is_empty();
    if !intent_ok {
        add_bucket_failure(buckets, "intent_miss");
    }
    Ok(())
}

fn run_archive_context_probe(probe: &AxiomMe, buckets: &mut HashMap<String, usize>) -> Result<()> {
    let archive_session = probe.session(Some("eval-probe-archive"));
    archive_session.load()?;
    archive_session.add_message("user", "archive probe token refresh context")?;
    archive_session.commit()?;
    let context = archive_session.get_context_for_search("token refresh", 2, 8)?;
    let archive_ok = context
        .recent_messages
        .iter()
        .any(|msg| msg.text.contains("archive probe token refresh context"));
    if !archive_ok {
        add_bucket_failure(buckets, "archive_context_miss");
    }
    Ok(())
}

fn run_memory_category_probe(probe: &AxiomMe, buckets: &mut HashMap<String, usize>) -> Result<()> {
    let memory_session = probe.session(Some("eval-probe-memory"));
    memory_session.load()?;
    memory_session.add_message("user", "My name is Eval Probe")?;
    memory_session.add_message("user", "I prefer concise Rust code")?;
    memory_session.add_message("user", "This project repository is AxiomMe")?;
    memory_session.add_message("assistant", "Today we deployed release candidate")?;
    memory_session.add_message("assistant", "Root cause fixed with workaround")?;
    memory_session.add_message("assistant", "Always run this checklist before release")?;
    let commit = memory_session.commit()?;
    if commit.memories_extracted < 6 {
        add_bucket_failure(buckets, "memory_category_miss");
    }
    Ok(())
}
