use std::thread;
use std::time::Duration;

use anyhow::Result;
use axiomme_core::AxiomMe;

#[derive(Debug, serde::Serialize, Default)]
pub(super) struct QueueWorkReport {
    mode: String,
    iterations: u32,
    fetched: usize,
    processed: usize,
    done: usize,
    dead_letter: usize,
    requeued: usize,
    skipped: usize,
}

pub(super) fn run_queue_worker(
    app: &AxiomMe,
    iterations: u32,
    limit: usize,
    sleep_ms: u64,
    include_dead_letter: bool,
    stop_when_idle: bool,
) -> Result<QueueWorkReport> {
    let mut total = QueueWorkReport::default();
    for i in 0..iterations {
        let report = app.replay_outbox(limit, include_dead_letter)?;
        total.iterations = i + 1;
        if total.mode.is_empty() {
            total.mode = "work".to_string();
        }
        total.fetched += report.fetched;
        total.processed += report.processed;
        total.done += report.done;
        total.dead_letter += report.dead_letter;
        total.requeued += report.requeued;
        total.skipped += report.skipped;

        if stop_when_idle && report.fetched == 0 {
            break;
        }
        if i + 1 < iterations {
            thread::sleep(Duration::from_millis(sleep_ms));
        }
    }
    Ok(total)
}

pub(super) fn run_queue_daemon(
    app: &AxiomMe,
    max_cycles: u32,
    limit: usize,
    sleep_ms: u64,
    include_dead_letter: bool,
    stop_when_idle: bool,
    idle_cycles: u32,
) -> Result<QueueWorkReport> {
    let mut total = QueueWorkReport {
        mode: "daemon".to_string(),
        ..QueueWorkReport::default()
    };
    let mut idle_streak = 0u32;
    let mut cycle = 0u32;

    loop {
        if max_cycles > 0 && cycle >= max_cycles {
            break;
        }
        cycle += 1;

        let report = app.replay_outbox(limit, include_dead_letter)?;
        total.iterations = cycle;
        total.fetched += report.fetched;
        total.processed += report.processed;
        total.done += report.done;
        total.dead_letter += report.dead_letter;
        total.requeued += report.requeued;
        total.skipped += report.skipped;

        if report.fetched == 0 {
            idle_streak = idle_streak.saturating_add(1);
        } else {
            idle_streak = 0;
        }
        if stop_when_idle && idle_streak >= idle_cycles.max(1) {
            break;
        }

        thread::sleep(Duration::from_millis(sleep_ms));
    }

    Ok(total)
}
