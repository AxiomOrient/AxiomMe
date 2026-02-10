use std::fs;
use std::time::Instant;

use chrono::Utc;
use walkdir::WalkDir;

use crate::catalog::{benchmark_gate_result_uri, release_check_result_uri};
use crate::error::Result;
use crate::models::{
    BenchmarkCorpusMetadata, BenchmarkEnvironmentMetadata, BenchmarkGateResult, MetadataFilter,
    ReleaseCheckDocument,
};
use crate::quality::{command_stdout, infer_corpus_profile};
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

fn normalize_retrieval_backend(raw: Option<String>) -> String {
    match raw
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("sqlite") | Some("fts") | Some("fts5") | Some("bm25") | None => "sqlite".to_string(),
        Some("qdrant") => "qdrant".to_string(),
        Some("hybrid") => "hybrid".to_string(),
        Some("memory") => "memory".to_string(),
        Some(other) if !other.is_empty() => other.to_string(),
        _ => "sqlite".to_string(),
    }
}

fn normalize_reranker_profile(raw: Option<String>) -> String {
    match raw
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("off") | Some("none") | Some("disabled") => "off".to_string(),
        Some("doc-aware") | Some("doc-aware-v1") | None => "doc-aware-v1".to_string(),
        Some(other) if !other.is_empty() => other.to_string(),
        _ => "doc-aware-v1".to_string(),
    }
}

impl AxiomMe {
    pub(super) fn measure_benchmark_commit_latencies(&self, samples: usize) -> Result<Vec<u128>> {
        let mut latencies = Vec::new();
        for idx in 0..samples.max(1) {
            let session_id = format!("bench-commit-{}", uuid::Uuid::new_v4().simple());
            let session = self.session(Some(&session_id));
            session.load()?;
            session.add_message("user", format!("benchmark commit sample {}", idx))?;
            session.add_message("assistant", "benchmark ack")?;
            let started = Instant::now();
            let _ = session.commit()?;
            latencies.push(started.elapsed().as_millis());
            let _ = self.delete(&session_id);
        }
        Ok(latencies)
    }

    pub(super) fn collect_benchmark_environment_metadata(&self) -> BenchmarkEnvironmentMetadata {
        let hw_model = command_stdout("sysctl", &["-n", "hw.model"]).unwrap_or_default();
        let cpu_model = command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"])
            .unwrap_or_else(|| format!("{} ({})", std::env::consts::ARCH, "unknown-cpu"));
        let ram_bytes = command_stdout("sysctl", &["-n", "hw.memsize"])
            .and_then(|raw| raw.parse::<u64>().ok())
            .unwrap_or(0);
        let os_version = command_stdout("sw_vers", &["-productVersion"])
            .map(|v| format!("macOS {v}"))
            .unwrap_or_else(|| std::env::consts::OS.to_string());
        let rustc_version =
            command_stdout("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_string());
        let retrieval_backend =
            normalize_retrieval_backend(std::env::var("AXIOMME_RETRIEVAL_BACKEND").ok());
        let reranker_profile = normalize_reranker_profile(std::env::var("AXIOMME_RERANKER").ok());
        let qdrant_version = self
            .qdrant
            .as_ref()
            .map(|qdrant| {
                qdrant
                    .server_version()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "unknown".to_string())
            })
            .unwrap_or_else(|| "disabled".to_string());
        let qdrant_enabled = self.qdrant.is_some();
        let qdrant_base_url = self
            .qdrant
            .as_ref()
            .map(|qdrant| qdrant.config().base_url.clone());
        let qdrant_collection = self
            .qdrant
            .as_ref()
            .map(|qdrant| qdrant.config().collection.clone());

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
            qdrant_version,
            qdrant_enabled,
            qdrant_base_url,
            qdrant_collection,
        }
    }

    pub(super) fn collect_benchmark_corpus_metadata(&self) -> Result<BenchmarkCorpusMetadata> {
        let root_uri = AxiomUri::root(Scope::Resources);
        let root_path = self.fs.resolve_uri(&root_uri);

        let mut rows = Vec::<(String, u64, i64)>::new();
        let mut total_bytes = 0u64;
        if root_path.exists() {
            for entry in WalkDir::new(&root_path)
                .follow_links(false)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                if !entry.path().is_file() {
                    continue;
                }

                let Ok(rel) = entry.path().strip_prefix(&root_path) else {
                    continue;
                };
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                let size = meta.len();
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|delta| delta.as_secs() as i64)
                    .unwrap_or(0);
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

    pub(super) fn persist_benchmark_gate_result(
        &self,
        result: &BenchmarkGateResult,
    ) -> Result<String> {
        let uri = benchmark_gate_result_uri(&uuid::Uuid::new_v4().to_string())?;
        self.fs
            .write(&uri, &serde_json::to_string_pretty(result)?, true)?;
        Ok(uri.to_string())
    }

    pub(super) fn persist_release_check_result(
        &self,
        result: &BenchmarkGateResult,
    ) -> Result<String> {
        let check_id = uuid::Uuid::new_v4().to_string();
        let uri = release_check_result_uri(&check_id)?;
        let doc = ReleaseCheckDocument {
            version: 1,
            check_id,
            created_at: Utc::now().to_rfc3339(),
            gate_profile: result.gate_profile.clone(),
            status: if result.passed { "pass" } else { "fail" }.to_string(),
            passed: result.passed,
            reasons: result.reasons.clone(),
            threshold_p95_ms: result.threshold_p95_ms,
            min_top1_accuracy: result.min_top1_accuracy,
            max_p95_regression_pct: result.max_p95_regression_pct,
            max_top1_regression_pct: result.max_top1_regression_pct,
            window_size: result.window_size,
            required_passes: result.required_passes,
            evaluated_runs: result.evaluated_runs,
            passing_runs: result.passing_runs,
            latest_report_uri: result.latest.as_ref().map(|x| x.report_uri.clone()),
            previous_report_uri: result.previous.as_ref().map(|x| x.report_uri.clone()),
            gate_record_uri: result.gate_record_uri.clone(),
        };
        self.fs
            .write(&uri, &serde_json::to_string_pretty(&doc)?, true)?;
        Ok(uri.to_string())
    }

    pub(super) fn evaluate_session_memory_gate(&self) -> Result<(bool, usize, String)> {
        let probe_root =
            std::env::temp_dir().join(format!("axiomme-release-g4-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&probe_root)?;
        let probe = AxiomMe::new(&probe_root)?;
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

    pub(super) fn ensure_release_benchmark_seed_trace(&self) -> Result<()> {
        let seed_text =
            "AxiomMe release benchmark seed context for deterministic retrieval quality.";
        let seed_query = "release benchmark seed context";
        let source_path = std::env::temp_dir().join("axiomme_release_benchmark_seed.txt");
        fs::write(&source_path, format!("{seed_text}\n"))?;
        let source = source_path.to_string_lossy().to_string();
        let target_uri = "axiom://resources/release-gate-seed";
        let add_result = self.add_resource(&source, Some(target_uri), None, None, true, None);
        let _ = fs::remove_file(&source_path);
        add_result?;
        let _ = self.find(
            seed_query,
            Some(target_uri),
            Some(5),
            None,
            None::<MetadataFilter>,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_reranker_profile, normalize_retrieval_backend};

    #[test]
    fn benchmark_environment_normalizes_backend_values() {
        assert_eq!(normalize_retrieval_backend(None), "sqlite");
        assert_eq!(
            normalize_retrieval_backend(Some("BM25".to_string())),
            "sqlite"
        );
        assert_eq!(
            normalize_retrieval_backend(Some("hybrid".to_string())),
            "hybrid"
        );
    }

    #[test]
    fn benchmark_environment_normalizes_reranker_values() {
        assert_eq!(normalize_reranker_profile(None), "doc-aware-v1");
        assert_eq!(
            normalize_reranker_profile(Some("doc-aware".to_string())),
            "doc-aware-v1"
        );
        assert_eq!(normalize_reranker_profile(Some("OFF".to_string())), "off");
    }
}
