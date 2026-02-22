use std::num::NonZeroU32;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use axiomme_core::AxiomMe;
use axiomme_core::models::ReplayReport;

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum QueueRunMode {
    Work,
    Daemon,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct QueueWorkReport {
    mode: QueueRunMode,
    iterations: u32,
    fetched: usize,
    processed: usize,
    done: usize,
    dead_letter: usize,
    requeued: usize,
    skipped: usize,
}

impl QueueWorkReport {
    const fn new(mode: QueueRunMode) -> Self {
        Self {
            mode,
            iterations: 0,
            fetched: 0,
            processed: 0,
            done: 0,
            dead_letter: 0,
            requeued: 0,
            skipped: 0,
        }
    }

    fn absorb_replay(&mut self, report: &ReplayReport) {
        self.fetched += report.fetched;
        self.processed += report.processed;
        self.done += report.done;
        self.dead_letter += report.dead_letter;
        self.requeued += report.requeued;
        self.skipped += report.skipped;
    }
}

pub(super) fn run_queue_worker(
    app: &AxiomMe,
    iterations: u32,
    limit: usize,
    sleep_ms: u64,
    include_dead_letter: bool,
    stop_when_idle: bool,
) -> Result<QueueWorkReport> {
    let mut total = QueueWorkReport::new(QueueRunMode::Work);
    for i in 0..iterations {
        let report = app.replay_outbox(limit, include_dead_letter)?;
        total.iterations = i + 1;
        total.absorb_replay(&report);

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
    let mut total = QueueWorkReport::new(QueueRunMode::Daemon);
    let max_cycles = non_zero_u32(max_cycles);
    let mut idle_streak = 0u32;
    let mut cycle = 0u32;

    loop {
        if let Some(max_cycles) = max_cycles
            && cycle >= max_cycles.get()
        {
            break;
        }
        cycle += 1;

        let report = app.replay_outbox(limit, include_dead_letter)?;
        total.iterations = cycle;
        total.absorb_replay(&report);

        if report.fetched == 0 {
            idle_streak = idle_streak.saturating_add(1);
        } else {
            idle_streak = 0;
        }
        if stop_when_idle && idle_streak >= idle_cycles {
            break;
        }

        thread::sleep(Duration::from_millis(sleep_ms));
    }

    Ok(total)
}

fn non_zero_u32(value: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(value)
}
