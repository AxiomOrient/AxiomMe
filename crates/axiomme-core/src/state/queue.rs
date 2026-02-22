use chrono::{Duration, Utc};
use rusqlite::{OptionalExtension, Row, params};

use crate::error::Result;
use crate::models::{
    OutboxEvent, QueueCheckpoint, QueueCounts, QueueDeadLetterRate, QueueLaneStatus, QueueStatus,
};

use super::SqliteStateStore;
use super::queue_lane::{LANE_EMBEDDING, LANE_SEMANTIC, lane_for_event_type};

impl SqliteStateStore {
    pub fn enqueue(
        &self,
        event_type: &str,
        uri: &str,
        payload_json: impl serde::Serialize,
    ) -> Result<i64> {
        self.enqueue_with_status(event_type, uri, payload_json, "new", 0)
    }

    pub fn enqueue_dead_letter(
        &self,
        event_type: &str,
        uri: &str,
        payload_json: impl serde::Serialize,
    ) -> Result<i64> {
        self.enqueue_with_status(event_type, uri, payload_json, "dead_letter", 1)
    }

    fn enqueue_with_status(
        &self,
        event_type: &str,
        uri: &str,
        payload_json: impl serde::Serialize,
        status: &str,
        attempt_count: u32,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let lane = lane_for_event_type(event_type);
        let payload_json = serde_json::to_value(payload_json)?.to_string();

        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT INTO outbox(event_type, uri, payload_json, created_at, status, attempt_count, next_attempt_at, lane)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?4, ?7)
                ",
                params![
                    event_type,
                    uri,
                    payload_json,
                    now,
                    status,
                    i64::from(attempt_count),
                    lane
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn fetch_outbox(&self, status: &str, limit: usize) -> Result<Vec<OutboxEvent>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT id, event_type, uri, payload_json, status, attempt_count, next_attempt_at
                FROM outbox
                WHERE status = ?1
                  AND (?1 <> 'new' OR COALESCE(next_attempt_at, created_at) <= ?3)
                ORDER BY id ASC
                LIMIT ?2
                ",
            )?;

            let rows = stmt.query_map(
                params![status, usize_to_i64_saturating(limit), now],
                outbox_event_from_row,
            )?;

            let mut events = Vec::new();
            for event in rows {
                events.push(event?);
            }
            Ok(events)
        })
    }

    pub fn get_outbox_event(&self, id: i64) -> Result<Option<OutboxEvent>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT id, event_type, uri, payload_json, status, attempt_count, next_attempt_at
                FROM outbox
                WHERE id = ?1
                ",
            )?;

            let row = stmt
                .query_row(params![id], outbox_event_from_row)
                .optional()?;
            Ok(row)
        })
    }

    #[cfg(test)]
    pub(crate) fn update_outbox_payload_json(
        &self,
        id: i64,
        payload_json: &serde_json::Value,
    ) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE outbox SET payload_json = ?1 WHERE id = ?2",
                params![payload_json.to_string(), id],
            )?;
            Ok(())
        })
    }

    pub fn mark_outbox_status(&self, id: i64, status: &str, increment_attempt: bool) -> Result<()> {
        self.with_conn(|conn| {
            if status == "processing" {
                let now = Utc::now().to_rfc3339();
                if increment_attempt {
                    conn.execute(
                        "UPDATE outbox SET status = ?1, attempt_count = attempt_count + 1, next_attempt_at = ?3 WHERE id = ?2",
                        params![status, id, now],
                    )?;
                } else {
                    conn.execute(
                        "UPDATE outbox SET status = ?1, next_attempt_at = ?3 WHERE id = ?2",
                        params![status, id, now],
                    )?;
                }
            } else if increment_attempt {
                conn.execute(
                    "UPDATE outbox SET status = ?1, attempt_count = attempt_count + 1 WHERE id = ?2",
                    params![status, id],
                )?;
            } else {
                conn.execute(
                    "UPDATE outbox SET status = ?1 WHERE id = ?2",
                    params![status, id],
                )?;
            }
            Ok(())
        })
    }

    pub fn recover_timed_out_processing_events(&self, timeout_seconds: i64) -> Result<u64> {
        let stale_before = (Utc::now() - Duration::seconds(timeout_seconds.max(0))).to_rfc3339();
        self.with_conn(|conn| {
            let affected = conn.execute(
                r"
                UPDATE outbox
                SET status = 'new'
                WHERE status = 'processing'
                  AND COALESCE(next_attempt_at, created_at) <= ?1
                ",
                params![stale_before],
            )?;
            Ok(i64_to_u64_saturating(
                i64::try_from(affected).unwrap_or(i64::MAX),
            ))
        })
    }

    pub fn requeue_outbox_with_delay(&self, id: i64, delay_seconds: i64) -> Result<()> {
        let next_attempt = (Utc::now() + Duration::seconds(delay_seconds.max(0))).to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE outbox SET status = 'new', next_attempt_at = ?1 WHERE id = ?2",
                params![next_attempt, id],
            )?;
            Ok(())
        })
    }

    pub fn force_outbox_due_now(&self, id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE outbox SET next_attempt_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            Ok(())
        })
    }

    #[cfg(test)]
    pub(crate) fn set_outbox_next_attempt_at_for_test(
        &self,
        id: i64,
        next_attempt_at: &str,
    ) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE outbox SET next_attempt_at = ?1 WHERE id = ?2",
                params![next_attempt_at, id],
            )?;
            Ok(())
        })
    }

    pub fn get_checkpoint(&self, worker_name: &str) -> Result<Option<i64>> {
        self.with_conn(|conn| {
            let value = conn
                .query_row(
                    "SELECT last_event_id FROM queue_checkpoint WHERE worker_name = ?1",
                    params![worker_name],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            Ok(value)
        })
    }

    pub fn set_checkpoint(&self, worker_name: &str, last_event_id: i64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT INTO queue_checkpoint(worker_name, last_event_id, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(worker_name) DO UPDATE SET
                  last_event_id=excluded.last_event_id,
                  updated_at=excluded.updated_at
                ",
                params![worker_name, last_event_id, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    pub fn start_reconcile_run(&self, run_id: &str) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r"
                INSERT OR REPLACE INTO reconcile_runs(run_id, started_at, drift_count, status)
                VALUES (?1, ?2, 0, 'running')
                ",
                params![run_id, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    pub fn finish_reconcile_run(
        &self,
        run_id: &str,
        drift_count: usize,
        status: &str,
    ) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                r"
                UPDATE reconcile_runs
                SET ended_at = ?2, drift_count = ?3, status = ?4
                WHERE run_id = ?1
                ",
                params![
                    run_id,
                    Utc::now().to_rfc3339(),
                    usize_to_i64_saturating(drift_count),
                    status
                ],
            )?;
            Ok(())
        })
    }

    pub fn queue_snapshot(&self) -> Result<(QueueCounts, QueueStatus)> {
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            let mut counts = QueueCounts::default();
            let mut semantic = QueueLaneStatus::default();
            let mut embedding = QueueLaneStatus::default();

            let mut stmt = conn.prepare(
                r"
                WITH normalized AS (
                    SELECT
                        CASE
                            WHEN lane = ?1 THEN ?1
                            WHEN lane = ?2 THEN ?2
                            ELSE ?3
                        END AS lane_norm,
                        status,
                        COALESCE(next_attempt_at, created_at) AS due_at
                    FROM outbox
                )
                SELECT
                    lane_norm,
                    SUM(CASE WHEN status = 'new' THEN 1 ELSE 0 END) AS new_total,
                    SUM(CASE WHEN status = 'new' AND due_at <= ?4 THEN 1 ELSE 0 END) AS new_due,
                    SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END) AS processing,
                    SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) AS done,
                    SUM(CASE WHEN status = 'dead_letter' THEN 1 ELSE 0 END) AS dead_letter,
                    MIN(CASE WHEN status = 'new' THEN due_at ELSE NULL END) AS earliest_new_due_at
                FROM normalized
                GROUP BY lane_norm
                ",
            )?;
            let rows = stmt.query_map(
                params![LANE_SEMANTIC, LANE_EMBEDDING, LANE_SEMANTIC, now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )?;

            for row in rows {
                let (lane, new_total, new_due, processing, done, dead_letter, earliest_new_due_at) =
                    row?;
                let lane_totals = LaneTotals {
                    new_total: i64_to_u64_saturating(new_total),
                    new_due: i64_to_u64_saturating(new_due),
                    processing: i64_to_u64_saturating(processing),
                    done: i64_to_u64_saturating(done),
                    dead_letter: i64_to_u64_saturating(dead_letter),
                };

                apply_lane_totals(&mut counts, &lane_totals, earliest_new_due_at);
                if lane == LANE_EMBEDDING {
                    apply_lane_status(&mut embedding, &lane_totals);
                } else {
                    apply_lane_status(&mut semantic, &lane_totals);
                }
            }

            Ok((
                counts,
                QueueStatus {
                    semantic,
                    embedding,
                },
            ))
        })
    }

    pub fn queue_status(&self) -> Result<QueueStatus> {
        let (_, status) = self.queue_snapshot()?;
        Ok(status)
    }

    pub fn queue_counts(&self) -> Result<QueueCounts> {
        let (counts, _) = self.queue_snapshot()?;
        Ok(counts)
    }

    pub fn queue_dead_letter_rates_by_event_type(&self) -> Result<Vec<QueueDeadLetterRate>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT
                    event_type,
                    COUNT(*) AS total,
                    SUM(CASE WHEN status = 'dead_letter' THEN 1 ELSE 0 END) AS dead_letter
                FROM outbox
                GROUP BY event_type
                ORDER BY event_type ASC
                ",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;

            let mut out = Vec::<QueueDeadLetterRate>::new();
            for row in rows {
                let (event_type, total_raw, dead_letter_raw) = row?;
                let total = i64_to_u64_saturating(total_raw);
                let dead_letter = i64_to_u64_saturating(dead_letter_raw);
                let dead_letter_rate = if total == 0 {
                    0.0
                } else {
                    ratio_u64(dead_letter, total)
                };
                out.push(QueueDeadLetterRate {
                    event_type,
                    total,
                    dead_letter,
                    dead_letter_rate,
                });
            }
            Ok(out)
        })
    }

    pub fn list_checkpoints(&self) -> Result<Vec<QueueCheckpoint>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT worker_name, last_event_id, updated_at FROM queue_checkpoint ORDER BY worker_name ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(QueueCheckpoint {
                    worker_name: row.get(0)?,
                    last_event_id: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            })?;

            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct LaneTotals {
    new_total: u64,
    new_due: u64,
    processing: u64,
    done: u64,
    dead_letter: u64,
}

fn apply_lane_totals(
    counts: &mut QueueCounts,
    lane_totals: &LaneTotals,
    earliest_new_due_at: Option<String>,
) {
    counts.new_total += lane_totals.new_total;
    counts.new_due += lane_totals.new_due;
    counts.processing += lane_totals.processing;
    counts.done += lane_totals.done;
    counts.dead_letter += lane_totals.dead_letter;

    if let Some(candidate) = earliest_new_due_at {
        let replace = counts
            .earliest_next_attempt_at
            .as_ref()
            .is_none_or(|current| candidate < *current);
        if replace {
            counts.earliest_next_attempt_at = Some(candidate);
        }
    }
}

const fn apply_lane_status(status: &mut QueueLaneStatus, lane_totals: &LaneTotals) {
    status.new_total += lane_totals.new_total;
    status.new_due += lane_totals.new_due;
    status.processing += lane_totals.processing;
    status.processed += lane_totals.done;
    status.error_count += lane_totals.dead_letter;
}

fn ratio_u64(numer: u64, denom: u64) -> f64 {
    if denom == 0 {
        return 0.0;
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "queue metrics require ratio output and tolerate precision loss"
    )]
    {
        numer as f64 / denom as f64
    }
}

fn outbox_event_from_row(row: &Row<'_>) -> rusqlite::Result<OutboxEvent> {
    let payload = row.get::<_, String>(3)?;
    let payload_json =
        serde_json::from_str::<serde_json::Value>(&payload).unwrap_or(serde_json::Value::Null);
    Ok(OutboxEvent {
        id: row.get(0)?,
        event_type: row.get(1)?,
        uri: row.get(2)?,
        payload_json,
        status: row.get(4)?,
        attempt_count: i64_to_u32_saturating(row.get::<_, i64>(5)?),
        next_attempt_at: row.get(6)?,
    })
}

fn i64_to_u32_saturating(value: i64) -> u32 {
    if value <= 0 {
        0
    } else {
        u32::try_from(value).unwrap_or(u32::MAX)
    }
}

fn i64_to_u64_saturating(value: i64) -> u64 {
    if value <= 0 {
        0
    } else {
        u64::try_from(value).unwrap_or(u64::MAX)
    }
}

fn usize_to_i64_saturating(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
