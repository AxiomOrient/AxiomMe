use crate::quality::{average_latency_ms, percentile_u128};

pub(super) struct LatencySummary {
    pub p50: u128,
    pub p95: u128,
    pub p99: u128,
    pub avg: f32,
}

pub(super) fn summarize_latencies(latencies: &[u128]) -> LatencySummary {
    let mut ordered = latencies.to_vec();
    ordered.sort_unstable();
    LatencySummary {
        p50: percentile_u128(&ordered, 0.50),
        p95: percentile_u128(&ordered, 0.95),
        p99: percentile_u128(&ordered, 0.99),
        avg: average_latency_ms(&ordered),
    }
}

pub(super) fn safe_ratio(num: usize, denom: usize) -> f32 {
    if denom == 0 {
        0.0
    } else {
        num as f32 / denom as f32
    }
}

pub(super) fn safe_ratio_f32(num: f32, denom: usize) -> f32 {
    if denom == 0 { 0.0 } else { num / denom as f32 }
}

pub(super) fn percent_delta_u128(current: u128, previous: u128) -> Option<f32> {
    if previous == 0 {
        None
    } else {
        Some((current as f32 - previous as f32) / previous as f32 * 100.0)
    }
}

pub(super) fn percent_drop_f32(current: f32, previous: f32) -> Option<f32> {
    if previous <= 0.0 {
        None
    } else {
        Some((previous - current) / previous * 100.0)
    }
}
