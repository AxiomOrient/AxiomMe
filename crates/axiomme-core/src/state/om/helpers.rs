use chrono::{DateTime, Utc};
use rusqlite::{params, types::Type};

use crate::error::Result;

use super::OmReflectionApplyOutcome;

pub(super) fn update_reflection_apply_metrics_tx(
    tx: &rusqlite::Transaction<'_>,
    outcome: OmReflectionApplyOutcome,
    latency_ms: u64,
) -> Result<()> {
    let (applied_delta, stale_delta, idempotent_delta) = match outcome {
        OmReflectionApplyOutcome::Applied => (1u64, 0u64, 0u64),
        OmReflectionApplyOutcome::StaleGeneration => (0u64, 1u64, 0u64),
        OmReflectionApplyOutcome::IdempotentEvent => (0u64, 0u64, 1u64),
    };
    tx.execute(
        r"
        INSERT INTO om_runtime_metrics(
            id,
            reflect_apply_attempts_total,
            reflect_apply_applied_total,
            reflect_apply_stale_generation_total,
            reflect_apply_idempotent_total,
            reflect_apply_latency_ms_total,
            reflect_apply_latency_ms_max,
            updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(id) DO UPDATE SET
            reflect_apply_attempts_total = om_runtime_metrics.reflect_apply_attempts_total + excluded.reflect_apply_attempts_total,
            reflect_apply_applied_total = om_runtime_metrics.reflect_apply_applied_total + excluded.reflect_apply_applied_total,
            reflect_apply_stale_generation_total = om_runtime_metrics.reflect_apply_stale_generation_total + excluded.reflect_apply_stale_generation_total,
            reflect_apply_idempotent_total = om_runtime_metrics.reflect_apply_idempotent_total + excluded.reflect_apply_idempotent_total,
            reflect_apply_latency_ms_total = om_runtime_metrics.reflect_apply_latency_ms_total + excluded.reflect_apply_latency_ms_total,
            reflect_apply_latency_ms_max = MAX(om_runtime_metrics.reflect_apply_latency_ms_max, excluded.reflect_apply_latency_ms_max),
            updated_at = excluded.updated_at
        ",
        params![
            1i64,
            1i64,
            u64_to_i64_saturating(applied_delta),
            u64_to_i64_saturating(stale_delta),
            u64_to_i64_saturating(idempotent_delta),
            u64_to_i64_saturating(latency_ms),
            u64_to_i64_saturating(latency_ms),
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub(super) fn elapsed_millis_u64(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn u64_to_i64_saturating(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn i64_to_u32_saturating(value: i64) -> u32 {
    if value <= 0 {
        0
    } else {
        u32::try_from(value).unwrap_or(u32::MAX)
    }
}

pub(super) fn i64_to_u64_saturating(value: i64) -> u64 {
    if value <= 0 {
        0
    } else {
        u64::try_from(value).unwrap_or(u64::MAX)
    }
}

pub(super) fn usize_to_i64_saturating(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn u32_to_usize_saturating(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

pub(super) fn bool_to_i64(value: bool) -> i64 {
    i64::from(u8::from(value))
}

pub(super) fn parse_required_rfc3339(idx: usize, raw: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|x| x.with_timezone(&Utc))
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(err)))
}

pub(super) fn parse_optional_rfc3339(
    idx: usize,
    raw: Option<&str>,
) -> rusqlite::Result<Option<DateTime<Utc>>> {
    raw.map(|value| parse_required_rfc3339(idx, value))
        .transpose()
}

pub(super) fn ratio_u64(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "ratio is a telemetry metric where floating precision tradeoff is acceptable"
    )]
    {
        numerator as f64 / denominator as f64
    }
}

pub(super) fn parse_string_vec_json(idx: usize, raw: &str) -> rusqlite::Result<Vec<String>> {
    serde_json::from_str::<Vec<String>>(raw)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(err)))
}
