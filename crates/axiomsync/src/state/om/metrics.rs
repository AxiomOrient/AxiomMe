use rusqlite::OptionalExtension;

use crate::error::Result;
use crate::models::{OmQueueStatus, OmReflectionApplyMetrics};

use super::SqliteStateStore;

impl SqliteStateStore {
    pub fn om_status_snapshot(&self) -> Result<OmQueueStatus> {
        self.with_conn(|conn| {
            let (
                records_total,
                observing_count,
                reflecting_count,
                buffering_observation_count,
                buffering_reflection_count,
                observation_tokens_active,
                pending_message_tokens,
                observer_trigger_count_total,
                reflector_trigger_count_total,
            ) = conn.query_row(
                r"
                SELECT
                    COUNT(*) AS records_total,
                    COALESCE(SUM(CASE WHEN is_observing != 0 THEN 1 ELSE 0 END), 0) AS observing_count,
                    COALESCE(SUM(CASE WHEN is_reflecting != 0 THEN 1 ELSE 0 END), 0) AS reflecting_count,
                    COALESCE(SUM(CASE WHEN is_buffering_observation != 0 THEN 1 ELSE 0 END), 0) AS buffering_observation_count,
                    COALESCE(SUM(CASE WHEN is_buffering_reflection != 0 THEN 1 ELSE 0 END), 0) AS buffering_reflection_count,
                    COALESCE(SUM(observation_token_count), 0) AS observation_tokens_active,
                    COALESCE(SUM(pending_message_tokens), 0) AS pending_message_tokens,
                    COALESCE(SUM(observer_trigger_count_total), 0) AS observer_trigger_count_total,
                    COALESCE(SUM(reflector_trigger_count_total), 0) AS reflector_trigger_count_total
                FROM om_records
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                    ))
                },
            )?;

            Ok(OmQueueStatus {
                records_total: super::i64_to_u64_saturating(records_total),
                observing_count: super::i64_to_u64_saturating(observing_count),
                reflecting_count: super::i64_to_u64_saturating(reflecting_count),
                buffering_observation_count: super::i64_to_u64_saturating(buffering_observation_count),
                buffering_reflection_count: super::i64_to_u64_saturating(buffering_reflection_count),
                observation_tokens_active: super::i64_to_u64_saturating(observation_tokens_active),
                pending_message_tokens: super::i64_to_u64_saturating(pending_message_tokens),
                observer_trigger_count_total: super::i64_to_u64_saturating(observer_trigger_count_total),
                reflector_trigger_count_total: super::i64_to_u64_saturating(reflector_trigger_count_total),
            })
        })
    }

    pub fn om_reflection_apply_metrics_snapshot(&self) -> Result<OmReflectionApplyMetrics> {
        self.with_conn(|conn| {
            let row = conn
                .query_row(
                    r"
                    SELECT
                        reflect_apply_attempts_total,
                        reflect_apply_applied_total,
                        reflect_apply_stale_generation_total,
                        reflect_apply_idempotent_total,
                        reflect_apply_latency_ms_total,
                        reflect_apply_latency_ms_max
                    FROM om_runtime_metrics
                    WHERE id = 1
                    ",
                    [],
                    |row| {
                        Ok((
                            super::i64_to_u64_saturating(row.get::<_, i64>(0)?),
                            super::i64_to_u64_saturating(row.get::<_, i64>(1)?),
                            super::i64_to_u64_saturating(row.get::<_, i64>(2)?),
                            super::i64_to_u64_saturating(row.get::<_, i64>(3)?),
                            super::i64_to_u64_saturating(row.get::<_, i64>(4)?),
                            super::i64_to_u64_saturating(row.get::<_, i64>(5)?),
                        ))
                    },
                )
                .optional()?;

            let Some((
                attempts_total,
                applied_total,
                stale_generation_total,
                idempotent_total,
                latency_total_ms,
                max_latency_ms,
            )) = row
            else {
                return Ok(OmReflectionApplyMetrics::default());
            };

            Ok(OmReflectionApplyMetrics {
                attempts_total,
                applied_total,
                stale_generation_total,
                idempotent_total,
                stale_generation_ratio: super::ratio_u64(stale_generation_total, attempts_total),
                avg_latency_ms: super::ratio_u64(latency_total_ms, attempts_total),
                max_latency_ms,
            })
        })
    }
}
