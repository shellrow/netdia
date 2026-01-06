use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Throttles high-frequency progress updates from port scan workers.
#[derive(Debug)]
pub struct ThrottledProgress {
    /// Total number of items to process.
    total: u32,
    /// Number of completed items.
    done: AtomicU32,
    /// Last `done` value for which the caller emitted an event.
    last_emitted: AtomicU32,
    /// Timestamp of the last emitted event.
    last_emit_at: Mutex<Instant>,
    /// Minimum time between events.
    min_interval: Duration,
    /// Emit at least every `step` increments.
    step: u32,
}

impl ThrottledProgress {
    pub fn new(total: u32) -> Self {
        // Aim at most ~100 updates (1% resolution), but never step 0.
        let step = (total / 100).max(1);

        Self {
            total,
            done: AtomicU32::new(0),
            last_emitted: AtomicU32::new(0),
            last_emit_at: Mutex::new(Instant::now()),
            // Do not spam UI more often than this even if step is small.
            min_interval: Duration::from_millis(80),
            step,
        }
    }

    /// Mark one item as finished.
    pub fn on_advance(&self) -> (u32, bool) {
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;

        // Always allow the final event to go through.
        if done >= self.total {
            self.last_emitted.store(done, Ordering::Relaxed);
            return (done, true);
        }

        let last = self.last_emitted.load(Ordering::Relaxed);
        let advanced_enough = done.saturating_sub(last) >= self.step;

        let mut last_ts = self
            .last_emit_at
            .lock()
            .expect("ThrottledProgress::last_emit_at poisoned");
        let elapsed = last_ts.elapsed();
        let time_ok = elapsed >= self.min_interval;

        let should_emit = advanced_enough || time_ok;
        if should_emit {
            self.last_emitted.store(done, Ordering::Relaxed);
            *last_ts = Instant::now();
        }

        (done, should_emit)
    }
}
