//! Queue ownership stays on main after every required recording range completes.

#![allow(unsafe_code)]

use super::super::WorldFrameContext;
use super::super::resource::WorldFrameSlot;
use crate::VulkanError;
use crate::device::vulkan_frame::swapchain_error;
use ash::vk;

/// Optional CPU timings around the two queue calls at the submission boundary.
pub(in crate::device::vulkan_world_frame) struct WorldSubmitTimings {
    pub(in crate::device::vulkan_world_frame) queue_submit: std::time::Duration,
    pub(in crate::device::vulkan_world_frame) queue_present: std::time::Duration,
}

/// Marks timestamp ownership only after submission succeeds; presentation failure
/// still leaves its queries protected by the submitted slot fence.
pub(in crate::device::vulkan_world_frame) fn submit_and_present(
    context: &WorldFrameContext<'_>,
    slot: &mut WorldFrameSlot,
    present_semaphore: vk::Semaphore,
    image_index: u32,
    profile: bool,
    gpu_sample: bool,
    shadows: &super::super::recording::ShadowSubmission,
) -> Result<Option<WorldSubmitTimings>, VulkanError> {
    let waits = [vk::SemaphoreSubmitInfo::default()
        .semaphore(slot.image_available())
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
    let mut commands = [vk::CommandBufferSubmitInfo::default(); 6];
    for (entry, command) in commands.iter_mut().zip(&shadows.commands[..shadows.count]) {
        *entry = entry.command_buffer(*command);
    }
    commands[shadows.count] = commands[shadows.count].command_buffer(slot.command_buffer());
    commands[shadows.count + 1] =
        commands[shadows.count + 1].command_buffer(slot.post_command_buffer());
    let signals = [vk::SemaphoreSubmitInfo::default()
        .semaphore(present_semaphore)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
    let submit = vk::SubmitInfo2::default()
        .wait_semaphore_infos(&waits)
        .command_buffer_infos(&commands[..shadows.count + 2])
        .signal_semaphore_infos(&signals);
    slot.reset_fence(context.device)?;
    let queue_submit_started = profile.then(std::time::Instant::now);
    // SAFETY: Command and synchronization resources live through fence retirement.
    if let Err(source) = unsafe {
        context
            .device
            .queue_submit2(context.graphics_queue, &[submit], slot.fence())
    } {
        slot.restore_signaled_fence(context.device)?;
        return Err(VulkanError::operation("submit world frame", source));
    }
    slot.gpu_timestamps.submitted(gpu_sample);
    let queue_submit = queue_submit_started.map(|started| started.elapsed());
    let wait = [present_semaphore];
    let swapchains = [context.swapchain];
    let indices = [image_index];
    let present = vk::PresentInfoKHR::default()
        .wait_semaphores(&wait)
        .swapchains(&swapchains)
        .image_indices(&indices);
    let queue_present_started = profile.then(std::time::Instant::now);
    // SAFETY: Presentation waits for this submission's signal.
    unsafe {
        context
            .swapchain_loader
            .queue_present(context.present_queue, &present)
    }
    .map_err(|source| swapchain_error("present world frame", source))?;
    Ok(queue_submit
        .zip(queue_present_started)
        .map(|(queue_submit, queue_present_started)| WorldSubmitTimings {
            queue_submit,
            queue_present: queue_present_started.elapsed(),
        }))
}
