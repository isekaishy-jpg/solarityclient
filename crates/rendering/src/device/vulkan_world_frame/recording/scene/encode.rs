//! Secondary buffers inherit attachment formats, but explicitly establish all draw state.

#![allow(unsafe_code)]

use super::command::{SceneBindings, SceneCommand};
use crate::VulkanError;
use ash::{Device, vk};

/// Dynamic-rendering inheritance is immutable throughout an admitted scene.
#[derive(Clone, Copy)]
pub(super) struct SceneInheritance {
    pub(super) color: vk::Format,
    pub(super) depth: vk::Format,
    pub(super) viewport: vk::Viewport,
    pub(super) scissor: vk::Rect2D,
}

/// One bounded contiguous range from the stock command order.
#[derive(Default)]
pub(in crate::device::vulkan_world_frame) struct SceneJob {
    pub(super) index: usize,
    pub(super) device: Option<Device>,
    pub(super) command: vk::CommandBuffer,
    pub(super) inheritance: Option<SceneInheritance>,
    pub(super) draws: solarity_cpu::CpuBuffer<SceneCommand>,
    pub(super) measurement: solarity_cpu::WorkMeasurement,
    pub(super) result: Option<Result<(), VulkanError>>,
}

impl SceneJob {
    /// Only Vulkan recording runs here; resource lookup and ordered fog publication are complete.
    pub(super) fn execute(
        &mut self,
        context: &solarity_cpu::JobContext<'_>,
    ) -> solarity_cpu::JobOutcome {
        let _profile = solarity_profiling::profile!("rendering.scene.record");
        let _cycles = solarity_profiling::profile_cycles!("rendering.scene.record_cpu");
        context.diagnostic_value("rendering.scene.commands", self.draws.len() as u64);
        let started = self.measurement.start();
        let result = self.record();
        let outcome = if result.is_ok() {
            self.measurement.finish(started);
            solarity_cpu::JobOutcome::Succeeded
        } else {
            solarity_cpu::JobOutcome::Failed
        };
        self.result = Some(result);
        outcome
    }

    /// The pending frame pins resources until every recording job has returned.
    fn record(&self) -> Result<(), VulkanError> {
        let device = self
            .device
            .as_ref()
            .unwrap_or_else(|| unreachable!("admitted scene job owns device functions"));
        let inheritance = self
            .inheritance
            .unwrap_or_else(|| unreachable!("admitted scene job owns attachment inheritance"));
        begin(device, self.command, inheritance)?;
        let mut bindings = SceneBindings::default();
        for draw in self.draws.iter() {
            bindings.record(device, self.command, draw);
        }
        end(device, self.command)
    }
}

/// Each buffer owns a separate pool, retired by the parent primary's slot fence.
pub(super) fn begin(
    device: &Device,
    command: vk::CommandBuffer,
    state: SceneInheritance,
) -> Result<(), VulkanError> {
    let colors = [state.color];
    let mut rendering = vk::CommandBufferInheritanceRenderingInfo::default()
        .color_attachment_formats(&colors)
        .depth_attachment_format(state.depth)
        .stencil_attachment_format(state.depth)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let inheritance = vk::CommandBufferInheritanceInfo::default().push_next(&mut rendering);
    let begin = vk::CommandBufferBeginInfo::default()
        .flags(
            vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT
                | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE,
        )
        .inheritance_info(&inheritance);
    // SAFETY: Its pool is reset after slot retirement; formats match the active
    // dynamic-rendering attachments and every world pipeline. No inherited state is assumed.
    unsafe {
        device
            .begin_command_buffer(command, &begin)
            .map_err(|error| VulkanError::operation("begin scene recording", error))?;
        device.cmd_set_viewport(command, 0, &[state.viewport]);
        device.cmd_set_scissor(command, 0, &[state.scissor]);
    }
    Ok(())
}

/// All draws and local queries have finished before the primary may reference this buffer.
pub(in crate::device::vulkan_world_frame) fn end(
    device: &Device,
    command: vk::CommandBuffer,
) -> Result<(), VulkanError> {
    // SAFETY: This owner has begun the exclusively held secondary command buffer.
    unsafe { device.end_command_buffer(command) }
        .map_err(|error| VulkanError::operation("end scene recording", error))
}
