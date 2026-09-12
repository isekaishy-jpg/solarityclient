//! Fixed-size aggregation and sampling policy for retired GPU timestamps.

#![allow(unsafe_code)]

use std::time::{Duration, Instant};

use ash::{Instance, vk};

use crate::device::VulkanError;

use super::query::GpuTimestampRead;

/// Boundaries follow submission order, preserving interleaved M2/effect queues.
pub(super) const PHASES: [&str; 10] = [
    "shadows",
    "uploads and sky",
    "terrain",
    "WMO",
    "ground detail",
    "opaque liquids",
    "M2 and transparent effects",
    "screen effects",
    "UI",
    "present transition and capture",
];
/// One start timestamp followed by each phase's completion timestamp.
pub(in crate::device::vulkan_world_frame) const QUERY_COUNT: usize = PHASES.len() + 1;
/// Sparse sampling bounds driver work without changing normal frame submission.
const SAMPLE_INTERVAL: u64 = 16;

/// Queue timestamp units and wrapping width, validated before enabling queries.
#[derive(Clone, Copy)]
struct TimestampClock {
    period_ns: f64,
    mask: u64,
}

impl TimestampClock {
    /// Converts modulo queue ticks without subtracting large floating-point epochs.
    fn elapsed_ms(self, start: u64, end: u64) -> f64 {
        (end.wrapping_sub(start) & self.mask) as f64 * self.period_ns / 1_000_000.0
    }
}

/// Fixed-size aggregation for completed samples; no per-draw or per-frame logs.
pub(in crate::device) struct GpuFrameProfiler {
    clock: TimestampClock,
    frame: u64,
    started: Instant,
    samples: u64,
    unavailable: u64,
    sums_ms: [f64; PHASES.len()],
    maxima_ms: [f64; PHASES.len()],
    total_ms: f64,
    maximum_ms: f64,
    worst_ms: [f64; PHASES.len()],
}

impl GpuFrameProfiler {
    /// Queries device capabilities once, only for an explicit diagnostic run.
    pub(in crate::device) fn from_environment(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
        graphics_family: u32,
    ) -> Result<Option<Self>, VulkanError> {
        if std::env::var_os("SOLARITY_GPU_TIMINGS").is_none() {
            return Ok(None);
        }
        // SAFETY: The selected physical device belongs to the live instance.
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        // SAFETY: Same physical-device lifetime; this is a read-only startup query.
        let families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let bits = families
            .get(graphics_family as usize)
            .map_or(0, |family| family.timestamp_valid_bits);
        let period_ns = f64::from(properties.limits.timestamp_period);
        if !(1..=64).contains(&bits) || !period_ns.is_finite() || period_ns <= 0.0 {
            return Err(VulkanError::operation(
                "enable GPU timings",
                "graphics queue timestamp clock is unavailable",
            ));
        }
        tracing::info!(
            sample_interval = SAMPLE_INTERVAL,
            timestamp_valid_bits = bits,
            timestamp_period_ns = period_ns,
            "enabled sampled GPU frame timings"
        );
        Ok(Some(Self {
            clock: TimestampClock {
                period_ns,
                mask: u64::MAX >> (64 - bits),
            },
            frame: 0,
            started: Instant::now(),
            samples: 0,
            unavailable: 0,
            sums_ms: [0.0; PHASES.len()],
            maxima_ms: [0.0; PHASES.len()],
            total_ms: 0.0,
            maximum_ms: 0.0,
            worst_ms: [0.0; PHASES.len()],
        }))
    }

    /// Selects one complete frame in sixteen, independently of swapchain slot count.
    pub(in crate::device::vulkan_world_frame) fn sample_next(&mut self) -> bool {
        let sample = self.frame.is_multiple_of(SAMPLE_INTERVAL);
        self.frame = self.frame.wrapping_add(1);
        sample
    }

    /// Accounts a retired sample without waiting if the driver reports not-ready.
    pub(in crate::device::vulkan_world_frame) fn record(
        &mut self,
        timestamps: GpuTimestampRead,
        extent: (u32, u32),
    ) {
        if matches!(timestamps, GpuTimestampRead::Idle) {
            return;
        }
        if let GpuTimestampRead::Ready(timestamps) = timestamps {
            let phases = std::array::from_fn::<_, { PHASES.len() }, _>(|index| {
                self.clock
                    .elapsed_ms(timestamps[index], timestamps[index + 1])
            });
            let total = self
                .clock
                .elapsed_ms(timestamps[0], timestamps[QUERY_COUNT - 1]);
            self.samples += 1;
            self.total_ms += total;
            for (index, elapsed) in phases.iter().copied().enumerate() {
                self.sums_ms[index] += elapsed;
                self.maxima_ms[index] = self.maxima_ms[index].max(elapsed);
            }
            if total > self.maximum_ms {
                self.maximum_ms = total;
                self.worst_ms = phases;
            }
        } else {
            self.unavailable += 1;
        }
        if self.started.elapsed() < Duration::from_secs(2) {
            return;
        }
        let divisor = self.samples.max(1) as f64;
        let phases = std::array::from_fn::<_, { PHASES.len() }, _>(|index| {
            (
                PHASES[index],
                self.sums_ms[index] / divisor,
                self.maxima_ms[index],
            )
        });
        tracing::info!(samples = self.samples, unavailable = self.unavailable,
            sample_interval = SAMPLE_INTERVAL, width = extent.0, height = extent.1,
            mean_ms = self.total_ms / divisor, maximum_ms = self.maximum_ms,
            phase_mean_max_ms = ?phases, worst_frame_phases_ms = ?self.worst_ms,
            "profiled sampled GPU world frame");
        self.started = Instant::now();
        self.samples = 0;
        self.unavailable = 0;
        self.sums_ms.fill(0.0);
        self.maxima_ms.fill(0.0);
        self.total_ms = 0.0;
        self.maximum_ms = 0.0;
        self.worst_ms.fill(0.0);
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/gpu_profile.rs"]
mod tests;
