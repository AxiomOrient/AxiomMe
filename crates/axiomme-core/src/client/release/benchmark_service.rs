use std::fs;
use std::time::Duration;
use std::time::Instant;

use chrono::Utc;
use walkdir::WalkDir;

use crate::catalog::{benchmark_gate_result_uri, release_check_result_uri};
use crate::config::RETRIEVAL_BACKEND_MEMORY;
use crate::error::{AxiomError, Result};
use crate::models::{
    BenchmarkCorpusMetadata, BenchmarkEnvironmentMetadata, BenchmarkGateResult, MetadataFilter,
    ReleaseCheckDocument, ReleaseGateStatus,
};
use crate::quality::{
    command_stdout, duration_to_latency_ms, duration_to_latency_us, infer_corpus_profile,
};
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

fn normalize_reranker_profile(raw: Option<&str>) -> String {
    match raw
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("off" | "none" | "disabled") => "off".to_string(),
        Some("doc-aware" | "doc-aware-v1") | None => "doc-aware-v1".to_string(),
        Some(other) if !other.is_empty() => other.to_string(),
        _ => "doc-aware-v1".to_string(),
    }
}

pub const RELEASE_BENCHMARK_SEED_TARGET_URI: &str = "axiom://resources/release-gate-seed";
pub const RELEASE_BENCHMARK_SEED_FILE_NAME: &str = "axiomme_release_benchmark_seed.txt";
pub const RELEASE_BENCHMARK_SEED_QUERY: &str = "release benchmark seed context";
pub const RELEASE_BENCHMARK_SEED_QUERY_STABLE: &str = "deterministic retrieval quality";
pub const RELEASE_BENCHMARK_SEED_EXPECTED_URI: &str =
    "axiom://resources/release-gate-seed/axiomme_release_benchmark_seed.txt";

impl AxiomMe {
    pub fn measure_benchmark_commit_latencies(&self, samples: usize) -> Result<Vec<u128>> {
        let (latencies_ms, _) = self.measure_benchmark_commit_latencies_with_units(samples)?;
        Ok(latencies_ms)
    }

    pub fn measure_benchmark_commit_latencies_with_units(
        &self,
        samples: usize,
    ) -> Result<(Vec<u128>, Vec<u128>)> {
        let durations = self.collect_benchmark_commit_latency_durations(samples)?;
        let latencies_ms = durations
            .iter()
            .copied()
            .map(duration_to_latency_ms)
            .collect::<Vec<_>>();
        let latencies_us = durations
            .into_iter()
            .map(duration_to_latency_us)
            .collect::<Vec<_>>();
        Ok((latencies_ms, latencies_us))
    }

    fn collect_benchmark_commit_latency_durations(&self, samples: usize) -> Result<Vec<Duration>> {
        let mut latencies = Vec::new();
        for idx in 0..samples.max(1) {
            let session_id = format!("bench-commit-{}", uuid::Uuid::new_v4().simple());
            let session = self.session(Some(&session_id));
            session.load()?;
            session.add_message("user", format!("benchmark commit sample {idx}"))?;
            session.add_message("assistant", "benchmark ack")?;
            let started = Instant::now();
            let _ = session.commit()?;
            latencies.push(started.elapsed());
            let _ = self.delete(&session_id);
        }
        Ok(latencies)
    }

    #[must_use]
    pub fn collect_benchmark_environment_metadata(&self) -> BenchmarkEnvironmentMetadata {
        let hw_model = command_stdout("sysctl", &["-n", "hw.model"]).unwrap_or_default();
        let cpu_model = command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"])
            .unwrap_or_else(|| format!("{} ({})", std::env::consts::ARCH, "unknown-cpu"));
        let ram_bytes = command_stdout("sysctl", &["-n", "hw.memsize"])
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(0);
        let os_version = command_stdout("sw_vers", &["-productVersion"]).map_or_else(
            || std::env::consts::OS.to_string(),
            |version| format!("macOS {version}"),
        );
        let rustc_version =
            command_stdout("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_string());
        let retrieval_backend = RETRIEVAL_BACKEND_MEMORY.to_string();
        let reranker_profile = normalize_reranker_profile(self.config.search.reranker.as_deref());
        let embedding = crate::embedding::embedding_profile();

        let machine_profile = if hw_model.to_ascii_lowercase().contains("macmini") {
            "mac-mini-single-node".to_string()
        } else {
            "personal-single-node".to_string()
        };

        BenchmarkEnvironmentMetadata {
            machine_profile,
            cpu_model,
            ram_bytes,
            os_version,
            rustc_version,
            retrieval_backend,
            reranker_profile,
            embedding_provider: Some(embedding.provider),
            embedding_vector_version: Some(embedding.vector_version),
            embedding_strict_error: crate::embedding::embedding_strict_error(),
        }
    }

    pub fn collect_benchmark_corpus_metadata(&self) -> Result<BenchmarkCorpusMetadata> {
        let root_uri = AxiomUri::root(Scope::Resources);
        let root_path = self.fs.resolve_uri(&root_uri);

        let mut rows = Vec::<(String, u64, i64)>::new();
        let mut total_bytes = 0u64;
        if root_path.exists() {
            for entry in WalkDir::new(&root_path).follow_links(false) {
                let entry = entry.map_err(|e| AxiomError::Validation(e.to_string()))?;
                if entry.file_type().is_symlink() || !entry.file_type().is_file() {
                    continue;
                }

                let rel = entry
                    .path()
                    .strip_prefix(&root_path)
                    .map_err(|e| AxiomError::Validation(e.to_string()))?;
                let meta = entry
                    .metadata()
                    .map_err(|e| AxiomError::Validation(e.to_string()))?;
                let size = meta.len();
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |delta| {
                        i64::try_from(delta.as_secs()).unwrap_or(i64::MAX)
                    });
                total_bytes = total_bytes.saturating_add(size);
                rows.push((rel.to_string_lossy().replace('\\', "/"), size, modified));
            }
        }

        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let mut hasher = blake3::Hasher::new();
        for (path, size, modified) in &rows {
            hasher.update(path.as_bytes());
            hasher.update(&size.to_be_bytes());
            hasher.update(&modified.to_be_bytes());
        }
        let digest = hasher.finalize().to_hex().to_string();
        let snapshot_id = format!("resources-{}", &digest[..12.min(digest.len())]);
        let file_count = rows.len();

        Ok(BenchmarkCorpusMetadata {
            profile: infer_corpus_profile(file_count, total_bytes),
            snapshot_id,
            root_uri: root_uri.to_string(),
            file_count,
            total_bytes,
        })
    }

    pub fn persist_benchmark_gate_result(&self, result: &BenchmarkGateResult) -> Result<String> {
        let uri = benchmark_gate_result_uri(&uuid::Uuid::new_v4().to_string())?;
        self.fs
            .write(&uri, &serde_json::to_string_pretty(result)?, true)?;
        Ok(uri.to_string())
    }

    pub fn persist_release_check_result(&self, result: &BenchmarkGateResult) -> Result<String> {
        let check_id = uuid::Uuid::new_v4().to_string();
        let uri = release_check_result_uri(&check_id)?;
        let doc = ReleaseCheckDocument {
            version: 1,
            check_id,
            created_at: Utc::now().to_rfc3339(),
            gate_profile: result.gate_profile.clone(),
            status: ReleaseGateStatus::from_passed(result.passed),
            passed: result.passed,
            reasons: result.reasons.clone(),
            threshold_p95_ms: result.threshold_p95_ms,
            min_top1_accuracy: result.min_top1_accuracy,
            min_stress_top1_accuracy: result.min_stress_top1_accuracy,
            max_p95_regression_pct: result.max_p95_regression_pct,
            max_top1_regression_pct: result.max_top1_regression_pct,
            window_size: result.window_size,
            required_passes: result.required_passes,
            evaluated_runs: result.evaluated_runs,
            passing_runs: result.passing_runs,
            latest_report_uri: result.latest.as_ref().map(|x| x.report_uri.clone()),
            previous_report_uri: result.previous.as_ref().map(|x| x.report_uri.clone()),
            latest_p95_latency_us: result.latest.as_ref().and_then(|x| x.p95_latency_us),
            previous_p95_latency_us: result.previous.as_ref().and_then(|x| x.p95_latency_us),
            embedding_provider: result.embedding_provider.clone(),
            embedding_strict_error: result.embedding_strict_error.clone(),
            gate_record_uri: result.gate_record_uri.clone(),
        };
        self.fs
            .write(&uri, &serde_json::to_string_pretty(&doc)?, true)?;
        Ok(uri.to_string())
    }

    pub fn evaluate_session_memory_gate(&self) -> Result<(bool, usize, String)> {
        let probe_root =
            std::env::temp_dir().join(format!("axiomme-release-g4-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&probe_root)?;
        let probe = Self::new(&probe_root)?;
        probe.initialize()?;

        let output = (|| -> Result<(bool, usize, String)> {
            let session_id = format!("release-gate-{}", uuid::Uuid::new_v4().simple());
            let session = probe.session(Some(&session_id));
            session.load()?;
            session.add_message("user", "My name is release-gate probe")?;
            session.add_message("user", "I prefer deterministic release checks")?;
            session.add_message("user", "This project repository is AxiomMe")?;
            session.add_message("assistant", "Today we deployed release candidate")?;
            session.add_message("assistant", "Root cause fixed with workaround")?;
            session.add_message("assistant", "Always run this checklist before release")?;
            let commit = session.commit()?;

            let find = probe.find(
                "deterministic release checks",
                Some("axiom://user/memories/preferences"),
                Some(5),
                None,
                None::<MetadataFilter>,
            )?;
            let hit_count = find.query_results.len();

            let profile_uri = AxiomUri::parse("axiom://user/memories/profile.md")?;
            let has_profile = probe.fs.exists(&profile_uri);
            let preferences_count = probe
                .ls("axiom://user/memories/preferences", false, false)
                .map(|entries| entries.into_iter().filter(|entry| !entry.is_dir).count())
                .unwrap_or(0);
            let entities_count = probe
                .ls("axiom://user/memories/entities", false, false)
                .map(|entries| entries.into_iter().filter(|entry| !entry.is_dir).count())
                .unwrap_or(0);
            let events_count = probe
                .ls("axiom://user/memories/events", false, false)
                .map(|entries| entries.into_iter().filter(|entry| !entry.is_dir).count())
                .unwrap_or(0);
            let cases_count = probe
                .ls("axiom://agent/memories/cases", false, false)
                .map(|entries| entries.into_iter().filter(|entry| !entry.is_dir).count())
                .unwrap_or(0);
            let patterns_count = probe
                .ls("axiom://agent/memories/patterns", false, false)
                .map(|entries| entries.into_iter().filter(|entry| !entry.is_dir).count())
                .unwrap_or(0);

            let missing_categories = [
                (!has_profile, "profile"),
                (preferences_count == 0, "preferences"),
                (entities_count == 0, "entities"),
                (events_count == 0, "events"),
                (cases_count == 0, "cases"),
                (patterns_count == 0, "patterns"),
            ]
            .into_iter()
            .filter_map(|(missing, name)| if missing { Some(name) } else { None })
            .collect::<Vec<_>>();
            let memory_category_miss = missing_categories.len();

            let passed =
                commit.memories_extracted >= 6 && hit_count > 0 && memory_category_miss == 0;
            let details = format!(
                "session_id={} memories_extracted={} hit_count={} missing_categories={}",
                session_id,
                commit.memories_extracted,
                hit_count,
                if missing_categories.is_empty() {
                    "-".to_string()
                } else {
                    missing_categories.join(",")
                }
            );
            Ok((passed, memory_category_miss, details))
        })();

        let _ = fs::remove_dir_all(&probe_root);
        output
    }

    pub fn ensure_release_benchmark_seed_trace(&self) -> Result<()> {
        let seed_text =
            "AxiomMe release benchmark seed context for deterministic retrieval quality.";
        let source_path = std::env::temp_dir().join(RELEASE_BENCHMARK_SEED_FILE_NAME);
        fs::write(&source_path, format!("{seed_text}\n"))?;
        let source = source_path.to_string_lossy().to_string();
        let add_result = self.add_resource(
            &source,
            Some(RELEASE_BENCHMARK_SEED_TARGET_URI),
            None,
            None,
            true,
            None,
        );
        let _ = fs::remove_file(&source_path);
        add_result?;
        let _ = self.find(
            RELEASE_BENCHMARK_SEED_QUERY,
            Some(RELEASE_BENCHMARK_SEED_TARGET_URI),
            Some(5),
            None,
            None::<MetadataFilter>,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::models::{
        BenchmarkGateResult, BenchmarkGateRunResult, BenchmarkSummary, ReleaseCheckDocument,
    };
    use crate::uri::AxiomUri;

    use super::AxiomMe;
    use super::normalize_reranker_profile;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn benchmark_environment_normalizes_reranker_values() {
        assert_eq!(normalize_reranker_profile(None), "doc-aware-v1");
        assert_eq!(
            normalize_reranker_profile(Some("doc-aware")),
            "doc-aware-v1"
        );
        assert_eq!(normalize_reranker_profile(Some("OFF")), "off");
    }

    #[test]
    fn persist_release_check_result_copies_structured_embedding_fields() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        let result = BenchmarkGateResult {
            passed: false,
            gate_profile: "macmini-release".to_string(),
            threshold_p95_ms: 10_000,
            min_top1_accuracy: 0.75,
            min_stress_top1_accuracy: None,
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 2,
            required_passes: 2,
            evaluated_runs: 2,
            passing_runs: 1,
            latest: Some(BenchmarkSummary {
                run_id: "run-a".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                executed_cases: 10,
                top1_accuracy: 0.5,
                p95_latency_ms: 900,
                p95_latency_us: Some(899_750),
                report_uri: "axiom://queue/benchmarks/reports/a.json".to_string(),
            }),
            previous: None,
            regression_pct: None,
            top1_regression_pct: None,
            stress_top1_accuracy: None,
            run_results: vec![BenchmarkGateRunResult {
                run_id: "run-a".to_string(),
                passed: false,
                p95_latency_ms: 900,
                p95_latency_us: Some(899_750),
                top1_accuracy: 0.5,
                stress_top1_accuracy: None,
                regression_pct: None,
                top1_regression_pct: None,
                reasons: vec![
                    "release_embedding_provider_required:semantic-lite!=semantic-model-http"
                        .to_string(),
                ],
            }],
            gate_record_uri: None,
            release_check_uri: None,
            embedding_provider: Some("semantic-lite".to_string()),
            embedding_strict_error: Some("semantic-model-http embed request failed".to_string()),
            reasons: vec![
                "release_embedding_provider_required:semantic-lite!=semantic-model-http"
                    .to_string(),
                "release_embedding_strict_error:semantic-model-http embed request failed"
                    .to_string(),
            ],
        };

        let uri = app
            .persist_release_check_result(&result)
            .expect("persist release check");
        let parsed = AxiomUri::parse(&uri).expect("uri parse");
        let raw = app.fs.read(&parsed).expect("read");
        let doc: ReleaseCheckDocument = serde_json::from_str(&raw).expect("doc parse");
        assert_eq!(doc.embedding_provider.as_deref(), Some("semantic-lite"));
        assert_eq!(
            doc.embedding_strict_error.as_deref(),
            Some("semantic-model-http embed request failed")
        );
        assert_eq!(doc.latest_p95_latency_us, Some(899_750));
        assert_eq!(doc.previous_p95_latency_us, None);
    }

    #[cfg(unix)]
    #[test]
    fn collect_benchmark_corpus_metadata_skips_symlink_files() {
        let temp = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside");
        let app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        let before = app
            .collect_benchmark_corpus_metadata()
            .expect("metadata before");

        let outside_file = outside.path().join("secret.md");
        fs::write(&outside_file, "outside payload").expect("write outside");

        let link_path = temp.path().join("resources").join("linked-secret.md");
        symlink(&outside_file, &link_path).expect("symlink");

        let after = app
            .collect_benchmark_corpus_metadata()
            .expect("metadata after");

        assert_eq!(before.file_count, after.file_count);
        assert_eq!(before.total_bytes, after.total_bytes);
        assert_eq!(before.snapshot_id, after.snapshot_id);
    }
}
