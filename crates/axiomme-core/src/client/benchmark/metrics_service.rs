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
        p50: percentile_u128(&ordered, 5_000),
        p95: percentile_u128(&ordered, 9_500),
        p99: percentile_u128(&ordered, 9_900),
        avg: average_latency_ms(&ordered),
    }
}

pub(super) fn safe_ratio(num: usize, denom: usize) -> f32 {
    if denom == 0 {
        0.0
    } else {
        usize_to_f32(num) / usize_to_f32(denom)
    }
}

pub(super) fn safe_ratio_f32(num: f32, denom: usize) -> f32 {
    if denom == 0 {
        0.0
    } else {
        num / usize_to_f32(denom)
    }
}

pub(super) fn safe_ratio_u128(num: u128, denom: usize) -> f32 {
    if denom == 0 {
        0.0
    } else {
        u128_to_f32(num) / usize_to_f32(denom)
    }
}

pub(super) fn percent_delta_u128(current: u128, previous: u128) -> Option<f32> {
    if previous == 0 {
        None
    } else {
        let current = u128_to_f32(current);
        let previous = u128_to_f32(previous);
        Some((current - previous) / previous * 100.0)
    }
}

pub(super) fn percent_drop_f32(current: f32, previous: f32) -> Option<f32> {
    if previous <= 0.0 {
        None
    } else {
        Some((previous - current) / previous * 100.0)
    }
}

const fn usize_to_f32(value: usize) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "benchmark ratio math intentionally converts integer counters into f32"
    )]
    {
        value as f32
    }
}

const fn u128_to_f32(value: u128) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "benchmark aggregates are reported as f32 summary metrics"
    )]
    {
        value as f32
    }
}
