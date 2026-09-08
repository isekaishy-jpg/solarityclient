//! Optional aggregate timing for the resident camera's collision providers.

use std::time::{Duration, Instant};

pub(super) struct CameraProfile {
    started: Instant,
    count: u64,
    totals: [Duration; 5],
    maxima: [Duration; 5],
}

impl CameraProfile {
    pub(super) fn from_environment() -> Option<Self> {
        std::env::var_os("SOLARITY_FRAME_TIMINGS").map(|_| Self {
            started: Instant::now(),
            count: 0,
            totals: [Duration::ZERO; 5],
            maxima: [Duration::ZERO; 5],
        })
    }

    pub(super) fn record(&mut self, sample: [Duration; 5]) {
        self.count += 1;
        for (index, value) in sample.into_iter().enumerate() {
            self.totals[index] += value;
            self.maxima[index] = self.maxima[index].max(value);
        }
        if self.started.elapsed() < Duration::from_secs(2) {
            return;
        }
        let mean = self
            .totals
            .map(|total| total.as_secs_f64() * 1e6 / self.count as f64);
        tracing::info!(
            resolutions = self.count,
            terrain_mean_us = mean[0],
            world_model_mean_us = mean[1],
            m2_mean_us = mean[2],
            liquid_ray_mean_us = mean[3],
            camera_volume_mean_us = mean[4],
            maximum_us = ?self.maxima.map(|value| value.as_micros()),
            "profiled camera collision providers"
        );
        self.started = Instant::now();
        self.count = 0;
        self.totals = [Duration::ZERO; 5];
        self.maxima = [Duration::ZERO; 5];
    }
}

pub(super) fn measure<T>(
    sample: &mut Option<[Duration; 5]>,
    index: usize,
    query: impl FnOnce() -> T,
) -> T {
    let started = sample.as_ref().map(|_| Instant::now());
    let result = query();
    if let (Some(sample), Some(started)) = (sample, started) {
        sample[index] += started.elapsed();
    }
    result
}
