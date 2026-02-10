use std::path::Path;
use std::time::Instant;

use crate::context_ops::default_resource_target;
use crate::error::{AxiomError, Result};
use crate::ingest::IngestManager;
use crate::models::{AddResourceResult, GlobResult, QueueStatus};
use crate::pack;
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

impl AxiomMe {
    pub fn add_resource(
        &self,
        path_or_url: &str,
        target: Option<&str>,
        _reason: Option<&str>,
        _instruction: Option<&str>,
        wait: bool,
        _timeout_secs: Option<u64>,
    ) -> Result<AddResourceResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let source = path_or_url.to_string();
        let target_raw = target.map(ToString::to_string);

        let output = (|| -> Result<AddResourceResult> {
            let target_uri = if let Some(t) = target {
                AxiomUri::parse(t)?
            } else {
                default_resource_target(path_or_url)?
            };

            let ingest_manager = IngestManager::new(self.fs.clone(), self.parser_registry.clone());
            let mut ingest = ingest_manager.start_session()?;

            let stage_result =
                if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
                    let response = reqwest::blocking::get(path_or_url);
                    match response {
                        Ok(resp) => match resp.text() {
                            Ok(text) => ingest.stage_text("source.txt", &text),
                            Err(err) => Err(AxiomError::from(err)),
                        },
                        Err(err) => Err(AxiomError::from(err)),
                    }
                } else {
                    let src = Path::new(path_or_url);
                    if !src.exists() {
                        return Err(AxiomError::NotFound(path_or_url.to_string()));
                    }
                    ingest.stage_local_path(src)
                };
            if let Err(err) = stage_result {
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
        })();

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
                    })),
                );
                Err(err)
            }
        }
    }

    pub fn wait_processed(&self, _timeout_secs: Option<u64>) -> Result<QueueStatus> {
        self.state.queue_status()
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

    pub fn rm(&self, uri: &str, recursive: bool) -> Result<()> {
        let uri = AxiomUri::parse(uri)?;
        self.fs.rm(&uri, recursive, false)?;

        let doomed = {
            let mut index = self
                .index
                .write()
                .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?;
            let doomed = index
                .all_records()
                .into_iter()
                .map(|r| r.uri)
                .filter(|u| {
                    AxiomUri::parse(u)
                        .map(|parsed| parsed.starts_with(&uri))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>();

            for d in &doomed {
                index.remove(d);
            }
            doomed
        };
        self.state
            .remove_search_documents_with_prefix(&uri.to_string())?;
        self.try_mirror_delete(&doomed, "rm")?;

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
        self.fs.mv(&from, &to, false)?;
        self.state
            .remove_search_documents_with_prefix(&from.to_string())?;
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
