use std::time::Instant;

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, Transaction, params, types::Type};

use crate::error::{AxiomError, Result};
use crate::llm_io::estimate_text_tokens;
use crate::om::{
    ContinuationPolicyV2, OmContinuationCandidateV2, OmContinuationSourceKind,
    OmContinuationStateV2, OmObservationChunk, OmOriginType, OmRecord, OmScope,
    resolve_canonical_thread_id, resolve_continuation_update,
};

use super::{
    OmReflectionApplyContext, OmReflectionApplyOutcome, OmReflectionBufferPayload, SqliteStateStore,
};

mod helpers;
mod metrics;
mod scope;
use helpers::{
    bool_to_i64, elapsed_millis_u64, i64_to_u32_saturating, i64_to_u64_saturating,
    parse_optional_rfc3339, parse_required_rfc3339, parse_string_vec_json, ratio_u64,
    update_reflection_apply_metrics_tx, usize_to_i64_saturating,
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

#[derive(Debug, Clone)]
pub struct OmContinuationState {
    pub canonical_thread_id: String,
    pub current_task: Option<String>,
    pub suggested_response: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct OmContinuationHints<'a> {
    pub current_task: Option<&'a str>,
    pub suggested_response: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct ActiveOmEntryRow {
    entry_id: String,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmActiveEntry {
    pub entry_id: String,
    pub canonical_thread_id: String,
    pub priority: String,
    pub text: String,
    pub origin_kind: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct ReflectionApplyRecordState {
    generation_count: u32,
    last_applied_outbox_event_id: Option<i64>,
    scope: OmScope,
    thread_id: Option<String>,
    session_id: Option<String>,
}

const OM_ENTRY_PRIORITY_HIGH: &str = "high";
const OM_ENTRY_PRIORITY_MEDIUM: &str = "medium";
const OM_ENTRY_ORIGIN_OBSERVATION: &str = "observation";
const OM_ENTRY_ORIGIN_REFLECTION: &str = "reflection";
const OM_CONTINUATION_SOURCE_REFLECTION: &str = "reflection";

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
                    Option::<i64>::None,
                    record.created_at.to_rfc3339(),
                    record.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_om_records(&self) -> Result<Vec<OmRecord>> {
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
                ",
            )?;

            let rows = stmt.query_map([], |row| {
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
                    last_observed_at: parse_optional_rfc3339(12, last_observed_raw.as_deref())?,
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
                    last_buffered_at_time: parse_optional_rfc3339(23, last_buffered_raw.as_deref())?,
                    buffered_reflection: row.get(24)?,
                    buffered_reflection_tokens: row
                        .get::<_, Option<i64>>(25)?
                        .map(i64_to_u32_saturating),
                    buffered_reflection_input_tokens: row
                        .get::<_, Option<i64>>(26)?
                        .map(i64_to_u32_saturating),
                    created_at: parse_required_rfc3339(28, &created_at_raw)?,
                    updated_at: parse_required_rfc3339(29, &updated_at_raw)?,
                })
            })?;

            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
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
        self.with_tx(|tx| {
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
                    &message_ids_json,
                    chunk.last_observed_at.to_rfc3339(),
                    chunk.created_at.to_rfc3339(),
                ],
            )?;
            insert_observation_entries_for_chunk_tx(tx, chunk, &message_ids_json)?;
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
                    SELECT id, generation_count
                    FROM om_records
                    WHERE scope_key = ?1
                    ",
                    params![scope_key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;

            let Some((resolved_record_id, generation_count_raw)) = row else {
                return Err(AxiomError::NotFound(format!(
                    "om record not found for scope_key={scope_key}"
                )));
            };
            let generation_count = i64_to_u32_saturating(generation_count_raw);
            if generation_count != expected_generation {
                return Ok(false);
            }
            if chunk.record_id != resolved_record_id {
                return Err(AxiomError::Validation(format!(
                    "om observation chunk record_id mismatch for scope_key={scope_key}: expected {resolved_record_id}, got {}",
                    chunk.record_id
                )));
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
                    &message_ids_json,
                    chunk.last_observed_at.to_rfc3339(),
                    chunk.created_at.to_rfc3339(),
                ],
            )?;
            insert_observation_entries_for_chunk_tx(tx, chunk, &message_ids_json)?;
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
                    reflected_observation_line_count = NULL,
                    updated_at = ?5
                WHERE scope_key = ?1 AND generation_count = ?6
                ",
                params![
                    scope_key,
                    payload.reflection,
                    i64::from(payload.reflection_token_count),
                    i64::from(payload.reflection_input_tokens),
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
        covers_entry_ids: &[String],
        context: OmReflectionApplyContext<'_>,
    ) -> Result<OmReflectionApplyOutcome> {
        let started = Instant::now();
        self.with_tx(|tx| {
            let Some(record_state) = load_reflection_apply_record_state_tx(tx, scope_key)? else {
                return Err(AxiomError::NotFound(format!(
                    "om record not found for scope_key={scope_key}"
                )));
            };

            if record_state.last_applied_outbox_event_id == Some(outbox_event_id) {
                let latency_ms = elapsed_millis_u64(started.elapsed().as_millis());
                update_reflection_apply_metrics_tx(
                    tx,
                    OmReflectionApplyOutcome::IdempotentEvent,
                    latency_ms,
                )?;
                return Ok(OmReflectionApplyOutcome::IdempotentEvent);
            }
            if record_state.generation_count != expected_generation {
                let latency_ms = elapsed_millis_u64(started.elapsed().as_millis());
                update_reflection_apply_metrics_tx(
                    tx,
                    OmReflectionApplyOutcome::StaleGeneration,
                    latency_ms,
                )?;
                return Ok(OmReflectionApplyOutcome::StaleGeneration);
            }

            let now = Utc::now();
            let now_rfc3339 = now.to_rfc3339();
            let canonical_thread_id =
                resolve_canonical_thread_id_for_record(&record_state, scope_key);
            let active_entries = list_active_om_entries_tx(tx, scope_key)?;

            let requested_cover_ids = covers_entry_ids
                .iter()
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<std::collections::HashSet<_>>();
            let covers_entry_ids = active_entries
                .iter()
                .filter(|entry| requested_cover_ids.contains(entry.entry_id.as_str()))
                .map(|entry| entry.entry_id.clone())
                .collect::<Vec<_>>();

            let reflection_text = reflection.trim();
            let covered_ids = if reflection_text.is_empty() {
                std::collections::HashSet::<&str>::new()
            } else {
                covers_entry_ids
                    .iter()
                    .map(String::as_str)
                    .collect::<std::collections::HashSet<_>>()
            };
            let reflection_entry_id = format!("reflection:{scope_key}:{outbox_event_id}");
            if !reflection_text.is_empty() {
                tx.execute(
                    r"
                    INSERT INTO om_entries(
                        entry_id, scope_key, canonical_thread_id, priority, text,
                        source_message_ids_json, origin_kind, created_at, superseded_by
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
                    ON CONFLICT(entry_id) DO UPDATE SET
                        canonical_thread_id = excluded.canonical_thread_id,
                        priority = excluded.priority,
                        text = excluded.text,
                        source_message_ids_json = excluded.source_message_ids_json,
                        origin_kind = excluded.origin_kind,
                        created_at = excluded.created_at
                    ",
                    params![
                        &reflection_entry_id,
                        scope_key,
                        &canonical_thread_id,
                        OM_ENTRY_PRIORITY_HIGH,
                        reflection_text,
                        "[]",
                        OM_ENTRY_ORIGIN_REFLECTION,
                        &now_rfc3339,
                    ],
                )?;

                for entry_id in &covers_entry_ids {
                    tx.execute(
                        "UPDATE om_entries SET superseded_by = ?2 WHERE entry_id = ?1",
                        params![entry_id, &reflection_entry_id],
                    )?;
                }

                tx.execute(
                    r"
                    INSERT INTO om_reflection_events(
                        event_id, scope_key, covers_entry_ids_json, reflection_entry_id, created_at
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT(event_id) DO UPDATE SET
                        covers_entry_ids_json = excluded.covers_entry_ids_json,
                        reflection_entry_id = excluded.reflection_entry_id,
                        created_at = excluded.created_at
                    ",
                    params![
                        format!("outbox:{outbox_event_id}"),
                        scope_key,
                        serde_json::to_string(&covers_entry_ids)?,
                        &reflection_entry_id,
                        &now_rfc3339,
                    ],
                )?;
            }

            let mut merged_parts = Vec::<String>::new();
            if !reflection_text.is_empty() {
                merged_parts.push(reflection_text.to_string());
            }
            merged_parts.extend(
                active_entries
                    .iter()
                    .filter(|entry| !covered_ids.contains(entry.entry_id.as_str()))
                    .map(|entry| entry.text.clone()),
            );
            let merged = merged_parts
                .iter()
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");

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
                    reflected_observation_line_count = NULL,
                    updated_at = ?6
                WHERE scope_key = ?1
                  AND generation_count = ?7
                ",
                params![
                    scope_key,
                    i64::from(record_state.generation_count.saturating_add(1)),
                    outbox_event_id,
                    merged,
                    i64::from(estimate_text_tokens(&merged)),
                    &now_rfc3339,
                    i64::from(expected_generation),
                ],
            )?;
            let continuation_current_task = normalize_optional_text(context.current_task);
            let continuation_suggested_response =
                normalize_optional_text(context.suggested_response);
            let continuation_hints = OmContinuationHints {
                current_task: continuation_current_task.as_deref(),
                suggested_response: continuation_suggested_response.as_deref(),
            };
            if continuation_hints.current_task.is_some()
                || continuation_hints.suggested_response.is_some()
            {
                upsert_om_continuation_state_tx(
                    tx,
                    scope_key,
                    &canonical_thread_id,
                    continuation_hints,
                    continuation_confidence(
                        continuation_hints.current_task,
                        continuation_hints.suggested_response,
                    ),
                    OM_CONTINUATION_SOURCE_REFLECTION,
                    &now_rfc3339,
                )?;
            }
            let latency_ms = elapsed_millis_u64(started.elapsed().as_millis());
            update_reflection_apply_metrics_tx(tx, OmReflectionApplyOutcome::Applied, latency_ms)?;
            Ok(OmReflectionApplyOutcome::Applied)
        })
    }
}

fn insert_observation_entries_for_chunk_tx(
    tx: &Transaction<'_>,
    chunk: &OmObservationChunk,
    source_message_ids_json: &str,
) -> Result<()> {
    let record = tx
        .query_row(
            r"
            SELECT scope, scope_key, thread_id, session_id
            FROM om_records
            WHERE id = ?1
            ",
            params![chunk.record_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((scope_raw, scope_key, thread_id, session_id)) = record else {
        return Ok(());
    };
    let scope = OmScope::parse(&scope_raw).ok_or_else(|| {
        AxiomError::Validation(format!(
            "invalid om scope while appending chunk: {scope_raw}"
        ))
    })?;
    let fallback = session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(scope_key.as_str());
    let canonical_thread_id = resolve_canonical_thread_id(
        scope,
        &scope_key,
        thread_id.as_deref(),
        session_id.as_deref(),
        fallback,
    );

    let entry_text = chunk.observations.trim();
    if entry_text.is_empty() {
        return Ok(());
    }
    let entry_id = format!("observation:{}", chunk.id);
    tx.execute(
        r"
        INSERT INTO om_entries(
            entry_id, scope_key, canonical_thread_id, priority, text,
            source_message_ids_json, origin_kind, created_at, superseded_by
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
        ON CONFLICT(entry_id) DO UPDATE SET
            canonical_thread_id = excluded.canonical_thread_id,
            priority = excluded.priority,
            text = excluded.text,
            source_message_ids_json = excluded.source_message_ids_json,
            origin_kind = excluded.origin_kind,
            created_at = excluded.created_at
        ",
        params![
            entry_id,
            &scope_key,
            &canonical_thread_id,
            OM_ENTRY_PRIORITY_MEDIUM,
            entry_text,
            source_message_ids_json,
            OM_ENTRY_ORIGIN_OBSERVATION,
            chunk.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn load_reflection_apply_record_state_tx(
    tx: &Transaction<'_>,
    scope_key: &str,
) -> Result<Option<ReflectionApplyRecordState>> {
    tx.query_row(
        r"
        SELECT generation_count, last_applied_outbox_event_id, scope, thread_id, session_id
        FROM om_records
        WHERE scope_key = ?1
        ",
        params![scope_key],
        |row| {
            let scope_raw = row.get::<_, String>(2)?;
            let scope = OmScope::parse(&scope_raw).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid om scope: {scope_raw}"),
                    )),
                )
            })?;
            Ok(ReflectionApplyRecordState {
                generation_count: i64_to_u32_saturating(row.get::<_, i64>(0)?),
                last_applied_outbox_event_id: row.get::<_, Option<i64>>(1)?,
                scope,
                thread_id: row.get::<_, Option<String>>(3)?,
                session_id: row.get::<_, Option<String>>(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn list_active_om_entries_tx(
    tx: &Transaction<'_>,
    scope_key: &str,
) -> Result<Vec<ActiveOmEntryRow>> {
    let mut stmt = tx.prepare(
        r"
        SELECT entry_id, text
        FROM om_entries
        WHERE scope_key = ?1 AND superseded_by IS NULL
        ORDER BY created_at ASC, entry_id ASC
        ",
    )?;
    let rows = stmt.query_map(params![scope_key], |row| {
        Ok(ActiveOmEntryRow {
            entry_id: row.get(0)?,
            text: row.get(1)?,
        })
    })?;
    let mut out = Vec::<ActiveOmEntryRow>::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn resolve_canonical_thread_id_for_record(
    record: &ReflectionApplyRecordState,
    scope_key: &str,
) -> String {
    let fallback = record
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(scope_key);
    resolve_canonical_thread_id(
        record.scope,
        scope_key,
        record.thread_id.as_deref(),
        record.session_id.as_deref(),
        fallback,
    )
}

fn upsert_om_continuation_state_tx(
    tx: &Transaction<'_>,
    scope_key: &str,
    canonical_thread_id: &str,
    continuation_hints: OmContinuationHints<'_>,
    confidence: f64,
    source_kind: &str,
    updated_at: &str,
) -> Result<()> {
    let previous_row = tx
        .query_row(
            r"
            SELECT current_task, suggested_response, confidence, source_kind, updated_at
            FROM om_continuation_state
            WHERE scope_key = ?1 AND canonical_thread_id = ?2
            ",
            params![scope_key, canonical_thread_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let previous_state = previous_row.as_ref().map(
        |(prev_task, prev_response, prev_confidence, prev_source_kind, prev_updated_at)| {
            OmContinuationStateV2 {
                scope_key: scope_key.to_string(),
                thread_id: canonical_thread_id.to_string(),
                current_task: prev_task.clone(),
                suggested_response: prev_response.clone(),
                confidence_milli: continuation_confidence_to_milli(*prev_confidence),
                source_kind: continuation_source_kind_from_storage(prev_source_kind),
                source_message_ids: Vec::new(),
                updated_at_rfc3339: prev_updated_at.clone(),
                staleness_budget_ms: 0,
            }
        },
    );
    let candidate = OmContinuationCandidateV2 {
        scope_key: scope_key.to_string(),
        thread_id: canonical_thread_id.to_string(),
        current_task: normalize_optional_text(continuation_hints.current_task),
        suggested_response: normalize_optional_text(continuation_hints.suggested_response),
        confidence_milli: continuation_confidence_to_milli(confidence),
        source_kind: continuation_source_kind_from_storage(source_kind),
        source_message_ids: Vec::new(),
        updated_at_rfc3339: updated_at.to_string(),
        staleness_budget_ms: 0,
    };
    let Some(merged) = resolve_continuation_update(
        previous_state.as_ref(),
        &candidate,
        ContinuationPolicyV2::default(),
    ) else {
        return Ok(());
    };
    let source_kind = continuation_source_kind_to_storage(merged.source_kind);

    tx.execute(
        r"
        INSERT INTO om_continuation_state(
            scope_key, canonical_thread_id, current_task, suggested_response,
            confidence, source_kind, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(scope_key, canonical_thread_id) DO UPDATE SET
            current_task = excluded.current_task,
            suggested_response = excluded.suggested_response,
            confidence = excluded.confidence,
            source_kind = excluded.source_kind,
            updated_at = excluded.updated_at
        ",
        params![
            scope_key,
            canonical_thread_id,
            merged.current_task,
            merged.suggested_response,
            continuation_confidence_from_milli(merged.confidence_milli),
            source_kind,
            merged.updated_at_rfc3339,
        ],
    )?;
    Ok(())
}

fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(super) fn continuation_source_kind_from_storage(raw: &str) -> OmContinuationSourceKind {
    match raw.trim() {
        "reflection" => OmContinuationSourceKind::Reflector,
        "explicit_user_task" => OmContinuationSourceKind::ExplicitUserTask,
        "observer_deterministic" => OmContinuationSourceKind::ObserverDeterministic,
        "observer" | "observer_llm" | "observer_interval" => OmContinuationSourceKind::ObserverLlm,
        _ => OmContinuationSourceKind::ObserverLlm,
    }
}

pub(super) const fn continuation_source_kind_to_storage(
    value: OmContinuationSourceKind,
) -> &'static str {
    match value {
        OmContinuationSourceKind::ObserverLlm => "observer_llm",
        OmContinuationSourceKind::ObserverDeterministic => "observer_deterministic",
        OmContinuationSourceKind::Reflector => "reflection",
        OmContinuationSourceKind::ExplicitUserTask => "explicit_user_task",
    }
}

pub(super) fn continuation_confidence_to_milli(raw: f64) -> u16 {
    (raw * 1000.0).round().clamp(0.0, 1000.0) as u16
}

pub(super) fn continuation_confidence_from_milli(raw: u16) -> f64 {
    f64::from(raw) / 1000.0
}

fn continuation_confidence(current_task: Option<&str>, suggested_response: Option<&str>) -> f64 {
    let has_current_task = normalize_optional_text(current_task).is_some();
    let has_suggested_response = normalize_optional_text(suggested_response).is_some();
    if has_current_task && has_suggested_response {
        0.92
    } else if has_current_task || has_suggested_response {
        0.82
    } else {
        0.0
    }
}
