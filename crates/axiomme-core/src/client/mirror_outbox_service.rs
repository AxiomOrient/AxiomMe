use crate::error::{AxiomError, Result};
use crate::models::{IndexRecord, OutboxEvent};
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub(super) fn ensure_qdrant_collection(&self) -> Result<()> {
        if let Err(err) = self.ensure_qdrant_collection_strict() {
            let collection = self
                .qdrant
                .as_ref()
                .map(|q| q.config().collection.clone())
                .unwrap_or_else(|| "disabled".to_string());
            let base_url = self
                .qdrant
                .as_ref()
                .map(|q| q.config().base_url.clone())
                .unwrap_or_default();
            let id = self.state.enqueue(
                "qdrant_ensure_collection_failed",
                &format!("axiom://queue/qdrant/{}", collection),
                serde_json::json!({
                    "error": err.to_string(),
                    "base_url": base_url,
                    "collection": collection,
                }),
            )?;
            self.state.mark_outbox_status(id, "dead_letter", true)?;
        }
        Ok(())
    }

    pub(super) fn try_mirror_upsert(&self, record: &IndexRecord, source: &str) -> Result<()> {
        if let Err(err) = self.mirror_upsert_strict(record) {
            let collection = self
                .qdrant
                .as_ref()
                .map(|q| q.config().collection.clone())
                .unwrap_or_else(|| "disabled".to_string());
            let id = self.state.enqueue(
                "qdrant_upsert_failed",
                &record.uri,
                serde_json::json!({
                    "error": err.to_string(),
                    "source": source,
                    "collection": collection,
                }),
            )?;
            self.state.mark_outbox_status(id, "dead_letter", true)?;
        }
        Ok(())
    }

    pub(super) fn try_mirror_delete(&self, uris: &[String], source: &str) -> Result<()> {
        if let Err(err) = self.mirror_delete_strict(uris) {
            let collection = self
                .qdrant
                .as_ref()
                .map(|q| q.config().collection.clone())
                .unwrap_or_else(|| "disabled".to_string());
            let id = self.state.enqueue(
                "qdrant_delete_failed",
                &format!("axiom://queue/qdrant/{}", collection),
                serde_json::json!({
                    "error": err.to_string(),
                    "source": source,
                    "count": uris.len(),
                    "uris": uris,
                }),
            )?;
            self.state.mark_outbox_status(id, "dead_letter", true)?;
        }
        Ok(())
    }

    pub(super) fn mirror_upsert_strict(&self, record: &IndexRecord) -> Result<()> {
        let Some(qdrant) = &self.qdrant else {
            return Ok(());
        };
        qdrant.upsert_record(record)
    }

    pub(super) fn mirror_delete_strict(&self, uris: &[String]) -> Result<()> {
        let Some(qdrant) = &self.qdrant else {
            return Ok(());
        };
        qdrant.delete_uris(uris)
    }

    pub(super) fn ensure_qdrant_collection_strict(&self) -> Result<()> {
        let Some(qdrant) = &self.qdrant else {
            return Ok(());
        };
        qdrant.ensure_collection()
    }

    pub(super) fn handle_outbox_event(&self, event: &OutboxEvent) -> Result<bool> {
        match event.event_type.as_str() {
            "semantic_scan" => {
                let target = AxiomUri::parse(&event.uri)?;
                self.ensure_tiers_recursive(&target)?;
                self.reindex_uri_tree(&target)?;
                Ok(true)
            }
            "upsert" | "reindex" => {
                let record = self
                    .index
                    .read()
                    .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?
                    .get(&event.uri)
                    .cloned();
                if let Some(record) = record {
                    self.mirror_upsert_strict(&record)?;
                    Ok(true)
                } else {
                    self.mirror_delete_strict(std::slice::from_ref(&event.uri))?;
                    Ok(false)
                }
            }
            "delete" => {
                self.mirror_delete_strict(std::slice::from_ref(&event.uri))?;
                Ok(true)
            }
            "qdrant_ensure_collection_failed" => {
                self.ensure_qdrant_collection_strict()?;
                Ok(true)
            }
            "qdrant_upsert_failed" => {
                let record = self
                    .index
                    .read()
                    .map_err(|_| AxiomError::Internal("index lock poisoned".to_string()))?
                    .get(&event.uri)
                    .cloned();
                if let Some(record) = record {
                    self.mirror_upsert_strict(&record)?;
                } else {
                    self.mirror_delete_strict(std::slice::from_ref(&event.uri))?;
                }
                Ok(true)
            }
            "qdrant_delete_failed" => {
                let uris = event
                    .payload_json
                    .get("uris")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(ToString::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![event.uri.clone()]);
                self.mirror_delete_strict(&uris)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
