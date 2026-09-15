//! Fixed-size aggregation and sampling policy for retired GPU timestamps.

#![allow(unsafe_code)]

use std::time::Duration;

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
#[derive(Clone, Copy)]
pub(in crate::device) struct GpuFrameProfiler {
    clock: TimestampClock,
}

impl GpuFrameProfiler {
    /// Records a UI-only submission after its existing frame-slot fence.
    pub(in crate::device) fn record_ui(&self, sample: super::GpuTimestampRead<2>) {
        match sample {
            GpuTimestampRead::Ready {
                generation,
                timestamps,
                trace,
            } => {
                static UI: solarity_profiling::Site =
                    solarity_profiling::Site::new("rendering.gpu.ui_only", true);
                trace.gpu_duration(
                    "UI",
                    Duration::from_secs_f64(
                        self.clock.elapsed_ms(timestamps[0], timestamps[1]) / 1000.,
                    ),
                );
                UI.duration(
                    generation,
                    "",
                    Duration::from_secs_f64(
                        self.clock.elapsed_ms(timestamps[0], timestamps[1]) / 1000.,
                    ),
                );
            }
            GpuTimestampRead::Unavailable { generation }
                if generation == solarity_profiling::generation() =>
            {
                solarity_profiling::profile_value!("rendering.gpu_timestamp_unavailable", 1);
            }
            _ => {}
        }
    }

    /// Queries timestamp capability once; query pools remain lazy and commands stay off until F10.
    pub(in crate::device) fn new_if_supported(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
        graphics_family: u32,
    ) -> Result<Option<Self>, VulkanError> {
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
            return Ok(None);
        }
        Ok(Some(Self {
            clock: TimestampClock {
                period_ns,
                mask: u64::MAX >> (64 - bits),
            },
        }))
    }

    /// Shares the live frame detail cadence with CPU probes, independent of swapchain slots.
    pub(in crate::device::vulkan_world_frame) fn sample_next(&mut self) -> bool {
        solarity_profiling::detail_enabled()
    }

    /// Accounts a retired sample without waiting if the driver reports not-ready.
    pub(in crate::device::vulkan_world_frame) fn record(
        &mut self,
        timestamps: GpuTimestampRead,
        extent: (u32, u32),
    ) {
        let _ = extent;
        match timestamps {
            GpuTimestampRead::Idle => {}
            GpuTimestampRead::Unavailable { generation } => {
                if solarity_profiling::generation() == generation {
                    solarity_profiling::profile_value!("rendering.gpu_timestamp_unavailable", 1);
                }
            }
            GpuTimestampRead::Ready {
                generation,
                timestamps,
                trace,
            } => {
                static GPU: solarity_profiling::Site =
                    solarity_profiling::Site::new("rendering.gpu", true);
                for (index, phase) in PHASES.iter().enumerate() {
                    let milliseconds = self
                        .clock
                        .elapsed_ms(timestamps[index], timestamps[index + 1]);
                    trace.gpu_duration(phase, Duration::from_secs_f64(milliseconds / 1000.));
                    GPU.duration(
                        generation,
                        phase,
                        Duration::from_secs_f64(milliseconds / 1000.),
                    );
                }
                let total = self
                    .clock
                    .elapsed_ms(timestamps[0], timestamps[QUERY_COUNT - 1]);
                GPU.duration(generation, "", Duration::from_secs_f64(total / 1000.));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/gpu_profile.rs"]
mod tests;
