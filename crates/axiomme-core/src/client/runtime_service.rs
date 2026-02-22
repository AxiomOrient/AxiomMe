use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Utc};

use crate::catalog::request_log_uri;
use crate::config::{RETRIEVAL_BACKEND_MEMORY, RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY};
use crate::error::{AxiomError, Result};
use crate::jsonl::{jsonl_all_lines_invalid, parse_jsonl_tolerant};
use crate::models::{
    BackendStatus, CommitMode, CommitResult, EmbeddingBackendStatus, MemoryPromotionRequest,
    MemoryPromotionResult, QueueDiagnostics, QueueOverview, RequestLogEntry, SessionInfo,
    SessionMeta,
};
use crate::queue_policy::default_scope_set;
use crate::session::Session;
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

const INDEX_PROFILE_STAMP_KEY: &str = "index_profile_stamp";
const SEARCH_STACK_VERSION: &str = "drr-memory-v1";
impl AxiomMe {
    pub fn session(&self, session_id: Option<&str>) -> Session {
        let id = session_id.map_or_else(
            || format!("s-{}", uuid::Uuid::new_v4().simple()),
            ToString::to_string,
        );
        Session::new(id, self.fs.clone(), self.state.clone(), self.index.clone())
            .with_config(self.config.clone())
    }

    pub fn backend_status(&self) -> Result<BackendStatus> {
        let local_records = self
            .index
            .read()
            .map_err(|_| AxiomError::lock_poisoned("index"))?
            .all_records()
            .len();

        let embed = crate::embedding::embedding_profile();

        Ok(BackendStatus {
            local_records,
            retrieval_backend: RETRIEVAL_BACKEND_MEMORY.to_string(),
            retrieval_backend_policy: RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY.to_string(),
            embedding: EmbeddingBackendStatus {
                provider: embed.provider,
                vector_version: embed.vector_version,
                dim: embed.dim,
            },
        })
    }

    pub fn queue_diagnostics(&self) -> Result<QueueDiagnostics> {
        let queue_dead_letter_rate = self
            .state
            .queue_dead_letter_rates_by_event_type()?
            .into_iter()
            .filter(is_om_event_type)
            .collect::<Vec<_>>();
        Ok(QueueDiagnostics {
            counts: self.state.queue_counts()?,
            checkpoints: self.state.list_checkpoints()?,
            queue_dead_letter_rate,
            om_status: self.state.om_status_snapshot()?,
            om_reflection_apply_metrics: self.state.om_reflection_apply_metrics_snapshot()?,
        })
    }

    pub fn queue_overview(&self) -> Result<QueueOverview> {
        let (counts, lanes) = self.state.queue_snapshot()?;
        let queue_dead_letter_rate = self
            .state
            .queue_dead_letter_rates_by_event_type()?
            .into_iter()
            .filter(is_om_event_type)
            .collect::<Vec<_>>();
        Ok(QueueOverview {
            counts,
            checkpoints: self.state.list_checkpoints()?,
            lanes,
            queue_dead_letter_rate,
            om_status: self.state.om_status_snapshot()?,
            om_reflection_apply_metrics: self.state.om_reflection_apply_metrics_snapshot()?,
        })
    }

    pub fn list_request_logs(&self, limit: usize) -> Result<Vec<RequestLogEntry>> {
        self.list_request_logs_filtered(limit, None, None)
    }

    pub fn list_request_logs_filtered(
        &self,
        limit: usize,
        operation: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<RequestLogEntry>> {
        let uri = request_log_uri()?;
        if !self.fs.exists(&uri) {
            return Ok(Vec::new());
        }
        let raw = self.fs.read(&uri)?;
        let operation = operation
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_ascii_lowercase);
        let status = status
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_ascii_lowercase);
        let parsed = parse_jsonl_tolerant::<RequestLogEntry>(&raw);
        if parsed.items.is_empty() && parsed.skipped_lines > 0 {
            return Err(jsonl_all_lines_invalid(
                "request log",
                None,
                parsed.skipped_lines,
                parsed.first_error.as_ref(),
            ));
        }

        let mut entries = Vec::new();
        for entry in parsed.items {
            if let Some(op) = operation.as_deref()
                && !entry.operation.eq_ignore_ascii_case(op)
            {
                continue;
            }
            if let Some(st) = status.as_deref()
                && !entry.status.eq_ignore_ascii_case(st)
            {
                continue;
            }
            entries.push(entry);
        }
        entries.reverse();
        entries.truncate(limit.max(1));
        Ok(entries)
    }

    pub fn sessions(&self) -> Result<Vec<SessionInfo>> {
        let root = AxiomUri::root(Scope::Session);
        let mut out = Vec::new();
        for entry in self.fs.list(&root, false)? {
            if !entry.is_dir {
                continue;
            }
            let session_uri = AxiomUri::parse(&entry.uri)?;
            out.push(SessionInfo {
                session_id: entry.name.clone(),
                uri: entry.uri,
                updated_at: self.session_updated_at(&session_uri),
            });
        }
        out.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(out)
    }

    pub fn promote_session_memories(
        &self,
        request: &MemoryPromotionRequest,
    ) -> Result<MemoryPromotionResult> {
        let session = self.session(Some(&request.session_id));
        session.load()?;
        session.promote_memories(request)
    }

    pub fn checkpoint_session_archive_only(&self, session_id: &str) -> Result<CommitResult> {
        let session = self.session(Some(session_id));
        session.load()?;
        session.commit_with_mode(CommitMode::ArchiveOnly)
    }

    pub fn promote_and_checkpoint_archive_only(
        &self,
        request: &MemoryPromotionRequest,
    ) -> Result<MemoryPromotionResult> {
        let session = self.session(Some(&request.session_id));
        session.load()?;
        let result = session.promote_memories(request)?;
        let _ = session.commit_with_mode(CommitMode::ArchiveOnly)?;
        Ok(result)
    }

    pub fn delete(&self, session_id: &str) -> Result<bool> {
        let uri = AxiomUri::root(Scope::Session).join(session_id)?;
        if !self.fs.exists(&uri) {
            return Ok(false);
        }
        self.fs.rm(&uri, true, true)?;
        self.prune_index_prefix_from_memory(&uri)?;
        self.state
            .remove_search_documents_with_prefix(&uri.to_string())?;
        self.state
            .remove_index_state_with_prefix(&uri.to_string())?;
        let _ = self
            .state
            .remove_promotion_checkpoints_for_session(session_id)?;
        Ok(true)
    }

    pub fn reindex_all(&self) -> Result<()> {
        self.state.clear_search_index()?;
        self.state.clear_index_state()?;
        {
            let mut index = self
                .index
                .write()
                .map_err(|_| AxiomError::lock_poisoned("index"))?;
            index.clear();
        }
        self.reindex_scopes(&default_scope_set())?;
        self.state
            .set_system_value(INDEX_PROFILE_STAMP_KEY, &self.current_index_profile_stamp())?;
        Ok(())
    }

    pub(super) fn initialize_runtime_index(&self) -> Result<()> {
        let current_stamp = self.current_index_profile_stamp();
        let stored_stamp = self.state.get_system_value(INDEX_PROFILE_STAMP_KEY)?;

        if stored_stamp.as_deref() != Some(current_stamp.as_str()) {
            self.reindex_all()?;
            return Ok(());
        }

        if self.has_index_state_drift()? {
            self.reindex_all()?;
            return Ok(());
        }

        let restored = self.restore_index_from_state()?;
        if restored == 0 {
            self.reindex_all()?;
        }
        Ok(())
    }

    fn restore_index_from_state(&self) -> Result<usize> {
        let records = self.state.list_search_documents()?;
        let mut restored = 0usize;
        let mut index = self
            .index
            .write()
            .map_err(|_| AxiomError::lock_poisoned("index"))?;
        index.clear();
        for record in records {
            let Ok(uri) = AxiomUri::parse(&record.uri) else {
                continue;
            };
            if uri.scope().is_internal() {
                continue;
            }
            index.upsert(record);
            restored = restored.saturating_add(1);
        }
        drop(index);
        Ok(restored)
    }

    fn current_index_profile_stamp(&self) -> String {
        let embed = crate::embedding::embedding_profile();
        format!(
            "stack:{};embed:{}@{}:{}",
            SEARCH_STACK_VERSION, embed.provider, embed.vector_version, embed.dim
        )
    }

    fn has_index_state_drift(&self) -> Result<bool> {
        for (uri, stored_mtime) in self.state.list_index_state_entries()? {
            let Ok(parsed) = AxiomUri::parse(&uri) else {
                return Ok(true);
            };
            let path = self.fs.resolve_uri(&parsed);
            if !path.exists() {
                return Ok(true);
            }

            let mtime = metadata_mtime_nanos(&path);
            if mtime != stored_mtime {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn session_updated_at(&self, session_uri: &AxiomUri) -> DateTime<Utc> {
        let session_path = self.fs.resolve_uri(session_uri);
        let meta_path = session_path.join(".meta.json");
        if let Ok(raw_meta) = fs::read_to_string(&meta_path)
            && let Ok(meta) = serde_json::from_str::<SessionMeta>(&raw_meta)
        {
            return meta.updated_at;
        }

        fs::metadata(&session_path)
            .and_then(|m| m.modified())
            .map_or_else(|_| Utc::now(), DateTime::<Utc>::from)
    }
}

fn saturating_duration_nanos_to_i64(duration: std::time::Duration) -> i64 {
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}

fn metadata_mtime_nanos(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, saturating_duration_nanos_to_i64)
}

fn is_om_event_type(rate: &crate::models::QueueDeadLetterRate) -> bool {
    rate.event_type.starts_with("om_")
}
