use std::time::Instant;

use chrono::Utc;

use crate::error::{AxiomError, Result};
use crate::models::{FindResult, RequestLogEntry, RetrievalTrace, SearchOptions};
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub fn get_trace(&self, trace_id: &str) -> Result<Option<RetrievalTrace>> {
        let Some(entry) = self.state.get_trace_index(trace_id)? else {
            return Ok(None);
        };
        let uri = AxiomUri::parse(&entry.uri)?;
        let raw = self.fs.read(&uri)?;
        let trace = serde_json::from_str::<RetrievalTrace>(&raw)?;
        Ok(Some(trace))
    }

    pub fn replay_trace(&self, trace_id: &str, limit: Option<usize>) -> Result<Option<FindResult>> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();

        let output = (|| -> Result<Option<FindResult>> {
            let Some(stored_trace) = self.get_trace(trace_id)? else {
                return Ok(None);
            };

            let target_uri = stored_trace
                .target_uri
                .as_deref()
                .map(AxiomUri::parse)
                .transpose()?;
            let replay_limit = limit.unwrap_or_else(|| stored_trace.final_topk.len().max(1));
            let request_type = format!("{}_replay", stored_trace.request_type);

            let mut result = {
                let index = self
                    .index
                    .read()
                    .map_err(|_| AxiomError::lock_poisoned("index"))?;
                let options = SearchOptions {
                    query: stored_trace.query,
                    target_uri,
                    session: None,
                    session_hints: Vec::new(),
                    budget: None,
                    limit: replay_limit,
                    score_threshold: None,
                    min_match_tokens: None,
                    filter: None,
                    request_type,
                };
                self.drr.run(&index, &options)
            };
            self.persist_trace_result(&mut result)?;
            Ok(Some(result))
        })();

        match output {
            Ok(Some(result)) => {
                let replay_trace_id = result.trace.as_ref().map(|x| x.trace_id.clone());
                self.try_log_request(&RequestLogEntry {
                    request_id,
                    operation: "trace.replay".to_string(),
                    status: "ok".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id: replay_trace_id,
                    target_uri: None,
                    error_code: None,
                    error_message: None,
                    details: Some(serde_json::json!({
                        "source_trace_id": trace_id,
                        "limit": limit,
                    })),
                });
                Ok(Some(result))
            }
            Ok(None) => {
                self.try_log_request(&RequestLogEntry {
                    request_id,
                    operation: "trace.replay".to_string(),
                    status: "not_found".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id: None,
                    target_uri: None,
                    error_code: None,
                    error_message: None,
                    details: Some(serde_json::json!({
                        "source_trace_id": trace_id,
                        "limit": limit,
                    })),
                });
                Ok(None)
            }
            Err(err) => {
                self.try_log_request(&RequestLogEntry {
                    request_id,
                    operation: "trace.replay".to_string(),
                    status: "error".to_string(),
                    latency_ms: started.elapsed().as_millis(),
                    created_at: Utc::now().to_rfc3339(),
                    trace_id: None,
                    target_uri: None,
                    error_code: Some(err.code().to_string()),
                    error_message: Some(err.to_string()),
                    details: Some(serde_json::json!({
                        "source_trace_id": trace_id,
                        "limit": limit,
                    })),
                });
                Err(err)
            }
        }
    }
}
