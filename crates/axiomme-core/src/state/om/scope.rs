use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::error::Result;

use super::{OmActiveEntry, OmContinuationState, OmThreadState, SqliteStateStore};

impl SqliteStateStore {
    pub fn upsert_om_scope_session(&self, scope_key: &str, session_id: &str) -> Result<()> {
        let scope_key = scope_key.trim();
        let session_id = session_id.trim();
        if scope_key.is_empty() || session_id.is_empty() {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT INTO om_scope_sessions(scope_key, session_id, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(scope_key, session_id) DO UPDATE SET
                    updated_at=excluded.updated_at
                ",
                params![scope_key, session_id, now],
            )?;
            Ok(())
        })
    }

    pub fn list_om_scope_sessions(&self, scope_key: &str, limit: usize) -> Result<Vec<String>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT session_id
                FROM om_scope_sessions
                WHERE scope_key = ?1
                ORDER BY updated_at DESC, session_id ASC
                LIMIT ?2
                ",
            )?;

            let rows = stmt.query_map(
                params![scope_key, super::usize_to_i64_saturating(limit)],
                |row| row.get::<_, String>(0),
            )?;
            let mut out = Vec::<String>::new();
            for row in rows {
                let session_id = row?;
                if !session_id.trim().is_empty() {
                    out.push(session_id);
                }
            }
            Ok(out)
        })
    }

    pub fn list_om_scope_keys_for_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<String>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Ok(Vec::new());
        }
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT scope_key
                FROM om_scope_sessions
                WHERE session_id = ?1
                ORDER BY updated_at DESC, scope_key ASC
                LIMIT ?2
                ",
            )?;

            let rows = stmt.query_map(
                params![session_id, super::usize_to_i64_saturating(limit)],
                |row| row.get::<_, String>(0),
            )?;
            let mut out = Vec::<String>::new();
            for row in rows {
                let scope_key = row?;
                if !scope_key.trim().is_empty() {
                    out.push(scope_key);
                }
            }
            Ok(out)
        })
    }

    pub(crate) fn upsert_om_thread_state(
        &self,
        scope_key: &str,
        thread_id: &str,
        last_observed_at: Option<DateTime<Utc>>,
        current_task: Option<&str>,
        suggested_response: Option<&str>,
    ) -> Result<()> {
        let scope_key = scope_key.trim();
        let thread_id = thread_id.trim();
        if scope_key.is_empty() || thread_id.is_empty() {
            return Ok(());
        }
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT INTO om_thread_states(
                    scope_key, thread_id, last_observed_at, current_task, suggested_response, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(scope_key, thread_id) DO UPDATE SET
                    last_observed_at=COALESCE(excluded.last_observed_at, om_thread_states.last_observed_at),
                    current_task=COALESCE(excluded.current_task, om_thread_states.current_task),
                    suggested_response=COALESCE(excluded.suggested_response, om_thread_states.suggested_response),
                    updated_at=excluded.updated_at
                ",
                params![
                    scope_key,
                    thread_id,
                    last_observed_at.map(|x| x.to_rfc3339()),
                    current_task,
                    suggested_response,
                    now
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn list_om_thread_states(&self, scope_key: &str) -> Result<Vec<OmThreadState>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT scope_key, thread_id, last_observed_at, current_task, suggested_response, updated_at
                FROM om_thread_states
                WHERE scope_key = ?1
                ORDER BY updated_at DESC, thread_id ASC
                ",
            )?;
            let rows = stmt.query_map(params![scope_key], |row| {
                let last_observed_raw = row.get::<_, Option<String>>(2)?;
                let updated_at_raw = row.get::<_, String>(5)?;
                Ok(OmThreadState {
                    scope_key: row.get(0)?,
                    thread_id: row.get(1)?,
                    last_observed_at: super::parse_optional_rfc3339(2, last_observed_raw.as_deref())?,
                    current_task: row.get(3)?,
                    suggested_response: row.get(4)?,
                    updated_at: super::parse_required_rfc3339(5, &updated_at_raw)?,
                })
            })?;
            let mut out = Vec::<OmThreadState>::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub(crate) fn upsert_om_continuation_state(
        &self,
        scope_key: &str,
        canonical_thread_id: &str,
        current_task: Option<&str>,
        suggested_response: Option<&str>,
        confidence: f64,
        source_kind: &str,
        updated_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let scope_key = scope_key.trim();
        let canonical_thread_id = canonical_thread_id.trim();
        let source_kind = source_kind.trim();
        if scope_key.is_empty() || canonical_thread_id.is_empty() || source_kind.is_empty() {
            return Ok(());
        }
        let current_task = normalize_optional_text(current_task);
        let suggested_response = normalize_optional_text(suggested_response);
        if current_task.is_none() && suggested_response.is_none() {
            return Ok(());
        }

        let now = updated_at.unwrap_or_else(Utc::now).to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT INTO om_continuation_state(
                    scope_key, canonical_thread_id, current_task, suggested_response,
                    confidence, source_kind, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(scope_key, canonical_thread_id) DO UPDATE SET
                    current_task=COALESCE(excluded.current_task, om_continuation_state.current_task),
                    suggested_response=COALESCE(excluded.suggested_response, om_continuation_state.suggested_response),
                    confidence=CASE
                        WHEN excluded.current_task IS NULL AND excluded.suggested_response IS NULL
                        THEN om_continuation_state.confidence
                        ELSE excluded.confidence
                    END,
                    source_kind=CASE
                        WHEN excluded.current_task IS NULL AND excluded.suggested_response IS NULL
                        THEN om_continuation_state.source_kind
                        ELSE excluded.source_kind
                    END,
                    updated_at=excluded.updated_at
                ",
                params![
                    scope_key,
                    canonical_thread_id,
                    current_task,
                    suggested_response,
                    confidence,
                    source_kind,
                    now
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn list_om_continuation_states(
        &self,
        scope_key: &str,
    ) -> Result<Vec<OmContinuationState>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT canonical_thread_id, current_task, suggested_response
                FROM om_continuation_state
                WHERE scope_key = ?1
                ORDER BY updated_at DESC, confidence DESC, canonical_thread_id ASC
                ",
            )?;
            let rows = stmt.query_map(params![scope_key], |row| {
                Ok(OmContinuationState {
                    canonical_thread_id: row.get(0)?,
                    current_task: row.get(1)?,
                    suggested_response: row.get(2)?,
                })
            })?;
            let mut out = Vec::<OmContinuationState>::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub(crate) fn resolve_om_continuation_state(
        &self,
        scope_key: &str,
        preferred_thread_id: Option<&str>,
    ) -> Result<Option<OmContinuationState>> {
        let preferred_thread_id = preferred_thread_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let states = self.list_om_continuation_states(scope_key)?;
        if states.is_empty() {
            return Ok(None);
        }
        if let Some(preferred_thread_id) = preferred_thread_id
            && let Some(state) = states
                .iter()
                .find(|state| state.canonical_thread_id == preferred_thread_id)
        {
            return Ok(Some(state.clone()));
        }
        Ok(states.into_iter().next())
    }

    pub(crate) fn list_om_active_entries(&self, scope_key: &str) -> Result<Vec<OmActiveEntry>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT entry_id, canonical_thread_id, priority, text, origin_kind, created_at
                FROM om_entries
                WHERE scope_key = ?1
                  AND superseded_by IS NULL
                ORDER BY created_at DESC, entry_id ASC
                ",
            )?;
            let rows = stmt.query_map(params![scope_key], |row| {
                let created_at_raw = row.get::<_, String>(5)?;
                Ok(OmActiveEntry {
                    entry_id: row.get(0)?,
                    canonical_thread_id: row.get(1)?,
                    priority: row.get(2)?,
                    text: row.get(3)?,
                    origin_kind: row.get(4)?,
                    created_at: super::parse_required_rfc3339(5, &created_at_raw)?,
                })
            })?;
            let mut out = Vec::<OmActiveEntry>::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }
}

fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}
