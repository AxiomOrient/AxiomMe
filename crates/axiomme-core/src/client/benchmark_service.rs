use chrono::Utc;

use crate::catalog::{benchmark_base_uri, benchmark_fixture_uri, parse_benchmark_fixture_document};
use crate::error::{AxiomError, Result};
use crate::models::{
    BenchmarkFixtureDocument, BenchmarkFixtureSummary, BenchmarkReport, BenchmarkRunOptions,
    BenchmarkTrendReport,
};
use crate::quality::to_benchmark_summary;
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub fn create_benchmark_fixture(
        &self,
        name: &str,
        query_limit: usize,
        include_golden: bool,
        include_trace: bool,
    ) -> Result<BenchmarkFixtureSummary> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(AxiomError::Validation(
                "fixture name cannot be empty".to_string(),
            ));
        }
        if !include_golden && !include_trace {
            return Err(AxiomError::Validation(
                "fixture creation requires at least one source".to_string(),
            ));
        }

        let options = BenchmarkRunOptions {
            query_limit,
            search_limit: 10,
            include_golden,
            include_trace,
            fixture_name: None,
        };
        let cases = self.collect_benchmark_query_cases(&options, query_limit.max(1))?;
        let fixture_uri = benchmark_fixture_uri(trimmed)?;
        let document = BenchmarkFixtureDocument {
            version: 1,
            created_at: Utc::now().to_rfc3339(),
            name: trimmed.to_string(),
            cases,
        };
        self.fs.write(
            &fixture_uri,
            &serde_json::to_string_pretty(&document)?,
            true,
        )?;

        Ok(BenchmarkFixtureSummary {
            name: document.name,
            uri: fixture_uri.to_string(),
            case_count: document.cases.len(),
            created_at: document.created_at,
        })
    }

    pub fn list_benchmark_fixtures(&self, limit: usize) -> Result<Vec<BenchmarkFixtureSummary>> {
        let limit = limit.max(1);
        let fixtures_dir = benchmark_base_uri()?.join("fixtures")?;
        if !self.fs.exists(&fixtures_dir) {
            return Ok(Vec::new());
        }

        let entries = self.fs.list(&fixtures_dir, false)?;
        let mut out = Vec::<BenchmarkFixtureSummary>::new();
        for entry in entries {
            if entry.is_dir || !entry.name.ends_with(".json") {
                continue;
            }
            let uri = AxiomUri::parse(&entry.uri)?;
            let raw = self.fs.read(&uri)?;
            let doc = parse_benchmark_fixture_document(&raw)?;
            out.push(BenchmarkFixtureSummary {
                name: doc.name,
                uri: uri.to_string(),
                case_count: doc.cases.len(),
                created_at: doc.created_at,
            });
        }
        out.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.name.cmp(&b.name))
        });
        out.truncate(limit);
        Ok(out)
    }

    pub fn list_benchmark_reports(&self, limit: usize) -> Result<Vec<BenchmarkReport>> {
        let limit = limit.max(1);
        let reports_dir = benchmark_base_uri()?.join("reports")?;
        if !self.fs.exists(&reports_dir) {
            return Ok(Vec::new());
        }
        let entries = self.fs.list(&reports_dir, false)?;
        let mut reports = Vec::<BenchmarkReport>::new();
        for entry in entries {
            if entry.is_dir || !entry.name.ends_with(".json") {
                continue;
            }
            let uri = AxiomUri::parse(&entry.uri)?;
            let raw = self.fs.read(&uri)?;
            let Ok(report) = serde_json::from_str::<BenchmarkReport>(&raw) else {
                continue;
            };
            reports.push(report);
        }
        reports.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.run_id.cmp(&a.run_id))
        });
        reports.truncate(limit);
        Ok(reports)
    }

    pub fn benchmark_trend(&self, limit: usize) -> Result<BenchmarkTrendReport> {
        let reports = self.list_benchmark_reports(limit.max(2))?;
        if reports.is_empty() {
            return Ok(BenchmarkTrendReport {
                latest: None,
                previous: None,
                delta_p95_latency_ms: None,
                delta_top1_accuracy: None,
                status: "no_data".to_string(),
            });
        }

        let latest = reports.first().cloned().map(to_benchmark_summary);
        let previous = reports.get(1).cloned().map(to_benchmark_summary);
        let delta_p95_latency_ms = match (latest.as_ref(), previous.as_ref()) {
            (Some(l), Some(p)) => Some(l.p95_latency_ms as i128 - p.p95_latency_ms as i128),
            _ => None,
        };
        let delta_top1_accuracy = match (latest.as_ref(), previous.as_ref()) {
            (Some(l), Some(p)) => Some(l.top1_accuracy - p.top1_accuracy),
            _ => None,
        };

        let status = match (delta_p95_latency_ms, delta_top1_accuracy) {
            (None, None) => "insufficient_history",
            (Some(dp95), Some(dacc)) if dp95 <= 0 && dacc >= 0.0 => "improved",
            (Some(dp95), Some(dacc)) if dp95 > 0 && dacc < 0.0 => "regressed",
            _ => "mixed",
        }
        .to_string();

        Ok(BenchmarkTrendReport {
            latest,
            previous,
            delta_p95_latency_ms,
            delta_top1_accuracy,
            status,
        })
    }
}
