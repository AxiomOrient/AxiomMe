use std::collections::HashMap;

use chrono::Utc;

use crate::catalog::{
    parse_trace_metrics_snapshot_document, trace_metrics_snapshot_uri, trace_metrics_snapshots_uri,
};
use crate::error::Result;
use crate::models::{
    TraceIndexEntry, TraceMetricsReport, TraceMetricsSample, TraceMetricsSnapshotDocument,
    TraceMetricsSnapshotSummary, TraceMetricsTrendReport, TraceRequestTypeMetrics,
};
use crate::quality::{percentile_u128, to_trace_metrics_snapshot_summary};
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub fn list_traces(&self, limit: usize) -> Result<Vec<TraceIndexEntry>> {
        self.state.list_trace_index(limit.max(1))
    }

    pub fn trace_metrics(&self, limit: usize, include_replays: bool) -> Result<TraceMetricsReport> {
        let entries = self.list_traces(limit.max(1))?;
        let mut analyzed = Vec::<TraceMetricsSample>::new();
        let mut skipped_missing = 0usize;
        let mut skipped_invalid = 0usize;

        for entry in &entries {
            if !include_replays && entry.request_type.ends_with("_replay") {
                continue;
            }

            match self.get_trace(&entry.trace_id) {
                Ok(Some(trace)) => {
                    analyzed.push(TraceMetricsSample {
                        trace_id: entry.trace_id.clone(),
                        request_type: entry.request_type.clone(),
                        latency_ms: trace.metrics.latency_ms,
                        explored_nodes: trace.metrics.explored_nodes,
                        convergence_rounds: trace.metrics.convergence_rounds,
                        created_at: entry.created_at.clone(),
                    });
                }
                Ok(None) => skipped_missing += 1,
                Err(_) => skipped_invalid += 1,
            }
        }

        let mut grouped =
            HashMap::<String, (Vec<u128>, usize, usize, u32)>::with_capacity(analyzed.len());
        for item in &analyzed {
            let row = grouped
                .entry(item.request_type.clone())
                .or_insert_with(|| (Vec::new(), 0, 0, 0));
            row.0.push(item.latency_ms);
            row.1 += 1;
            row.2 += item.explored_nodes;
            row.3 += item.convergence_rounds;
        }

        let mut by_request_type = grouped
            .into_iter()
            .map(
                |(request_type, (mut latencies, traces, explored_sum, convergence_sum))| {
                    latencies.sort_unstable();
                    let avg_latency_ms = if traces == 0 {
                        0.0
                    } else {
                        latencies.iter().copied().sum::<u128>() as f32 / traces as f32
                    };
                    let avg_explored_nodes = if traces == 0 {
                        0.0
                    } else {
                        explored_sum as f32 / traces as f32
                    };
                    let avg_convergence_rounds = if traces == 0 {
                        0.0
                    } else {
                        convergence_sum as f32 / traces as f32
                    };
                    TraceRequestTypeMetrics {
                        request_type,
                        traces,
                        p50_latency_ms: percentile_u128(&latencies, 0.50),
                        p95_latency_ms: percentile_u128(&latencies, 0.95),
                        avg_latency_ms,
                        avg_explored_nodes,
                        avg_convergence_rounds,
                    }
                },
            )
            .collect::<Vec<_>>();
        by_request_type.sort_by(|a, b| {
            b.traces
                .cmp(&a.traces)
                .then_with(|| a.request_type.cmp(&b.request_type))
        });

        let mut slowest_samples = analyzed.clone();
        slowest_samples.sort_by(|a, b| {
            b.latency_ms
                .cmp(&a.latency_ms)
                .then_with(|| b.created_at.cmp(&a.created_at))
        });
        slowest_samples.truncate(20);

        Ok(TraceMetricsReport {
            window_limit: limit.max(1),
            include_replays,
            indexed_traces_scanned: entries.len(),
            traces_analyzed: analyzed.len(),
            traces_skipped_missing: skipped_missing,
            traces_skipped_invalid: skipped_invalid,
            by_request_type,
            slowest_samples,
        })
    }

    pub fn create_trace_metrics_snapshot(
        &self,
        limit: usize,
        include_replays: bool,
    ) -> Result<TraceMetricsSnapshotSummary> {
        let report = self.trace_metrics(limit.max(1), include_replays)?;
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let uri = trace_metrics_snapshot_uri(&snapshot_id)?;
        let report_uri = uri.to_string();
        let doc = TraceMetricsSnapshotDocument {
            version: 1,
            snapshot_id,
            created_at,
            report,
        };
        self.fs
            .write(&uri, &serde_json::to_string_pretty(&doc)?, true)?;
        Ok(to_trace_metrics_snapshot_summary(&doc, &report_uri))
    }

    pub fn list_trace_metrics_snapshots(
        &self,
        limit: usize,
    ) -> Result<Vec<TraceMetricsSnapshotSummary>> {
        let docs = self.list_trace_metrics_snapshot_documents(limit.max(1))?;
        Ok(docs
            .into_iter()
            .map(|(report_uri, doc)| to_trace_metrics_snapshot_summary(&doc, &report_uri))
            .collect())
    }

    pub fn trace_metrics_trend(
        &self,
        limit: usize,
        request_type: Option<&str>,
    ) -> Result<TraceMetricsTrendReport> {
        let request_type = request_type
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .unwrap_or("find")
            .to_ascii_lowercase();
        let docs = self.list_trace_metrics_snapshot_documents(limit.max(2))?;
        if docs.is_empty() {
            return Ok(TraceMetricsTrendReport {
                request_type,
                latest: None,
                previous: None,
                latest_p95_latency_ms: None,
                previous_p95_latency_ms: None,
                delta_p95_latency_ms: None,
                latest_avg_explored_nodes: None,
                previous_avg_explored_nodes: None,
                delta_avg_explored_nodes: None,
                status: "no_data".to_string(),
            });
        }

        let latest = docs
            .first()
            .map(|(uri, doc)| to_trace_metrics_snapshot_summary(doc, uri));
        let previous = docs
            .get(1)
            .map(|(uri, doc)| to_trace_metrics_snapshot_summary(doc, uri));
        let request_type_match = request_type.as_str();
        let latest_metrics = docs.first().and_then(|(_, doc)| {
            doc.report
                .by_request_type
                .iter()
                .find(|x| x.request_type == request_type_match)
        });
        let previous_metrics = docs.get(1).and_then(|(_, doc)| {
            doc.report
                .by_request_type
                .iter()
                .find(|x| x.request_type == request_type_match)
        });

        let latest_p95_latency_ms = latest_metrics.map(|x| x.p95_latency_ms);
        let previous_p95_latency_ms = previous_metrics.map(|x| x.p95_latency_ms);
        let delta_p95_latency_ms = match (latest_p95_latency_ms, previous_p95_latency_ms) {
            (Some(l), Some(p)) => Some(l as i128 - p as i128),
            _ => None,
        };

        let latest_avg_explored_nodes = latest_metrics.map(|x| x.avg_explored_nodes);
        let previous_avg_explored_nodes = previous_metrics.map(|x| x.avg_explored_nodes);
        let delta_avg_explored_nodes =
            match (latest_avg_explored_nodes, previous_avg_explored_nodes) {
                (Some(l), Some(p)) => Some(l - p),
                _ => None,
            };

        let status = if previous.is_none() {
            "insufficient_history".to_string()
        } else if latest_metrics.is_none() || previous_metrics.is_none() {
            "missing_request_type".to_string()
        } else if let Some(delta) = delta_p95_latency_ms {
            if delta < 0 {
                "improved".to_string()
            } else if delta > 0 {
                "regressed".to_string()
            } else {
                "stable".to_string()
            }
        } else {
            "mixed".to_string()
        };

        Ok(TraceMetricsTrendReport {
            request_type,
            latest,
            previous,
            latest_p95_latency_ms,
            previous_p95_latency_ms,
            delta_p95_latency_ms,
            latest_avg_explored_nodes,
            previous_avg_explored_nodes,
            delta_avg_explored_nodes,
            status,
        })
    }

    fn list_trace_metrics_snapshot_documents(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, TraceMetricsSnapshotDocument)>> {
        let limit = limit.max(1);
        let dir = trace_metrics_snapshots_uri()?;
        if !self.fs.exists(&dir) {
            return Ok(Vec::new());
        }
        let entries = self.fs.list(&dir, false)?;
        let mut docs = Vec::<(String, TraceMetricsSnapshotDocument)>::new();
        for entry in entries {
            if entry.is_dir || !entry.name.ends_with(".json") {
                continue;
            }
            let uri = AxiomUri::parse(&entry.uri)?;
            let raw = self.fs.read(&uri)?;
            let Ok(doc) = parse_trace_metrics_snapshot_document(&raw) else {
                continue;
            };
            docs.push((uri.to_string(), doc));
        }
        docs.sort_by(|a, b| {
            b.1.created_at
                .cmp(&a.1.created_at)
                .then_with(|| b.1.snapshot_id.cmp(&a.1.snapshot_id))
        });
        docs.truncate(limit);
        Ok(docs)
    }
}
