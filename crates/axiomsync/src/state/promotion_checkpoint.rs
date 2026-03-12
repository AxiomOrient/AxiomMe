use chrono::Utc;
use rusqlite::{OptionalExtension, params, types::Type};

use crate::error::Result;

use super::SqliteStateStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromotionCheckpointPhase {
    Pending,
    Applying,
    Applied,
}

impl PromotionCheckpointPhase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Applying => "applying",
            Self::Applied => "applied",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(Self::Pending),
            "applying" => Some(Self::Applying),
            "applied" => Some(Self::Applied),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromotionCheckpointRecord {
    pub(crate) session_id: String,
    pub(crate) checkpoint_id: String,
    pub(crate) request_hash: String,
    pub(crate) request_json: String,
    pub(crate) phase: PromotionCheckpointPhase,
    pub(crate) result_json: Option<String>,
    pub(crate) applied_at: Option<String>,
    pub(crate) attempt_count: u32,
    pub(crate) updated_at: String,
}

impl SqliteStateStore {
    pub(crate) fn insert_promotion_checkpoint_pending(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        request_hash: &str,
        request_json: &str,
    ) -> Result<()> {
        self.with_conn(|conn| {
            let now = Utc::now().to_rfc3339();
            conn.execute(
                r"
                INSERT OR IGNORE INTO memory_promotion_checkpoints(
                    session_id, checkpoint_id, request_hash, request_json,
                    phase, result_json, applied_at, attempt_count, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, 0, ?6)
                ",
                params![
                    session_id,
                    checkpoint_id,
                    request_hash,
                    request_json,
                    PromotionCheckpointPhase::Pending.as_str(),
                    now
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn get_promotion_checkpoint(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<Option<PromotionCheckpointRecord>> {
        self.with_conn(|conn| {
            conn.query_row(
                r"
                SELECT
                    session_id,
                    checkpoint_id,
                    request_hash,
                    request_json,
                    phase,
                    result_json,
                    applied_at,
                    attempt_count,
                    updated_at
                FROM memory_promotion_checkpoints
                WHERE session_id = ?1 AND checkpoint_id = ?2
                ",
                params![session_id, checkpoint_id],
                |row| {
                    let phase_raw = row.get::<_, String>(4)?;
                    let phase = PromotionCheckpointPhase::parse(&phase_raw).ok_or_else(|| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("invalid promotion checkpoint phase: {phase_raw}"),
                            )),
                        )
                    })?;
                    Ok(PromotionCheckpointRecord {
                        session_id: row.get(0)?,
                        checkpoint_id: row.get(1)?,
                        request_hash: row.get(2)?,
                        request_json: row.get(3)?,
                        phase,
                        result_json: row.get(5)?,
                        applied_at: row.get(6)?,
                        attempt_count: i64_to_u32_saturating(row.get::<_, i64>(7)?),
                        updated_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub(crate) fn claim_promotion_checkpoint_applying(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        request_hash: &str,
    ) -> Result<bool> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                r"
                UPDATE memory_promotion_checkpoints
                SET
                    phase = ?1,
                    attempt_count = attempt_count + 1,
                    updated_at = ?2
                WHERE
                    session_id = ?3
                    AND checkpoint_id = ?4
                    AND request_hash = ?5
                    AND phase = ?6
                ",
                params![
                    PromotionCheckpointPhase::Applying.as_str(),
                    Utc::now().to_rfc3339(),
                    session_id,
                    checkpoint_id,
                    request_hash,
                    PromotionCheckpointPhase::Pending.as_str()
                ],
            )?;
            Ok(affected > 0)
        })
    }

    pub(crate) fn finalize_promotion_checkpoint_applied(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        request_hash: &str,
        result_json: &str,
    ) -> Result<bool> {
        self.with_conn(|conn| {
            let now = Utc::now().to_rfc3339();
            let affected = conn.execute(
                r"
                UPDATE memory_promotion_checkpoints
                SET
                    phase = ?1,
                    result_json = ?2,
                    applied_at = ?3,
                    updated_at = ?3
                WHERE
                    session_id = ?4
                    AND checkpoint_id = ?5
                    AND request_hash = ?6
                    AND phase = ?7
                ",
                params![
                    PromotionCheckpointPhase::Applied.as_str(),
                    result_json,
                    now,
                    session_id,
                    checkpoint_id,
                    request_hash,
                    PromotionCheckpointPhase::Applying.as_str()
                ],
            )?;
            Ok(affected > 0)
        })
    }

    pub(crate) fn set_promotion_checkpoint_pending(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        request_hash: &str,
    ) -> Result<bool> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                r"
                UPDATE memory_promotion_checkpoints
                SET
                    phase = ?1,
                    updated_at = ?2
                WHERE
                    session_id = ?3
                    AND checkpoint_id = ?4
                    AND request_hash = ?5
                    AND phase = ?6
                ",
                params![
                    PromotionCheckpointPhase::Pending.as_str(),
                    Utc::now().to_rfc3339(),
                    session_id,
                    checkpoint_id,
                    request_hash,
                    PromotionCheckpointPhase::Applying.as_str()
                ],
            )?;
            Ok(affected > 0)
        })
    }

    pub(crate) fn demote_stale_promotion_checkpoint(
        &self,
        session_id: &str,
        checkpoint_id: &str,
        stale_before: &str,
    ) -> Result<bool> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                r"
                UPDATE memory_promotion_checkpoints
                SET
                    phase = ?1,
                    updated_at = ?2
                WHERE
                    session_id = ?3
                    AND checkpoint_id = ?4
                    AND phase = ?5
                    AND updated_at <= ?6
                ",
                params![
                    PromotionCheckpointPhase::Pending.as_str(),
                    Utc::now().to_rfc3339(),
                    session_id,
                    checkpoint_id,
                    PromotionCheckpointPhase::Applying.as_str(),
                    stale_before
                ],
            )?;
            Ok(affected > 0)
        })
    }

    pub(crate) fn remove_promotion_checkpoints_for_session(
        &self,
        session_id: &str,
    ) -> Result<usize> {
        self.with_conn(|conn| {
            let affected = conn.execute(
                "DELETE FROM memory_promotion_checkpoints WHERE session_id = ?1",
                params![session_id],
            )?;
            Ok(affected)
        })
    }
}

fn i64_to_u32_saturating(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::state::SqliteStateStore;

    #[test]
    fn promotion_checkpoint_claim_is_single_winner() {
        let temp = tempdir().expect("tempdir");
        let store = SqliteStateStore::open(temp.path().join("state.db")).expect("open");
        store
            .insert_promotion_checkpoint_pending("s-1", "cp-1", "hash-a", r#"{"facts":[]}"#)
            .expect("insert");
        assert!(
            store
                .claim_promotion_checkpoint_applying("s-1", "cp-1", "hash-a")
                .expect("claim 1")
        );
        assert!(
            !store
                .claim_promotion_checkpoint_applying("s-1", "cp-1", "hash-a")
                .expect("claim 2")
        );
    }
}
