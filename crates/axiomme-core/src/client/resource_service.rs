use std::io::Read;
use std::path::Path;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use chrono::{DateTime, Utc};

use crate::context_ops::default_resource_target;
use crate::error::{AxiomError, Result};
use crate::ingest::{IngestManager, IngestSession};
use crate::models::{
    AddResourceIngestOptions, AddResourceResult, GlobResult, QueueCounts, QueueStatus,
};
use crate::pack;
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

const MAX_REMOTE_TEXT_BYTES: usize = 5 * 1024 * 1024;
const WAIT_PROCESSED_MIN_SLEEP: Duration = Duration::from_millis(100);
const WAIT_PROCESSED_MAX_SLEEP: Duration = Duration::from_secs(1);

impl AxiomMe {
    fn add_resource_core(
        &self,
        path_or_url: &str,
        target: Option<&str>,
        wait: bool,
        timeout_secs: Option<u64>,
        ingest_options: &AddResourceIngestOptions,
    ) -> Result<AddResourceResult> {
        let target_uri = target
            .map(AxiomUri::parse)
            .transpose()?
            .map_or_else(|| default_resource_target(path_or_url), Ok)?;
        let ingest_manager = IngestManager::new(self.fs.clone(), self.parser_registry.clone());
        let mut ingest = ingest_manager.start_session()?;
        if let Err(err) =
            stage_add_resource_source(path_or_url, timeout_secs, &mut ingest, ingest_options)
        {
            ingest.abort();
            return Err(err);
        }
        if let Err(err) = ingest.write_manifest(path_or_url) {
            ingest.abort();
            return Err(err);
        }
        if let Err(err) = ingest.finalize_to(&target_uri) {
            ingest.abort();
            return Err(err);
        }
        self.state.enqueue(
            "semantic_scan",
            &target_uri.to_string(),
            serde_json::json!({"op": "add_resource"}),
        )?;
        if wait {
            let _ = self.replay_outbox(256, false)?;
        }

        Ok(AddResourceResult {
            root_uri: target_uri.to_string(),
            queued: !wait,
            message: if wait {
                "resource ingested".to_string()
            } else {
                "resource staged and queued for semantic processing".to_string()
            },
        })
    }

    pub fn add_resource(
        &self,
        path_or_url: &str,
        target: Option<&str>,
        _reason: Option<&str>,
        _instruction: Option<&str>,
        wait: bool,
        timeout_secs: Option<u64>,
    ) -> Result<AddResourceResult> {
        self.add_resource_with_ingest_options(
            path_or_url,
            target,
            _reason,
            _instruction,
            wait,
            timeout_secs,
            AddResourceIngestOptions::default(),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "public API keeps explicit call fields while exposing ingest controls"
    )]
    pub fn add_resource_with_ingest_options(
        &self,
        path_or_url: &str,
        target: Option<&str>,
        _reason: Option<&str>,
        _instruction: Option<&str>,
        wait: bool,
        timeout_secs: Option<u64>,
        ingest_options: AddResourceIngestOptions,
    ) -> Result<AddResourceResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let source = path_or_url.to_string();
        let target_raw = target.map(ToString::to_string);
        let output =
            self.add_resource_core(path_or_url, target, wait, timeout_secs, &ingest_options);
        let ingest_options_json = serde_json::to_value(&ingest_options).unwrap_or_else(|_| {
            serde_json::json!({
                "markdown_only": ingest_options.markdown_only,
                "include_hidden": ingest_options.include_hidden,
                "exclude_globs": ingest_options.exclude_globs,
            })
        });

        match output {
            Ok(result) => {
                self.log_request_status(
                    request_id,
                    "add_resource",
                    "ok",
                    started,
                    Some(result.root_uri.clone()),
                    Some(serde_json::json!({
                        "source": source,
                        "wait": wait,
                        "queued": result.queued,
                        "ingest_options": ingest_options_json,
                    })),
                );
                Ok(result)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "add_resource",
                    started,
                    target_raw,
                    &err,
                    Some(serde_json::json!({
                        "source": source,
                        "wait": wait,
                        "ingest_options": ingest_options_json,
                    })),
                );
                Err(err)
            }
        }
    }

    pub fn wait_processed(&self, timeout_secs: Option<u64>) -> Result<QueueStatus> {
        let timeout = Duration::from_secs(timeout_secs.unwrap_or(30).max(1));
        let started = Instant::now();

        loop {
            let counts = self.state.queue_counts()?;
            if counts.new_total == 0 && counts.processing == 0 {
                return self.state.queue_status();
            }

            if counts.new_due > 0 {
                let replay_limit = counts.new_due.clamp(1, 256) as usize;
                let _ = self.replay_outbox(replay_limit, false)?;
            }

            let after = self.state.queue_counts()?;
            if after.new_total == 0 && after.processing == 0 {
                return self.state.queue_status();
            }

            if started.elapsed() >= timeout {
                return Err(AxiomError::Conflict(format!(
                    "wait_processed timeout after {}s: new_total={} processing={} dead_letter={}",
                    timeout.as_secs(),
                    after.new_total,
                    after.processing,
                    after.dead_letter
                )));
            }

            let timeout_remaining = timeout.saturating_sub(started.elapsed());
            let sleep_for = wait_processed_sleep_duration(&after, timeout_remaining);
            if !sleep_for.is_zero() {
                thread::sleep(sleep_for);
            }
        }
    }

    pub fn ls(
        &self,
        uri: &str,
        recursive: bool,
        _simple: bool,
    ) -> Result<Vec<crate::models::Entry>> {
        let uri = AxiomUri::parse(uri)?;
        self.fs.list(&uri, recursive)
    }

    pub fn glob(&self, pattern: &str, uri: Option<&str>) -> Result<GlobResult> {
        let base = if let Some(raw) = uri {
            Some(AxiomUri::parse(raw)?)
        } else {
            None
        };
        let matches = self.fs.glob(base.as_ref(), pattern)?;
        Ok(GlobResult { matches })
    }

    pub fn read(&self, uri: &str) -> Result<String> {
        let uri = AxiomUri::parse(uri)?;
        self.fs.read(&uri)
    }

    pub fn abstract_text(&self, uri: &str) -> Result<String> {
        let uri = AxiomUri::parse(uri)?;
        self.fs.read_abstract(&uri)
    }

    pub fn overview(&self, uri: &str) -> Result<String> {
        let uri = AxiomUri::parse(uri)?;
        self.fs.read_overview(&uri)
    }

    pub fn mkdir(&self, uri: &str) -> Result<()> {
        let uri = AxiomUri::parse(uri)?;
        if !matches!(
            uri.scope(),
            Scope::Resources | Scope::User | Scope::Agent | Scope::Session
        ) {
            return Err(AxiomError::PermissionDenied(format!(
                "mkdir is not allowed for scope: {}",
                uri.scope()
            )));
        }

        self.fs.create_dir_all(&uri, false)?;
        self.reindex_uri_tree(&uri)?;
        self.state.enqueue(
            "reindex",
            &uri.to_string(),
            serde_json::json!({"op": "mkdir"}),
        )?;
        Ok(())
    }

    pub fn rm(&self, uri: &str, recursive: bool) -> Result<()> {
        let uri = AxiomUri::parse(uri)?;
        self.fs.rm(&uri, recursive, false)?;

        self.prune_index_prefix_from_memory(&uri)?;
        self.state
            .remove_search_documents_with_prefix(&uri.to_string())?;
        self.state
            .remove_index_state_with_prefix(&uri.to_string())?;

        self.state.enqueue(
            "delete",
            &uri.to_string(),
            serde_json::json!({"op": "rm", "recursive": recursive}),
        )?;
        Ok(())
    }

    pub fn mv(&self, from_uri: &str, to_uri: &str) -> Result<()> {
        let from = AxiomUri::parse(from_uri)?;
        let to = AxiomUri::parse(to_uri)?;
        if from.scope() != to.scope() {
            return Err(AxiomError::PermissionDenied(format!(
                "cross-scope move is not allowed: {} -> {}",
                from.scope(),
                to.scope()
            )));
        }
        self.fs.mv(&from, &to, false)?;
        self.prune_index_prefix_from_memory(&from)?;
        self.state
            .remove_search_documents_with_prefix(&from.to_string())?;
        self.state
            .remove_index_state_with_prefix(&from.to_string())?;
        self.reindex_uri_tree(&to)?;

        self.state.enqueue(
            "reindex",
            &to.to_string(),
            serde_json::json!({"op": "mv", "from": from_uri}),
        )?;
        Ok(())
    }

    pub fn tree(&self, uri: &str) -> Result<crate::models::TreeResult> {
        let uri = AxiomUri::parse(uri)?;
        self.fs.tree(&uri)
    }

    pub fn export_ovpack(&self, uri: &str, to: &str) -> Result<String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let uri_raw = uri.to_string();
        let to_path = to.to_string();

        let output = (|| -> Result<String> {
            let uri = AxiomUri::parse(uri)?;
            if !matches!(
                uri.scope(),
                Scope::Resources | Scope::User | Scope::Agent | Scope::Session
            ) {
                return Err(AxiomError::PermissionDenied(
                    "ovpack export is not allowed for internal scopes".to_string(),
                ));
            }
            let out = pack::export_ovpack(&self.fs, &uri, Path::new(to))?;
            Ok(out.display().to_string())
        })();

        match output {
            Ok(export_path) => {
                self.log_request_status(
                    request_id,
                    "ovpack.export",
                    "ok",
                    started,
                    Some(uri_raw),
                    Some(serde_json::json!({
                        "to": to_path,
                        "output": export_path,
                    })),
                );
                Ok(export_path)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "ovpack.export",
                    started,
                    Some(uri_raw),
                    &err,
                    Some(serde_json::json!({
                        "to": to_path,
                    })),
                );
                Err(err)
            }
        }
    }

    pub fn import_ovpack(
        &self,
        file_path: &str,
        parent: &str,
        force: bool,
        vectorize: bool,
    ) -> Result<String> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let file_path_raw = file_path.to_string();
        let parent_raw = parent.to_string();

        let output = (|| -> Result<String> {
            let parent_uri = AxiomUri::parse(parent)?;
            if !matches!(
                parent_uri.scope(),
                Scope::Resources | Scope::User | Scope::Agent | Scope::Session
            ) {
                return Err(AxiomError::PermissionDenied(
                    "ovpack import is not allowed for internal scopes".to_string(),
                ));
            }
            let imported = pack::import_ovpack(&self.fs, Path::new(file_path), &parent_uri, force)?;
            if vectorize {
                self.prune_index_prefix_from_memory(&imported)?;
                self.state
                    .remove_search_documents_with_prefix(&imported.to_string())?;
                self.state
                    .remove_index_state_with_prefix(&imported.to_string())?;
                self.ensure_tiers_recursive(&imported)?;
                self.reindex_uri_tree(&imported)?;
            }
            Ok(imported.to_string())
        })();

        match output {
            Ok(imported_uri) => {
                self.log_request_status(
                    request_id,
                    "ovpack.import",
                    "ok",
                    started,
                    Some(parent_raw),
                    Some(serde_json::json!({
                        "file_path": file_path_raw,
                        "force": force,
                        "vectorize": vectorize,
                        "imported_uri": imported_uri,
                    })),
                );
                Ok(imported_uri)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "ovpack.import",
                    started,
                    Some(parent_raw),
                    &err,
                    Some(serde_json::json!({
                        "file_path": file_path_raw,
                        "force": force,
                        "vectorize": vectorize,
                    })),
                );
                Err(err)
            }
        }
    }
}

impl AxiomMe {
    pub(super) fn prune_index_prefix_from_memory(&self, prefix: &AxiomUri) -> Result<Vec<String>> {
        let doomed = {
            let mut index = self
                .index
                .write()
                .map_err(|_| AxiomError::lock_poisoned("index"))?;
            let doomed = index
                .all_records()
                .into_iter()
                .map(|record| record.uri)
                .filter(|uri| {
                    AxiomUri::parse(uri)
                        .map(|parsed| parsed.starts_with(prefix))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            for uri in &doomed {
                index.remove(uri);
            }
            doomed
        };
        Ok(doomed)
    }
}

fn stage_add_resource_source(
    path_or_url: &str,
    timeout_secs: Option<u64>,
    ingest: &mut IngestSession,
    ingest_options: &AddResourceIngestOptions,
) -> Result<()> {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(30).max(1));
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()?;
        let resp = client.get(path_or_url).send()?;
        if !resp.status().is_success() {
            return Err(AxiomError::Internal(format!(
                "failed to fetch {path_or_url}: status {}",
                resp.status()
            )));
        }
        if let Some(bytes) = resp.content_length()
            && bytes > MAX_REMOTE_TEXT_BYTES as u64
        {
            return Err(AxiomError::Validation(format!(
                "remote resource too large: {bytes} bytes (limit {MAX_REMOTE_TEXT_BYTES})"
            )));
        }
        let text = read_remote_text_limited(resp, MAX_REMOTE_TEXT_BYTES)?;
        return ingest.stage_text("source.txt", &text);
    }

    let src = Path::new(path_or_url);
    if !src.exists() {
        return Err(AxiomError::NotFound(path_or_url.to_string()));
    }
    ingest.stage_local_path_with_options(src, ingest_options)
}

fn read_remote_text_limited<R: Read>(mut reader: R, max_bytes: usize) -> Result<String> {
    let mut body = Vec::new();
    let mut limited = (&mut reader).take((max_bytes as u64) + 1);
    limited.read_to_end(&mut body)?;
    if body.len() > max_bytes {
        return Err(AxiomError::Validation(format!(
            "remote resource too large after download: {} bytes (limit {max_bytes})",
            body.len()
        )));
    }
    String::from_utf8(body).map_err(|err| {
        AxiomError::Validation(format!("remote resource is not valid utf-8 text: {err}"))
    })
}

fn wait_processed_sleep_duration(counts: &QueueCounts, timeout_remaining: Duration) -> Duration {
    if timeout_remaining.is_zero() {
        return Duration::ZERO;
    }

    let fallback_sleep = WAIT_PROCESSED_MIN_SLEEP.min(timeout_remaining);
    if counts.new_due > 0 {
        return fallback_sleep;
    }

    let Some(raw_due_at) = counts.earliest_next_attempt_at.as_deref() else {
        return fallback_sleep;
    };
    let Ok(parsed_due_at) = DateTime::parse_from_rfc3339(raw_due_at) else {
        return fallback_sleep;
    };

    let due_at_utc = parsed_due_at.with_timezone(&Utc);
    let now = Utc::now();
    if due_at_utc <= now {
        return fallback_sleep;
    }

    let Ok(until_due) = due_at_utc.signed_duration_since(now).to_std() else {
        return fallback_sleep;
    };
    until_due
        .clamp(WAIT_PROCESSED_MIN_SLEEP, WAIT_PROCESSED_MAX_SLEEP)
        .min(timeout_remaining)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;

    use chrono::Duration as ChronoDuration;
    use tempfile::tempdir;

    use super::*;
    use crate::client::AxiomMe;
    use crate::models::AddResourceIngestOptions;

    #[test]
    fn read_remote_text_limited_rejects_payload_over_limit() {
        let data = vec![b'a'; MAX_REMOTE_TEXT_BYTES + 1];
        let err = read_remote_text_limited(Cursor::new(data), MAX_REMOTE_TEXT_BYTES)
            .expect_err("must reject oversized payload");
        assert!(matches!(err, AxiomError::Validation(_)));
    }

    #[test]
    fn read_remote_text_limited_accepts_payload_within_limit() {
        let data = b"hello remote".to_vec();
        let text =
            read_remote_text_limited(Cursor::new(data.clone()), MAX_REMOTE_TEXT_BYTES).expect("ok");
        assert_eq!(text, String::from_utf8(data).expect("utf8"));
    }

    #[test]
    fn wait_processed_drains_pending_queue_work() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app new");
        app.initialize().expect("init");

        let src = temp.path().join("wait_processed.txt");
        fs::write(&src, "OAuth wait processed flow").expect("write");
        app.add_resource(
            src.to_str().expect("src"),
            Some("axiom://resources/wait-processed"),
            None,
            None,
            false,
            None,
        )
        .expect("add");

        let status = app.wait_processed(Some(5)).expect("wait");
        assert!(status.semantic.processed >= 1);

        let counts = app.state.queue_counts().expect("queue counts");
        assert_eq!(counts.new_total, 0);
        assert_eq!(counts.processing, 0);
    }

    #[test]
    fn wait_processed_times_out_when_retries_are_backed_off() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app new");
        app.initialize().expect("init");

        app.state
            .enqueue("semantic_scan", "invalid://uri", serde_json::json!({}))
            .expect("enqueue");

        let err = app
            .wait_processed(Some(1))
            .expect_err("must timeout while event is delayed");
        assert!(matches!(err, AxiomError::Conflict(_)));
    }

    #[test]
    fn wait_processed_sleep_duration_uses_timeout_remaining_bound() {
        let counts = QueueCounts {
            new_total: 1,
            new_due: 0,
            processing: 0,
            done: 0,
            dead_letter: 0,
            earliest_next_attempt_at: Some((Utc::now() + ChronoDuration::seconds(30)).to_rfc3339()),
        };

        let sleep = wait_processed_sleep_duration(&counts, Duration::from_millis(250));
        assert_eq!(sleep, Duration::from_millis(250));
    }

    #[test]
    fn wait_processed_sleep_duration_falls_back_for_invalid_due_timestamp() {
        let counts = QueueCounts {
            new_total: 1,
            new_due: 0,
            processing: 0,
            done: 0,
            dead_letter: 0,
            earliest_next_attempt_at: Some("not-a-timestamp".to_string()),
        };

        let sleep = wait_processed_sleep_duration(&counts, Duration::from_secs(5));
        assert_eq!(sleep, WAIT_PROCESSED_MIN_SLEEP);
    }

    #[test]
    fn wait_processed_sleep_duration_prefers_min_when_due_work_exists() {
        let counts = QueueCounts {
            new_total: 2,
            new_due: 1,
            processing: 0,
            done: 0,
            dead_letter: 0,
            earliest_next_attempt_at: Some((Utc::now() + ChronoDuration::seconds(30)).to_rfc3339()),
        };

        let sleep = wait_processed_sleep_duration(&counts, Duration::from_secs(5));
        assert_eq!(sleep, WAIT_PROCESSED_MIN_SLEEP);
    }

    #[test]
    fn add_resource_with_markdown_only_options_filters_non_markdown_and_hidden_entries() {
        let temp = tempdir().expect("tempdir");
        let app = AxiomMe::new(temp.path()).expect("app");
        app.initialize().expect("init");

        let source = temp.path().join("vault");
        fs::create_dir_all(source.join("nested")).expect("mkdir nested");
        fs::create_dir_all(source.join(".obsidian")).expect("mkdir hidden");
        fs::write(source.join("keep.md"), "# keep").expect("write keep");
        fs::write(source.join("nested").join("also.markdown"), "# keep nested")
            .expect("write nested keep");
        fs::write(source.join("drop.json"), "{\"drop\":true}").expect("write drop json");
        fs::write(source.join(".obsidian").join("drop.md"), "# drop hidden")
            .expect("write hidden drop");

        app.add_resource_with_ingest_options(
            source.to_str().expect("source path"),
            Some("axiom://resources/filtered"),
            None,
            None,
            true,
            None,
            AddResourceIngestOptions::markdown_only_defaults(),
        )
        .expect("add filtered");

        let uris = app
            .state
            .list_search_documents()
            .expect("list")
            .into_iter()
            .map(|record| record.uri)
            .collect::<Vec<_>>();
        assert!(
            uris.iter()
                .any(|uri| uri == "axiom://resources/filtered/keep.md")
        );
        assert!(
            uris.iter()
                .any(|uri| uri == "axiom://resources/filtered/nested/also.markdown")
        );
        assert!(
            !uris
                .iter()
                .any(|uri| uri == "axiom://resources/filtered/drop.json")
        );
        assert!(
            !uris
                .iter()
                .any(|uri| uri == "axiom://resources/filtered/.obsidian/drop.md")
        );
    }
}
