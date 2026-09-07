//! Bounded logging for sampled frames; slow frames do not write individually.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const REPORT_INTERVAL: Duration = Duration::from_secs(2);
const SLOW_FRAME: Duration = Duration::from_millis(5);

thread_local! {
    /// Application scopes are main-thread owned; workers get independent windows.
    static WINDOWS: RefCell<BTreeMap<&'static str, ProfileWindow>> = const { RefCell::new(BTreeMap::new()) };
}

/// Adds a sample without formatting, and publishes at most one report per interval.
pub(super) fn record(
    label: &'static str,
    now: Instant,
    elapsed: Duration,
    cycles: Option<u64>,
    phases: &[(&'static str, Duration)],
    tail: Duration,
) {
    WINDOWS.with_borrow_mut(|windows| {
        let window = windows
            .entry(label)
            .or_insert_with(|| ProfileWindow::new(now));
        if window.record(now, elapsed, cycles, phases, tail) {
            window.report(label);
            window.reset(now);
        }
    });
}

/// Timings for one named phase, including paths that omit it on some frames.
#[derive(Default)]
struct PhaseTotal {
    samples: u64,
    elapsed: Duration,
    maximum: Duration,
}

/// Retains cumulative timings and the worst complete frame in the current window.
struct ProfileWindow {
    started: Instant,
    samples: u64,
    elapsed: Duration,
    maximum: Duration,
    slow_frames: u64,
    cycles: u128,
    cycle_samples: u64,
    tail: Duration,
    phases: BTreeMap<&'static str, PhaseTotal>,
    worst_phases: [(&'static str, Duration); 16],
    worst_phase_count: usize,
}

impl ProfileWindow {
    /// Creates the scope once; resets retain the phase map's allocations.
    fn new(now: Instant) -> Self {
        Self {
            started: now,
            samples: 0,
            elapsed: Duration::ZERO,
            maximum: Duration::ZERO,
            slow_frames: 0,
            cycles: 0,
            cycle_samples: 0,
            tail: Duration::ZERO,
            phases: BTreeMap::new(),
            worst_phases: [("", Duration::ZERO); 16],
            worst_phase_count: 0,
        }
    }

    /// Accounts every frame and reports readiness solely from the wall-clock window.
    fn record(
        &mut self,
        now: Instant,
        elapsed: Duration,
        cycles: Option<u64>,
        phases: &[(&'static str, Duration)],
        tail: Duration,
    ) -> bool {
        self.samples += 1;
        self.elapsed += elapsed;
        self.slow_frames += u64::from(elapsed >= SLOW_FRAME);
        self.tail += tail;
        if let Some(cycles) = cycles {
            self.cycles += u128::from(cycles);
            self.cycle_samples += 1;
        }
        if elapsed >= self.maximum {
            self.maximum = elapsed;
            self.worst_phase_count = phases.len().min(self.worst_phases.len());
            self.worst_phases[..self.worst_phase_count]
                .copy_from_slice(&phases[..self.worst_phase_count]);
        }
        for &(label, elapsed) in phases {
            let phase = self.phases.entry(label).or_default();
            phase.samples += 1;
            phase.elapsed += elapsed;
            phase.maximum = phase.maximum.max(elapsed);
        }
        now.duration_since(self.started) >= REPORT_INTERVAL
    }

    /// Formats only at publication; counts expose how much the summary represents.
    fn report(&self, label: &str) {
        let phases = self
            .phases
            .iter()
            .filter(|(_, phase)| phase.samples != 0)
            .map(|(&label, phase)| {
                (
                    label,
                    phase.elapsed.as_secs_f64() * 1e6 / phase.samples as f64,
                    phase.maximum.as_secs_f64() * 1e6,
                )
            })
            .collect::<Vec<_>>();
        let cpu_cycles_mean =
            (self.cycle_samples != 0).then(|| self.cycles as f64 / self.cycle_samples as f64);
        tracing::info!(
            scope = label,
            frames = self.samples,
            slow_frames = self.slow_frames,
            mean_ms = self.elapsed.as_secs_f64() * 1000. / self.samples as f64,
            maximum_ms = self.maximum.as_secs_f64() * 1000.,
            ?cpu_cycles_mean,
            phase_mean_max_us = ?phases,
            worst_frame_phases = ?&self.worst_phases[..self.worst_phase_count],
            tail_mean_us = self.tail.as_secs_f64() * 1e6 / self.samples as f64,
            "profiled application frame interval"
        );
    }

    /// Starts another interval without discarding known scope/phase storage.
    fn reset(&mut self, now: Instant) {
        self.started = now;
        self.samples = 0;
        self.elapsed = Duration::ZERO;
        self.maximum = Duration::ZERO;
        self.slow_frames = 0;
        self.cycles = 0;
        self.cycle_samples = 0;
        self.tail = Duration::ZERO;
        self.worst_phase_count = 0;
        for phase in self.phases.values_mut() {
            *phase = PhaseTotal::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sustained slow frames must retain outliers without logging once per frame.
    #[test]
    fn sustained_slow_frames_produce_bounded_reports_with_phase_outliers() {
        let start = Instant::now();
        let mut window = ProfileWindow::new(start);
        let mut reports = 0;
        let mut reported_samples = 0;
        for frame in 1..=10_000 {
            let now = start + Duration::from_millis(frame * 8);
            let elapsed = Duration::from_millis(if frame % 250 == 0 { 40 } else { 8 });
            let phases = [
                ("prepare", Duration::from_millis(2)),
                ("present", elapsed - Duration::from_millis(2)),
            ];
            if window.record(now, elapsed, Some(100), &phases, Duration::ZERO) {
                assert_eq!(window.maximum, Duration::from_millis(40));
                assert_eq!(window.slow_frames, window.samples);
                assert_eq!(window.worst_phases[1].1, Duration::from_millis(38));
                assert_eq!(window.phases["prepare"].maximum, Duration::from_millis(2));
                assert_eq!(window.cycles, u128::from(window.samples) * 100);
                reported_samples += window.samples;
                reports += 1;
                window.reset(now);
            }
        }
        assert_eq!(reports, 40);
        assert_eq!(reported_samples, 10_000);
        assert_eq!(window.phases.len(), 2);
    }
}
