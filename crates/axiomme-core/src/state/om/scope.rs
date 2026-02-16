use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::error::Result;

use super::{OmThreadState, SqliteStateStore};

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
}
