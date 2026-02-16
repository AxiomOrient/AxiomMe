use std::time::Instant;

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params, types::Type};

use crate::error::{AxiomError, Result};
use crate::llm_io::estimate_text_tokens;
use crate::om::{OmObservationChunk, OmOriginType, OmRecord, OmScope, merge_buffered_reflection};

use super::{
    OmReflectionApplyContext, OmReflectionApplyOutcome, OmReflectionBufferPayload, SqliteStateStore,
};

mod helpers;
mod metrics;
mod scope;
use helpers::{
    bool_to_i64, elapsed_millis_u64, i64_to_u32_saturating, i64_to_u64_saturating,
    parse_optional_rfc3339, parse_required_rfc3339, parse_string_vec_json, ratio_u64,
    u32_to_usize_saturating, update_reflection_apply_metrics_tx, usize_to_i64_saturating,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmThreadState {
    pub scope_key: String,
    pub thread_id: String,
    pub last_observed_at: Option<DateTime<Utc>>,
    pub current_task: Option<String>,
    pub suggested_response: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl SqliteStateStore {
    #[cfg(test)]
    pub(crate) fn drop_om_tables_for_test(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("DROP TABLE IF EXISTS om_observation_chunks", [])?;
            conn.execute("DROP TABLE IF EXISTS om_records", [])?;
            Ok(())
        })
    }

    pub fn upsert_om_record(&self, record: &OmRecord) -> Result<()> {
        let activated_message_ids_json = serde_json::to_string(&record.last_activated_message_ids)?;
        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT INTO om_records(
                    id, scope, scope_key, session_id, thread_id, resource_id,
                    generation_count, last_applied_outbox_event_id, origin_type,
                    active_observations, observation_token_count, pending_message_tokens,
                    last_observed_at, current_task, suggested_response, last_activated_message_ids_json,
                    observer_trigger_count_total, reflector_trigger_count_total,
                    is_observing, is_reflecting,
                    is_buffering_observation, is_buffering_reflection,
                    last_buffered_at_tokens, last_buffered_at_time,
                    buffered_reflection, buffered_reflection_tokens,
                    buffered_reflection_input_tokens, reflected_observation_line_count,
                    created_at, updated_at
                )
                VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6,
                    ?7, ?8, ?9,
                    ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18,
                    ?19, ?20, ?21, ?22,
                    ?23, ?24,
                    ?25, ?26,
                    ?27, ?28,
                    ?29, ?30
                )
                ON CONFLICT(scope_key) DO UPDATE SET
                    scope=excluded.scope,
                    session_id=excluded.session_id,
                    thread_id=excluded.thread_id,
                    resource_id=excluded.resource_id,
                    generation_count=excluded.generation_count,
                    last_applied_outbox_event_id=excluded.last_applied_outbox_event_id,
                    origin_type=excluded.origin_type,
                    active_observations=excluded.active_observations,
                    observation_token_count=excluded.observation_token_count,
                    pending_message_tokens=excluded.pending_message_tokens,
                    last_observed_at=excluded.last_observed_at,
                    current_task=excluded.current_task,
                    suggested_response=excluded.suggested_response,
                    last_activated_message_ids_json=excluded.last_activated_message_ids_json,
                    observer_trigger_count_total=excluded.observer_trigger_count_total,
                    reflector_trigger_count_total=excluded.reflector_trigger_count_total,
                    is_observing=excluded.is_observing,
                    is_reflecting=excluded.is_reflecting,
                    is_buffering_observation=excluded.is_buffering_observation,
                    is_buffering_reflection=excluded.is_buffering_reflection,
                    last_buffered_at_tokens=excluded.last_buffered_at_tokens,
                    last_buffered_at_time=excluded.last_buffered_at_time,
                    buffered_reflection=excluded.buffered_reflection,
                    buffered_reflection_tokens=excluded.buffered_reflection_tokens,
                    buffered_reflection_input_tokens=excluded.buffered_reflection_input_tokens,
                    reflected_observation_line_count=excluded.reflected_observation_line_count,
                    updated_at=excluded.updated_at
                ",
                params![
                    record.id,
                    record.scope.as_str(),
                    record.scope_key,
                    record.session_id,
                    record.thread_id,
                    record.resource_id,
                    i64::from(record.generation_count),
                    record.last_applied_outbox_event_id,
                    record.origin_type.as_str(),
                    record.active_observations,
                    i64::from(record.observation_token_count),
                    i64::from(record.pending_message_tokens),
                    record.last_observed_at.map(|x| x.to_rfc3339()),
                    record.current_task,
                    record.suggested_response,
                    activated_message_ids_json,
                    i64::from(record.observer_trigger_count_total),
                    i64::from(record.reflector_trigger_count_total),
                    bool_to_i64(record.is_observing),
                    bool_to_i64(record.is_reflecting),
                    bool_to_i64(record.is_buffering_observation),
                    bool_to_i64(record.is_buffering_reflection),
                    i64::from(record.last_buffered_at_tokens),
                    record.last_buffered_at_time.map(|x| x.to_rfc3339()),
                    record.buffered_reflection,
                    record.buffered_reflection_tokens.map(i64::from),
                    record.buffered_reflection_input_tokens.map(i64::from),
                    record.reflected_observation_line_count.map(i64::from),
                    record.created_at.to_rfc3339(),
                    record.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_om_record_by_scope_key(&self, scope_key: &str) -> Result<Option<OmRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT
                    id, scope, scope_key, session_id, thread_id, resource_id,
                    generation_count, last_applied_outbox_event_id, origin_type,
                    active_observations, observation_token_count, pending_message_tokens,
                    last_observed_at, current_task, suggested_response, last_activated_message_ids_json,
                    observer_trigger_count_total, reflector_trigger_count_total,
                    is_observing, is_reflecting,
                    is_buffering_observation, is_buffering_reflection,
                    last_buffered_at_tokens, last_buffered_at_time,
                    buffered_reflection, buffered_reflection_tokens,
                    buffered_reflection_input_tokens, reflected_observation_line_count,
                    created_at, updated_at
                FROM om_records
                WHERE scope_key = ?1
                ",
            )?;

            let row = stmt
                .query_row(params![scope_key], |row| {
                    let scope_raw = row.get::<_, String>(1)?;
                    let scope = OmScope::parse(&scope_raw).ok_or_else(|| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("invalid om scope: {scope_raw}"),
                            )),
                        )
                    })?;
                    let origin_raw = row.get::<_, String>(8)?;
                    let origin_type = OmOriginType::parse(&origin_raw).ok_or_else(|| {
                        rusqlite::Error::FromSqlConversionFailure(
                            8,
                            Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("invalid om origin_type: {origin_raw}"),
                            )),
                        )
                    })?;

                    let last_observed_raw = row.get::<_, Option<String>>(12)?;
                    let last_activated_message_ids_raw = row.get::<_, String>(15)?;
                    let last_buffered_raw = row.get::<_, Option<String>>(23)?;
                    let last_activated_message_ids =
                        parse_string_vec_json(15, &last_activated_message_ids_raw)?;
                    let created_at_raw = row.get::<_, String>(28)?;
                    let updated_at_raw = row.get::<_, String>(29)?;

                    Ok(OmRecord {
                        id: row.get(0)?,
                        scope,
                        scope_key: row.get(2)?,
                        session_id: row.get(3)?,
                        thread_id: row.get(4)?,
                        resource_id: row.get(5)?,
                        generation_count: i64_to_u32_saturating(row.get::<_, i64>(6)?),
                        last_applied_outbox_event_id: row.get(7)?,
                        origin_type,
                        active_observations: row.get(9)?,
                        observation_token_count: i64_to_u32_saturating(row.get::<_, i64>(10)?),
                        pending_message_tokens: i64_to_u32_saturating(row.get::<_, i64>(11)?),
                        last_observed_at: parse_optional_rfc3339(
                            12,
                            last_observed_raw.as_deref(),
                        )?,
                        current_task: row.get(13)?,
                        suggested_response: row.get(14)?,
                        last_activated_message_ids,
                        observer_trigger_count_total: i64_to_u32_saturating(row.get::<_, i64>(16)?),
                        reflector_trigger_count_total: i64_to_u32_saturating(row.get::<_, i64>(17)?),
                        is_observing: row.get::<_, i64>(18)? != 0,
                        is_reflecting: row.get::<_, i64>(19)? != 0,
                        is_buffering_observation: row.get::<_, i64>(20)? != 0,
                        is_buffering_reflection: row.get::<_, i64>(21)? != 0,
                        last_buffered_at_tokens: i64_to_u32_saturating(row.get::<_, i64>(22)?),
                        last_buffered_at_time: parse_optional_rfc3339(
                            23,
                            last_buffered_raw.as_deref(),
                        )?,
                        buffered_reflection: row.get(24)?,
                        buffered_reflection_tokens: row
                            .get::<_, Option<i64>>(25)?
                            .map(i64_to_u32_saturating),
                        buffered_reflection_input_tokens: row
                            .get::<_, Option<i64>>(26)?
                            .map(i64_to_u32_saturating),
                        reflected_observation_line_count: row
                            .get::<_, Option<i64>>(27)?
                            .map(i64_to_u32_saturating),
                        created_at: parse_required_rfc3339(28, &created_at_raw)?,
                        updated_at: parse_required_rfc3339(29, &updated_at_raw)?,
                    })
                })
                .optional()?;

            Ok(row)
        })
    }

    pub fn append_om_observation_chunk(&self, chunk: &OmObservationChunk) -> Result<()> {
        let message_ids_json = serde_json::to_string(&chunk.message_ids)?;
        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT INTO om_observation_chunks(
                    id, record_id, seq, cycle_id, observations,
                    token_count, message_tokens, message_ids_json,
                    last_observed_at, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    chunk.id,
                    chunk.record_id,
                    i64::from(chunk.seq),
                    chunk.cycle_id,
                    chunk.observations,
                    i64::from(chunk.token_count),
                    i64::from(chunk.message_tokens),
                    message_ids_json,
                    chunk.last_observed_at.to_rfc3339(),
                    chunk.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn om_observer_event_applied(&self, outbox_event_id: i64) -> Result<bool> {
        self.with_conn(|conn| {
            let exists = conn
                .query_row(
                    "SELECT 1 FROM om_observer_applied_events WHERE outbox_event_id = ?1 LIMIT 1",
                    params![outbox_event_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            Ok(exists)
        })
    }

    pub fn append_om_observation_chunk_with_event_cas(
        &self,
        scope_key: &str,
        expected_generation: u32,
        outbox_event_id: i64,
        chunk: &OmObservationChunk,
    ) -> Result<bool> {
        let message_ids_json = serde_json::to_string(&chunk.message_ids)?;
        self.with_tx(|tx| {
            let row = tx
                .query_row(
                    r"
                    SELECT generation_count
                    FROM om_records
                    WHERE scope_key = ?1
                    ",
                    params![scope_key],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;

            let Some(generation_count) = row.map(i64_to_u32_saturating) else {
                return Err(AxiomError::NotFound(format!(
                    "om record not found for scope_key={scope_key}"
                )));
            };
            if generation_count != expected_generation {
                return Ok(false);
            }

            let already_applied = tx
                .query_row(
                    "SELECT 1 FROM om_observer_applied_events WHERE outbox_event_id = ?1 LIMIT 1",
                    params![outbox_event_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if already_applied {
                return Ok(false);
            }

            tx.execute(
                r"
                INSERT INTO om_observation_chunks(
                    id, record_id, seq, cycle_id, observations,
                    token_count, message_tokens, message_ids_json,
                    last_observed_at, created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    chunk.id,
                    chunk.record_id,
                    i64::from(chunk.seq),
                    chunk.cycle_id,
                    chunk.observations,
                    i64::from(chunk.token_count),
                    i64::from(chunk.message_tokens),
                    message_ids_json,
                    chunk.last_observed_at.to_rfc3339(),
                    chunk.created_at.to_rfc3339(),
                ],
            )?;
            tx.execute(
                r"
                INSERT INTO om_observer_applied_events(
                    outbox_event_id, scope_key, generation_count, created_at
                )
                VALUES (?1, ?2, ?3, ?4)
                ",
                params![
                    outbox_event_id,
                    scope_key,
                    i64::from(expected_generation),
                    Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(true)
        })
    }

    pub fn list_om_observation_chunks(&self, record_id: &str) -> Result<Vec<OmObservationChunk>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT id, record_id, seq, cycle_id, observations,
                       token_count, message_tokens, message_ids_json,
                       last_observed_at, created_at
                FROM om_observation_chunks
                WHERE record_id = ?1
                ORDER BY seq ASC, created_at ASC
                ",
            )?;

            let rows = stmt.query_map(params![record_id], |row| {
                let message_ids_raw = row.get::<_, String>(7)?;
                let message_ids =
                    serde_json::from_str::<Vec<String>>(&message_ids_raw).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(7, Type::Text, Box::new(err))
                    })?;
                let last_observed_at_raw = row.get::<_, String>(8)?;
                let created_at_raw = row.get::<_, String>(9)?;

                Ok(OmObservationChunk {
                    id: row.get(0)?,
                    record_id: row.get(1)?,
                    seq: i64_to_u32_saturating(row.get::<_, i64>(2)?),
                    cycle_id: row.get(3)?,
                    observations: row.get(4)?,
                    token_count: i64_to_u32_saturating(row.get::<_, i64>(5)?),
                    message_tokens: i64_to_u32_saturating(row.get::<_, i64>(6)?),
                    message_ids,
                    last_observed_at: parse_required_rfc3339(8, &last_observed_at_raw)?,
                    created_at: parse_required_rfc3339(9, &created_at_raw)?,
                })
            })?;

            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn clear_om_observation_chunks_through_seq(
        &self,
        record_id: &str,
        max_seq_inclusive: u32,
    ) -> Result<usize> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                "DELETE FROM om_observation_chunks WHERE record_id = ?1 AND seq <= ?2",
                params![record_id, i64::from(max_seq_inclusive)],
            )?;
            Ok(affected)
        })
    }

    pub fn buffer_om_reflection_with_cas(
        &self,
        scope_key: &str,
        expected_generation: u32,
        payload: OmReflectionBufferPayload<'_>,
    ) -> Result<bool> {
        self.with_tx(|tx| {
            let row = tx
                .query_row(
                    r"
                    SELECT generation_count, buffered_reflection
                    FROM om_records
                    WHERE scope_key = ?1
                    ",
                    params![scope_key],
                    |row| {
                        Ok((
                            i64_to_u32_saturating(row.get::<_, i64>(0)?),
                            row.get::<_, Option<String>>(1)?,
                        ))
                    },
                )
                .optional()?;

            let Some((generation_count, buffered_reflection)) = row else {
                return Err(AxiomError::NotFound(format!(
                    "om record not found for scope_key={scope_key}"
                )));
            };
            if generation_count != expected_generation {
                return Ok(false);
            }
            if buffered_reflection
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                tx.execute(
                    "UPDATE om_records SET is_buffering_reflection = 0, updated_at = ?2 WHERE scope_key = ?1",
                    params![scope_key, Utc::now().to_rfc3339()],
                )?;
                return Ok(false);
            }

            let now = Utc::now().to_rfc3339();
            let affected = tx.execute(
                r"
                UPDATE om_records
                SET is_buffering_reflection = 0,
                    buffered_reflection = ?2,
                    buffered_reflection_tokens = ?3,
                    buffered_reflection_input_tokens = ?4,
                    reflected_observation_line_count = ?5,
                    current_task = COALESCE(?6, current_task),
                    suggested_response = COALESCE(?7, suggested_response),
                    updated_at = ?8
                WHERE scope_key = ?1 AND generation_count = ?9
                ",
                params![
                    scope_key,
                    payload.reflection,
                    i64::from(payload.reflection_token_count),
                    i64::from(payload.reflection_input_tokens),
                    i64::from(payload.reflected_observation_line_count),
                    payload.current_task,
                    payload.suggested_response,
                    now,
                    i64::from(expected_generation),
                ],
            )?;
            Ok(affected > 0)
        })
    }

    pub fn clear_om_reflection_flags_with_cas(
        &self,
        scope_key: &str,
        expected_generation: u32,
    ) -> Result<bool> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                r"
                UPDATE om_records
                SET is_reflecting = 0,
                    is_buffering_reflection = 0,
                    updated_at = ?3
                WHERE scope_key = ?1
                  AND generation_count = ?2
                ",
                params![
                    scope_key,
                    i64::from(expected_generation),
                    Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(affected > 0)
        })
    }

    pub fn apply_om_reflection_with_cas(
        &self,
        scope_key: &str,
        expected_generation: u32,
        outbox_event_id: i64,
        reflection: &str,
        reflected_observation_line_count: u32,
        context: OmReflectionApplyContext<'_>,
    ) -> Result<OmReflectionApplyOutcome> {
        let started = Instant::now();
        self.with_tx(|tx| {
            let row = tx
                .query_row(
                    r"
                    SELECT generation_count, last_applied_outbox_event_id, active_observations
                    FROM om_records
                    WHERE scope_key = ?1
                    ",
                    params![scope_key],
                    |row| {
                        Ok((
                            i64_to_u32_saturating(row.get::<_, i64>(0)?),
                            row.get::<_, Option<i64>>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;

            let Some((generation_count, last_applied_outbox_event_id, active_observations)) = row
            else {
                return Err(AxiomError::NotFound(format!(
                    "om record not found for scope_key={scope_key}"
                )));
            };

            if last_applied_outbox_event_id == Some(outbox_event_id) {
                let latency_ms = elapsed_millis_u64(started.elapsed().as_millis());
                update_reflection_apply_metrics_tx(
                    tx,
                    OmReflectionApplyOutcome::IdempotentEvent,
                    latency_ms,
                )?;
                return Ok(OmReflectionApplyOutcome::IdempotentEvent);
            }
            if generation_count != expected_generation {
                let latency_ms = elapsed_millis_u64(started.elapsed().as_millis());
                update_reflection_apply_metrics_tx(
                    tx,
                    OmReflectionApplyOutcome::StaleGeneration,
                    latency_ms,
                )?;
                return Ok(OmReflectionApplyOutcome::StaleGeneration);
            }

            let active_lines = active_observations
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            let merged = merge_buffered_reflection(
                &active_lines,
                u32_to_usize_saturating(reflected_observation_line_count),
                reflection,
            );
            let now = Utc::now().to_rfc3339();
            tx.execute(
                r"
                UPDATE om_records
                SET generation_count = ?2,
                    last_applied_outbox_event_id = ?3,
                    origin_type = 'reflection',
                    active_observations = ?4,
                    observation_token_count = ?5,
                    is_reflecting = 0,
                    is_buffering_reflection = 0,
                    buffered_reflection = NULL,
                    buffered_reflection_tokens = NULL,
                    buffered_reflection_input_tokens = NULL,
                    reflected_observation_line_count = ?6,
                    current_task = COALESCE(?7, current_task),
                    suggested_response = COALESCE(?8, suggested_response),
                    updated_at = ?9
                WHERE scope_key = ?1
                  AND generation_count = ?10
                ",
                params![
                    scope_key,
                    i64::from(generation_count.saturating_add(1)),
                    outbox_event_id,
                    merged,
                    i64::from(estimate_text_tokens(&merged)),
                    i64::from(reflected_observation_line_count),
                    context.current_task,
                    context.suggested_response,
                    now,
                    i64::from(expected_generation),
                ],
            )?;
            let latency_ms = elapsed_millis_u64(started.elapsed().as_millis());
            update_reflection_apply_metrics_tx(tx, OmReflectionApplyOutcome::Applied, latency_ms)?;
            Ok(OmReflectionApplyOutcome::Applied)
        })
    }
}
