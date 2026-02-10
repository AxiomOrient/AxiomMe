use std::fs;

use chrono::Utc;

use crate::catalog::request_log_uri;
use crate::error::{AxiomError, Result};
use crate::models::{
    BackendStatus, EmbeddingBackendStatus, QdrantBackendStatus, QueueDiagnostics, RequestLogEntry,
    SessionInfo,
};
use crate::queue_policy::default_scope_set;
use crate::session::Session;
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

const INDEX_PROFILE_STAMP_KEY: &str = "index_profile_stamp";
const SEARCH_STACK_VERSION: &str = "sqlite-fts5-bm25-v3";

impl AxiomMe {
    pub fn session(&self, session_id: Option<&str>) -> Session {
        let id = session_id
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("s-{}", uuid::Uuid::new_v4().simple()));
        Session::new(
            id,
            self.fs.clone(),
            self.state.clone(),
            self.index.clone(),
            self.qdrant.clone(),
        )
    }

    pub fn backend_status(&self) -> Result<BackendStatus> {
        let local_records = self
            .index
            .read()
            .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?
            .all_records()
            .len();

        let qdrant = if let Some(mirror) = &self.qdrant {
            match mirror.health() {
                Ok(healthy) => Some(QdrantBackendStatus {
                    enabled: true,
                    base_url: mirror.config().base_url.clone(),
                    collection: mirror.config().collection.clone(),
                    healthy,
                    last_error: None,
                }),
                Err(err) => Some(QdrantBackendStatus {
                    enabled: true,
                    base_url: mirror.config().base_url.clone(),
                    collection: mirror.config().collection.clone(),
                    healthy: false,
                    last_error: Some(err.to_string()),
                }),
            }
        } else {
            None
        };
        let embed = crate::embedding::embedding_profile();

        Ok(BackendStatus {
            local_records,
            embedding: EmbeddingBackendStatus {
                provider: embed.provider,
                vector_version: embed.vector_version,
                dim: embed.dim,
            },
            qdrant,
        })
    }

    pub fn queue_diagnostics(&self) -> Result<QueueDiagnostics> {
        Ok(QueueDiagnostics {
            counts: self.state.queue_counts()?,
            checkpoints: self.state.list_checkpoints()?,
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
            .map(|x| x.to_ascii_lowercase());
        let status = status
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(|x| x.to_ascii_lowercase());
        let mut entries = raw
            .lines()
            .filter_map(|line| {
                if line.trim().is_empty() {
                    None
                } else {
                    serde_json::from_str::<RequestLogEntry>(line).ok()
                }
            })
            .filter(|entry| {
                if let Some(op) = operation.as_deref()
                    && !entry.operation.eq_ignore_ascii_case(op)
                {
                    return false;
                }
                if let Some(st) = status.as_deref()
                    && !entry.status.eq_ignore_ascii_case(st)
                {
                    return false;
                }
                true
            })
            .collect::<Vec<_>>();
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
            out.push(SessionInfo {
                session_id: entry.name.clone(),
                uri: entry.uri,
                updated_at: Utc::now(),
            });
        }
        out.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(out)
    }

    pub fn delete(&self, session_id: &str) -> Result<bool> {
        let uri = AxiomUri::root(Scope::Session).join(session_id)?;
        if !self.fs.exists(&uri) {
            return Ok(false);
        }
        self.fs.rm(&uri, true, true)?;
        Ok(true)
    }

    pub fn reindex_all(&self) -> Result<()> {
        self.state.clear_search_index()?;
        {
            let mut index = self
                .index
                .write()
                .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
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
        let count = records.len();
        let mut index = self
            .index
            .write()
            .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
        index.clear();
        for record in records {
            index.upsert(record);
        }
        Ok(count)
    }

    fn current_index_profile_stamp(&self) -> String {
        let embed = crate::embedding::embedding_profile();
        let qdrant = self
            .qdrant
            .as_ref()
            .map(|mirror| {
                format!(
                    "{}|{}",
                    mirror.config().base_url,
                    mirror.config().collection
                )
            })
            .unwrap_or_else(|| "disabled".to_string());
        format!(
            "stack:{};embed:{}@{}:{};qdrant:{}",
            SEARCH_STACK_VERSION, embed.provider, embed.vector_version, embed.dim, qdrant
        )
    }

    fn has_index_state_drift(&self) -> Result<bool> {
        for (uri, stored_mtime) in self.state.list_index_state_entries()? {
            let parsed = match AxiomUri::parse(&uri) {
                Ok(value) => value,
                Err(_) => return Ok(true),
            };
            let path = self.fs.resolve_uri(&parsed);
            if !path.exists() {
                return Ok(true);
            }

            let mtime = fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0);
            if mtime != stored_mtime {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
